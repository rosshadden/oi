use super::*;

impl<'a> Translator<'a> {
	// `if`/`else` lowered to branch&merge, yielding value of the chosen branch.
	// A diverging branch contributes nothing to the merge.
	// If all branches diverge, returns None.
	pub(super) fn conditional(
		&mut self,
		cond: &Spanned<Expr>,
		then: &[Spanned<Expr>],
		els: Option<&[Spanned<Expr>]>,
		span: Span,
	) -> Result<Option<(Value, Typ)>, Diagnostic> {
		let (cv, ct) = self.expr(cond)?;
		if ct != Typ::Bool {
			return Err(
				Diagnostic::new(format!("`if` condition must be Bool, got {ct}"), cond.1.into_range())
					.with_label("not a Bool"),
			);
		}

		let then_block = self.b.create_block();
		let else_block = self.b.create_block();
		self.b.ins().brif(cv, then_block, &[], else_block, &[]);
		self.b.seal_block(then_block);
		self.b.seal_block(else_block);

		// result var and merge block are created on the first non-diverging branch
		let mut result: Option<Variable> = None;
		let mut result_typ: Option<Typ> = None;
		let mut merge: Option<Block> = None;

		// branch-local bindings must not leak into the enclosing scope
		let saved = self.vars.clone();

		self.b.switch_to_block(then_block);
		let then_flow = self.block(then)?;
		self.vars = saved.clone();
		if let Some((v, t)) = then_flow {
			let var = self.b.declare_var(cl_type(&t, self.int));
			self.b.def_var(var, v);
			let m = self.b.create_block();
			self.b.ins().jump(m, &[]);
			result = Some(var);
			result_typ = Some(t);
			merge = Some(m);
		}

		self.b.switch_to_block(else_block);
		let else_flow = match els {
			Some(els) => self.block(els)?,
			None => {
				let t = result_typ.clone().unwrap_or(Typ::Tuple(vec![]));
				let z = self.zero(&t);
				Some((z, t))
			}
		};
		self.vars = saved;
		if let Some((v, t)) = else_flow {
			match &result_typ {
				Some(rt) if rt != &t => {
					return Err(Diagnostic::new(
						format!("`if` branches have mismatched types: {rt} and {t}"),
						span.into_range(),
					)
					.with_label("both branches must yield the same type"));
				}
				Some(_) => self.b.def_var(result.unwrap(), v),
				None => {
					let var = self.b.declare_var(cl_type(&t, self.int));
					self.b.def_var(var, v);
					result = Some(var);
					result_typ = Some(t);
				}
			}
			let m = merge.unwrap_or_else(|| self.b.create_block());
			self.b.ins().jump(m, &[]);
			merge = Some(m);
		}

		match merge {
			Some(m) => {
				self.b.switch_to_block(m);
				self.b.seal_block(m);
				let typ = result_typ.unwrap();
				Ok(Some((self.b.use_var(result.unwrap()), typ)))
			}
			None => Ok(None),
		}
	}

	// `match`
	// first arm wins.
	pub(super) fn match_expr(
		&mut self,
		subject: &Spanned<Expr>,
		arms: &[MatchArm],
		else_body: Option<&[Spanned<Expr>]>,
		span: Span,
	) -> Result<Option<(Value, Typ)>, Diagnostic> {
		let (sv, st) = self.expr(subject)?;
		let sv_var = self.b.declare_var(cl_type(&st, self.int));
		self.b.def_var(sv_var, sv);

		// ensure match covers every variant when applicable
		if matches!(&st, Typ::Enum(_) | Typ::Option(_)) {
			let pats = || arms.iter().flat_map(|a| &a.patterns);
			let catch_all = else_body.is_some() || pats().any(|p| matches!(&p.0, Expr::Ident(w) if w == "_"));
			if !catch_all {
				let variants = self.variants_of(&st);
				let covered = pats()
					.map(|p| self.enum_pattern(p, &st).map(|(d, _)| d))
					.collect::<Result<Vec<_>, _>>()?;
				let missing: Vec<_> = variants
					.iter()
					.filter(|v| !covered.contains(&v.disc))
					.map(|v| v.name.clone())
					.collect();
				if !missing.is_empty() {
					let msg = format!("non-exhaustive match, missing: {}", missing.join(", "));
					return Err(
						Diagnostic::new(msg, span.into_range()).with_label("cover these variants or add `else`")
					);
				}
			}
		}

		let merge = self.b.create_block();
		let mut result: Option<(Variable, Typ)> = None;

		// pre-create each arm's entry block so each arm knows where to fall through to on failure
		let arm_entries: Vec<Block> = arms.iter().map(|_| self.b.create_block()).collect();
		let else_blk = self.b.create_block();
		self.b.ins().jump(arm_entries.first().copied().unwrap_or(else_blk), &[]);

		for (i, arm) in arms.iter().enumerate() {
			let arm_body = self.b.create_block();
			let fail = arm_entries.get(i + 1).copied().unwrap_or(else_blk);

			self.b.switch_to_block(arm_entries[i]);
			self.b.seal_block(arm_entries[i]);

			// bindings
			let mut binds = vec![];
			for (j, pat) in arm.patterns.iter().enumerate() {
				let eq = if matches!(&pat.0, Expr::Ident(w) if w == "_") {
					// `_` wildcard
					self.b.ins().iconst(types::I8, 1)
				} else if let Expr::Range { start, end } = &pat.0 {
					let sv = self.b.use_var(sv_var);
					self.range_pattern(sv, &st, start.as_deref(), end.as_deref(), pat.1)?
				} else if matches!(&st, Typ::Enum(_) | Typ::Option(_)) {
					let (disc, b) = self.enum_pattern(pat, &st)?;
					if arm.patterns.len() == 1 {
						binds = b;
					}
					let sv = self.b.use_var(sv_var);
					let variants = self.variants_of(&st);
					let tag = self.enum_tag(&variants, sv);
					let disc = self.b.ins().iconst(self.int, disc);
					self.b.ins().icmp(IntCC::Equal, tag, disc)
				} else if let (Typ::Tuple(fields), Expr::Tuple(elems)) = (&st, &pat.0) {
					if elems.len() != fields.len() {
						let msg = format!(
							"tuple pattern has {} elements, subject has {}",
							elems.len(),
							fields.len()
						);
						return Err(Diagnostic::new(msg, pat.1.into_range()).with_label("arity mismatch"));
					}
					if arm.patterns.len() == 1 {
						let pairs = elems.iter().zip(fields).map(|((_, e), (_, t))| (e, t));
						binds = field_binds(pairs, 0, 8)?;
					}
					self.b.ins().iconst(types::I8, 1)
				} else if let (Typ::Struct(sname, fdefs), Expr::StructLit { name: pname, fields }) = (&st, &pat.0) {
					if arm.patterns.len() == 1 {
						binds = struct_pattern(fdefs, pname, sname, fields, pat.1)?;
					}
					self.b.ins().iconst(types::I8, 1)
				} else if let (Typ::Array(elem) | Typ::FixedArray(elem, _), Expr::Array(elems)) = (&st, &pat.0) {
					if arm.patterns.len() == 1 {
						let pairs = elems.iter().map(|e| (e, elem.as_ref()));
						binds = field_binds(pairs, 0, elem_size(elem) as i32)?;
					}
					let sv = self.b.use_var(sv_var);
					let (_, len) = self.array_parts(sv, &st);
					let count = self.b.ins().iconst(self.int, elems.len() as i64);
					self.b.ins().icmp(IntCC::Equal, len, count)
				} else {
					let sv = self.b.use_var(sv_var);
					let (pv, pt) = self.check_expr(pat, &st)?;
					if pt != st {
						return Err(Diagnostic::new(
							format!("match pattern ({pt}) does not match subject ({st})"),
							pat.1.into_range(),
						)
						.with_label("type mismatch"));
					}
					self.emit_eq(sv, pv, &st)
				};
				if j + 1 < arm.patterns.len() {
					let next = self.b.create_block();
					self.b.ins().brif(eq, arm_body, &[], next, &[]);
					self.b.seal_block(next);
					self.b.switch_to_block(next);
				} else {
					self.b.ins().brif(eq, arm_body, &[], fail, &[]);
				}
			}

			self.b.seal_block(arm_body);
			self.b.switch_to_block(arm_body);
			let saved = self.vars.clone();
			if let Some(name) = &arm.binding {
				let local = Local {
					var: sv_var,
					typ: st.clone(),
					mutable: false,
				};
				self.vars.insert(name.clone(), local);
			}
			let sv = self.b.use_var(sv_var);
			let base = match &st {
				Typ::Array(_) | Typ::FixedArray(..) => self.array_parts(sv, &st).0,
				_ => sv,
			};
			for (name, typ, off) in &binds {
				let cl = cl_type(typ, self.int);
				let fv = self.b.ins().load(cl, MemFlags::new(), base, *off);
				let var = self.b.declare_var(cl);
				self.b.def_var(var, fv);
				let local = Local {
					var,
					typ: typ.clone(),
					mutable: false,
				};
				self.vars.insert(name.clone(), local);
			}
			let flow = self.block(&arm.body)?;
			self.vars = saved;
			if let Some(vt) = flow {
				self.match_contribute(vt, &mut result, merge, span)?;
			}
		}

		self.b.switch_to_block(else_blk);
		self.b.seal_block(else_blk);
		let else_flow = if let Some(els) = else_body {
			let saved = self.vars.clone();
			let flow = self.block(els)?;
			self.vars = saved;
			flow
		} else {
			let t = result.as_ref().map_or(Typ::Tuple(vec![]), |(_, t)| t.clone());
			Some((self.zero(&t), t))
		};
		if let Some(vt) = else_flow {
			self.match_contribute(vt, &mut result, merge, span)?;
		}

		Ok(if let Some((var, typ)) = result {
			self.b.switch_to_block(merge);
			self.b.seal_block(merge);
			Some((self.b.use_var(var), typ))
		} else {
			None
		})
	}

	// Write (v, t) into the shared result variable and jump to `merge`.
	// All arms must agree on type. The first arm declares the variable.
	pub(super) fn match_contribute(
		&mut self,
		(v, t): (Value, Typ),
		result: &mut Option<(Variable, Typ)>,
		merge: Block,
		span: Span,
	) -> Result<(), Diagnostic> {
		match result {
			Some((_, rt)) if rt != &t => Err(Diagnostic::new(
				format!("`match` arms have mismatched types: {rt} and {t}"),
				span.into_range(),
			)
			.with_label("all arms must yield the same type")),
			Some((var, _)) => {
				self.b.def_var(*var, v);
				self.b.ins().jump(merge, &[]);
				Ok(())
			}
			None => {
				let var = self.b.declare_var(cl_type(&t, self.int));
				self.b.def_var(var, v);
				self.b.ins().jump(merge, &[]);
				*result = Some((var, t));
				Ok(())
			}
		}
	}

	pub(super) fn loop_expr(
		&mut self,
		cond: Option<&Spanned<Expr>>,
		body: &[Spanned<Expr>],
	) -> Result<Option<(Value, Typ)>, Diagnostic> {
		let top = self.b.create_block();
		self.b.ins().jump(top, &[]);
		self.b.switch_to_block(top);

		// a conditional loop branches at the top: into the body or out to exit
		let exit = match cond {
			Some(cond) => {
				let (cv, ct) = self.expr(cond)?;
				if ct != Typ::Bool {
					return Err(Diagnostic::new(
						format!("`loop` condition must be Bool, got {ct}"),
						cond.1.into_range(),
					)
					.with_label("not a Bool"));
				}
				let body_block = self.b.create_block();
				let exit = self.b.create_block();
				self.b.ins().brif(cv, body_block, &[], exit, &[]);
				self.b.seal_block(body_block);
				self.b.switch_to_block(body_block);
				Some(exit)
			}
			None => None,
		};

		self.loops.push(LoopFrame { top, exit });
		// bindings inside the loop must not leak past it
		let saved = self.vars.clone();
		let flow = self.block(body)?;
		self.vars = saved;
		let frame = self.loops.pop().expect("loop frame");

		if flow.is_some() {
			self.b.ins().jump(top, &[]);
		}
		self.b.seal_block(top);

		match frame.exit {
			Some(exit) => {
				self.b.switch_to_block(exit);
				self.b.seal_block(exit);
				Ok(Some((self.b.ins().iconst(self.int, 0), Typ::Tuple(vec![]))))
			}
			// an infinite loop with no `break` never falls through
			None => Ok(None),
		}
	}

	pub(super) fn for_loop(
		&mut self,
		pat: &Pattern,
		iter: &Spanned<Expr>,
		body: &[Spanned<Expr>],
		span: Span,
	) -> Result<(Value, Typ), Diagnostic> {
		let (val, typ) = self.expr(iter)?;
		// counter var, upper bound, and (data ptr, elem type) for array iteration
		let (counter, limit, arr_src): (_, _, Option<(Value, Typ)>) = match typ {
			Typ::Range => {
				let cl = cl_int_for_width(32);
				let start = self.b.ins().load(cl, MemFlags::new(), val, 0);
				let end = self.b.ins().load(cl, MemFlags::new(), val, 8);
				let v = self.b.declare_var(cl);
				self.b.def_var(v, start);
				(v, end, None)
			}
			Typ::Array(elem) => {
				let zero = self.b.ins().iconst(self.int, 0);
				let len = self.array_len(val);
				let data = self.array_data(val);
				let v = self.b.declare_var(self.int);
				self.b.def_var(v, zero);
				(v, len, Some((data, *elem)))
			}
			_ => {
				return Err(
					Diagnostic::new(format!("cannot iterate over {typ}"), iter.1.into_range())
						.with_label("not iterable"),
				);
			}
		};

		let (header, body_block, latch, exit) = (
			self.b.create_block(),
			self.b.create_block(),
			self.b.create_block(),
			self.b.create_block(),
		);
		self.b.ins().jump(header, &[]);

		self.b.switch_to_block(header);
		let iv = self.b.use_var(counter);
		let more = self.b.ins().icmp(IntCC::SignedLessThan, iv, limit);
		self.b.ins().brif(more, body_block, &[], exit, &[]);
		self.b.seal_block(body_block);

		self.b.switch_to_block(body_block);
		let iv = self.b.use_var(counter);
		let (val, typ) = match &arr_src {
			None => (iv, Typ::Int(32)),
			Some((data, elem)) => {
				let off = self.b.ins().imul_imm(iv, elem_size(elem));
				let addr = self.b.ins().iadd(*data, off);
				(
					self.b.ins().load(cl_type(elem, self.int), MemFlags::new(), addr, 0),
					elem.clone(),
				)
			}
		};
		let saved = self.vars.clone();
		self.bind_pattern(pat, val, &typ, span)?;
		self.loops.push(LoopFrame {
			top: latch,
			exit: Some(exit),
		});
		let flow = self.block(body)?;
		self.vars = saved;
		self.loops.pop().expect("loop frame");

		if flow.is_some() {
			self.b.ins().jump(latch, &[]);
		}
		self.b.seal_block(latch);
		self.b.seal_block(exit);

		self.b.switch_to_block(latch);
		let iv = self.b.use_var(counter);
		let next = self.b.ins().iadd_imm(iv, 1);
		self.b.def_var(counter, next);
		self.b.ins().jump(header, &[]);
		self.b.seal_block(header);

		self.b.switch_to_block(exit);
		Ok((self.b.ins().iconst(self.int, 0), Typ::Tuple(vec![])))
	}

	pub(super) fn bind_pattern(&mut self, pat: &Pattern, val: Value, typ: &Typ, span: Span) -> Result<(), Diagnostic> {
		match pat {
			Pattern::Name(name) => {
				let var = self.b.declare_var(cl_type(typ, self.int));
				self.b.def_var(var, val);
				self.vars.insert(
					name.clone(),
					Local {
						var,
						typ: typ.clone(),
						mutable: false,
					},
				);
			}
			Pattern::Tuple(names) => {
				let Typ::Tuple(fields) = typ else {
					return Err(Diagnostic::new(
						format!("cannot destructure {typ} with a tuple pattern"),
						span.into_range(),
					)
					.with_label("not a tuple"));
				};
				if names.len() != fields.len() {
					return Err(Diagnostic::new(
						format!(
							"pattern binds {} names but the tuple has {} fields",
							names.len(),
							fields.len()
						),
						span.into_range(),
					)
					.with_label("wrong number of fields"));
				}
				for (i, (name, (_, ftyp))) in names.iter().zip(fields).enumerate() {
					let fv = self.b.ins().load(cl_type(ftyp, self.int), MemFlags::new(), val, (i * 8) as i32);
					let var = self.b.declare_var(cl_type(ftyp, self.int));
					self.b.def_var(var, fv);
					self.vars.insert(
						name.clone(),
						Local {
							var,
							typ: ftyp.clone(),
							mutable: false,
						},
					);
				}
			}
		}
		Ok(())
	}
}
