use super::*;

impl<'a> Translator<'a> {
	// Evaluate a block of statements, returning the final value.
	// Returns None if the block diverged (every path returned/broke).
	pub fn block(&mut self, stmts: &[Spanned<Expr>]) -> Result<Option<(Value, Typ)>, Diagnostic> {
		self.block_tail(stmts, None)
	}

	// Coerces a bare trailing expression against `tail`.
	pub fn block_tail(
		&mut self,
		stmts: &[Spanned<Expr>],
		tail: Option<&Typ>,
	) -> Result<Option<(Value, Typ)>, Diagnostic> {
		let mut last = (self.b.ins().iconst(self.int, 0), Typ::Tuple(vec![]));
		for (i, stmt) in stmts.iter().enumerate() {
			let stmt_target = if i + 1 == stmts.len() { tail } else { None };
			match &stmt.0 {
				Expr::Bind {
					mutable,
					name,
					typ,
					value,
				} => {
					let annot = typ.as_ref().map(|(t, span)| self.types().resolve(t, *span)).transpose()?;
					let (val, typ) = match (value, annot) {
						(Some(value), Some(target)) => match self.coerce_lit(value, &target)? {
							Some(val) => (val, target),
							None => {
								let (val, found) = self.check_expr(value, &target)?;
								if found != target {
									return Err(Diagnostic::new(
										format!("expected {target}, got {found}"),
										value.1.into_range(),
									)
									.with_label("does not match the declared type"));
								}
								(val, target)
							}
						},
						(Some(value), None) => self.expr(value)?,
						(None, Some(target)) => (self.zero(&target), target),
						(None, None) => unreachable!("binding has neither a type nor a value"),
					};
					let (final_val, cl) = match &typ {
						Typ::Struct(_, fields) => (self.struct_copy(val, fields), self.int),
						Typ::FixedArray(elem, n) => (self.fixed_copy(val, elem, *n), self.int),
						_ => (val, self.b.func.dfg.value_type(val)),
					};
					// `:=` always declares a fresh binding, shadowing any earlier ones
					let var = self.b.declare_var(cl);
					self.b.def_var(var, final_val);
					self.vars.insert(name.clone(), Local::plain(var, typ, *mutable));
				}

				Expr::Assign { name, value } => {
					let local = self.mutable_local(name, stmt.1.into_range(), Mutation::Assign)?;
					let (val, typ) = self.check_expr(value, &local.typ)?;
					if typ != local.typ {
						return Err(Diagnostic::new(
							format!("cannot assign {typ} to `{name}`, which is {}", local.typ),
							value.1.into_range(),
						)
						.with_label("type mismatch"));
					}
					if let Typ::Struct(_, ref fields) = typ {
						let fields = fields.clone();
						let dst = self.read_local(&local);
						for (i, f) in fields.iter().enumerate() {
							let cl = cl_type(&f.typ, self.int);
							let fv = self.b.ins().load(cl, MemFlags::new(), val, (i * 8) as i32);
							self.b.ins().store(MemFlags::new(), fv, dst, (i * 8) as i32);
						}
					} else {
						self.write_local(&local, val);
					}
				}

				Expr::IndexAssign { name, index, value } => {
					let local = self.mutable_local(name, stmt.1.into_range(), Mutation::IndexAssign)?;
					if let Typ::Map(k, v) = local.typ.clone() {
						let (k, v) = (*k, *v);
						let (tag, key_bits) = self.map_key(index, &k)?;
						let (val, vtyp) = self.check_expr(value, &v)?;
						if vtyp != v {
							return Err(Diagnostic::new(
								format!("cannot assign {vtyp} to {v} value of map"),
								value.1.into_range(),
							)
							.with_label("type mismatch"));
						}
						let ptr = self.read_local(&local);
						let val_bits = self.map_bits(val);
						self.call_map_set(ptr, tag, key_bits, val_bits);
						continue;
					}
					let elem = match &local.typ {
						Typ::Array(e) | Typ::FixedArray(e, _) => (**e).clone(),
						_ => {
							return Err(
								Diagnostic::new(format!("`{name}` is not an array"), stmt.1.into_range())
									.with_label("not an array"),
							);
						}
					};
					let ptr = self.read_local(&local);
					let idx = self.int_value(index, "array index")?;
					let idx = self.b.ins().sextend(self.int, idx);
					let (val, vtyp) = self.expr(value)?;
					if vtyp != elem {
						return Err(Diagnostic::new(
							format!("cannot assign {vtyp} to element of {elem} array"),
							value.1.into_range(),
						)
						.with_label("type mismatch"));
					}
					let (data, len) = self.array_parts(ptr, &local.typ);
					self.store_index(data, len, &elem, idx, val);
				}

				Expr::MapDelete { name, key } => {
					let local = self.mutable_local(name, stmt.1.into_range(), Mutation::IndexAssign)?;
					let Typ::Map(k, _) = local.typ.clone() else {
						return Err(Diagnostic::new(format!("`{name}` is not a map"), stmt.1.into_range())
							.with_label("not a map"));
					};
					let (tag, key_bits) = self.map_key(key, &k)?;
					let ptr = self.read_local(&local);
					self.call_map_delete(ptr, tag, key_bits);
				}

				Expr::Append { name, value } => {
					let local = self.mutable_local(name, stmt.1.into_range(), Mutation::Append)?;
					let elem = match &local.typ {
						Typ::Array(e) => (**e).clone(),
						_ => {
							return Err(
								Diagnostic::new(format!("`{name}` is not an array"), stmt.1.into_range())
									.with_label("not an array"),
							);
						}
					};
					let (val, vtyp) = self.expr(value)?;
					let size = self.b.ins().iconst(self.int, elem_size(&elem));
					let ptr = self.read_local(&local);

					if vtyp == elem {
						// grow if full, then write the new element and bump len
						let len = self.array_len(ptr);
						let cap = self.array_cap(ptr);
						let full = self.b.ins().icmp(IntCC::Equal, len, cap);
						let grow_block = self.b.create_block();
						let ok_block = self.b.create_block();
						self.b.ins().brif(full, grow_block, &[], ok_block, &[]);
						self.b.seal_block(grow_block);

						self.b.switch_to_block(grow_block);
						let min_cap = self.b.ins().iadd_imm(len, 1);
						let func = self.import_fn(runtime::ARRAY_RESERVE, &[self.int, self.int, self.int], None);
						self.b.ins().call(func, &[ptr, min_cap, size]);
						self.b.ins().jump(ok_block, &[]);
						self.b.seal_block(ok_block);

						self.b.switch_to_block(ok_block);
						let len = self.array_len(ptr);
						let data = self.array_data(ptr);
						let off = self.b.ins().imul_imm(len, elem_size(&elem));
						let addr = self.b.ins().iadd(data, off);
						self.b.ins().store(MemFlags::new(), val, addr, 0);
						let new_len = self.b.ins().iadd_imm(len, 1);
						self.b.ins().store(MemFlags::new(), new_len, ptr, 8);
					} else if vtyp == Typ::Array(Box::new(elem.clone())) {
						let func = self.import_fn(runtime::ARRAY_EXTEND, &[self.int, self.int, self.int], None);
						self.b.ins().call(func, &[ptr, val, size]);
					} else {
						return Err(Diagnostic::new(
							format!("cannot append {vtyp} to {elem} array"),
							value.1.into_range(),
						)
						.with_label("type mismatch"));
					}
				}

				Expr::Return(value) => {
					let (val, typ) = match value {
						Some(e) => match self.ret.clone() {
							Some((target, _)) => self.check_expr(e, &target)?,
							None => self.expr(e)?,
						},
						None => {
							let typ = self.ret.as_ref().map_or(Typ::Tuple(vec![]), |(t, _)| t.clone());
							(self.zero(&typ), typ)
						}
					};
					self.emit_return(val, typ, stmt.1)?;
					return Ok(None);
				}

				Expr::If { cond, then, els } => {
					match self.conditional(cond, then, els.as_deref(), stmt_target, stmt.1)? {
						Some((v, t)) => last = (v, t),
						None => return Ok(None),
					}
				}

				Expr::Match {
					subject,
					arms,
					else_body,
				} => match self.match_expr(subject, arms, else_body.as_deref(), stmt_target, stmt.1)? {
					Some((v, t)) => last = (v, t),
					None => return Ok(None),
				},

				Expr::Loop { cond, body } => match self.loop_expr(cond.as_deref(), body)? {
					Some((v, t)) => last = (v, t),
					None => return Ok(None),
				},

				// TODO: revisit after adding the Iterator trait
				Expr::For { pat, iter, body } => last = self.for_loop(pat, iter, body, stmt.1)?,

				Expr::FieldAssign { name, field, value } => {
					let local = self.mutable_local(name, stmt.1.into_range(), Mutation::FieldAssign)?;
					let fields = match &local.typ {
						Typ::Struct(_, fields) => fields.clone(),
						_ => {
							return Err(
								Diagnostic::new(format!("`{name}` is not a struct"), stmt.1.into_range())
									.with_label("not a struct"),
							);
						}
					};
					let idx = fields.iter().position(|f| &f.name == field).ok_or_else(|| {
						Diagnostic::new(format!("struct has no field `{field}`"), stmt.1.into_range())
							.with_label("no such field")
					})?;
					let (val, vtyp) = self.expr(value)?;
					if vtyp != fields[idx].typ {
						return Err(Diagnostic::new(
							format!("cannot assign {vtyp} to field `{field}` of type {}", fields[idx].typ),
							value.1.into_range(),
						)
						.with_label("type mismatch"));
					}
					let ptr = self.read_local(&local);
					self.b.ins().store(MemFlags::new(), val, ptr, (idx * 8) as i32);
				}

				Expr::Break => {
					let exit = match self.loops.last() {
						Some(frame) => frame.exit,
						None => {
							return Err(Diagnostic::new("`break` outside of a loop", stmt.1.into_range())
								.with_label("not inside a loop"));
						}
					};
					// the first `break` creates the exit block
					let exit = match exit {
						Some(exit) => exit,
						None => {
							let exit = self.b.create_block();
							self.loops.last_mut().unwrap().exit = Some(exit);
							exit
						}
					};
					self.b.ins().jump(exit, &[]);
					return Ok(None);
				}

				Expr::Continue => {
					let top = match self.loops.last() {
						Some(frame) => frame.top,
						None => {
							return Err(Diagnostic::new("`continue` outside of a loop", stmt.1.into_range())
								.with_label("not inside a loop"));
						}
					};
					self.b.ins().jump(top, &[]);
					return Ok(None);
				}

				Expr::Doc(_) => {}

				_ => {
					last = match stmt_target {
						Some(t) => self.check_expr(stmt, t)?,
						None => self.expr(stmt)?,
					}
				}
			}
		}
		Ok(Some(last))
	}

	// Autowrap return types.
	// idk whether it'll be more general in the future, but for now this is for `Option` and `Result`.
	fn autowrap_return(&mut self, val: Value, typ: Typ) -> (Value, Typ) {
		match self.ret.as_ref().map(|(t, _)| t.clone()) {
			Some(Typ::Option(inner)) if typ == *inner => {
				let v = self.make_enum(&option_variants(&inner), 1, &[val]);
				(v, Typ::Option(inner))
			}
			Some(Typ::Result(inner)) if typ == *inner => {
				let v = self.make_enum(&result_variants(&inner), 0, &[val]);
				(v, Typ::Result(inner))
			}
			Some(Typ::Result(inner)) if typ == Typ::Error => {
				let v = self.make_enum(&result_variants(&inner), 1, &[val]);
				(v, Typ::Result(inner))
			}
			_ => (val, typ),
		}
	}

	// The first return fixes the fn's type, and later returns must agree.
	pub fn emit_return(&mut self, val: Value, typ: Typ, span: Span) -> Result<(), Diagnostic> {
		let (val, typ) = self.autowrap_return(val, typ);
		if let Some((declared, _)) = &self.ret
			&& &typ != declared
		{
			return Err(Diagnostic::new(
				format!("expected {declared} return value, got {typ}"),
				span.into_range(),
			)
			.with_label("wrong return type"));
		}
		if matches!(typ, Typ::Tuple(ref f) if f.is_empty()) {
			self.b.ins().return_(&[]);
			if self.ret.is_none() {
				self.ret = Some((typ, span));
			}
			return Ok(());
		}
		// structs and fixed arrays live on the stack, so copy to heap before returning
		let final_val = match &typ {
			Typ::Struct(_, fields) => {
				let fields = fields.clone();
				let heap = self.call_alloc(fields.len());
				for (i, f) in fields.iter().enumerate() {
					let cl = cl_type(&f.typ, self.int);
					let fv = self.b.ins().load(cl, MemFlags::new(), val, (i * 8) as i32);
					self.b.ins().store(MemFlags::new(), fv, heap, (i * 8) as i32);
				}
				heap
			}
			Typ::FixedArray(elem, n) => {
				let (elem, n) = ((**elem).clone(), *n);
				let stride = elem_size(&elem);
				let cl = cl_type(&elem, self.int);
				let heap = self.call_alloc_bytes(n as i64 * stride);
				for i in 0..n {
					let off = (i as i64 * stride) as i32;
					let v = self.b.ins().load(cl, MemFlags::new(), val, off);
					self.b.ins().store(MemFlags::new(), v, heap, off);
				}
				heap
			}
			_ => val,
		};
		// the cranelift signature takes its return type from the first return
		if self.b.func.signature.returns.is_empty() {
			self.b.func.signature.returns.push(AbiParam::new(cl_type(&typ, self.int)));
		}
		self.b.ins().return_(&[final_val]);
		if self.ret.is_none() {
			self.ret = Some((typ, span));
		}
		Ok(())
	}
}
