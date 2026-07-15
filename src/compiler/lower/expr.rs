use super::*;

impl<'a> Translator<'a> {
	pub fn expr(&mut self, expr: &Spanned<Expr>) -> Result<(Value, Typ), Diagnostic> {
		match &expr.0 {
			Expr::Int(n) => {
				if (i32::MIN as i64..=i32::MAX as i64).contains(n) {
					Ok((self.b.ins().iconst(types::I32, *n), Typ::Int(32)))
				} else {
					Ok((self.b.ins().iconst(types::I64, *n), Typ::Int(64)))
				}
			}
			Expr::Bool(v) => Ok((self.b.ins().iconst(self.int, *v as i64), Typ::Bool)),
			Expr::Float(x) => Ok((self.b.ins().f64const(*x), Typ::Float(64))),
			Expr::String(s) => Ok((self.str_const(s), Typ::Str)),
			Expr::Atom(name) => Ok((self.atom_const(name), Typ::Atom)),

			Expr::EnumShorthand { variant, .. } => Err(Diagnostic::new(
				format!("cannot infer the enum type of `.{variant}` here"),
				expr.1.into_range(),
			)
			.with_label("no enum type is expected in this position")
			.with_note(format!("qualify it, e.g. `Color.{variant}`"))),

			Expr::None => Err(
				Diagnostic::new("cannot infer the type of `none` here", expr.1.into_range())
					.with_label("no `?T` type is expected in this position")
					.with_note("qualify it (ex: `?int(none)`)"),
			),

			Expr::OptionInit { inner: (te, span), arg } => {
				let inner_typ = self.types().resolve(te, *span)?;
				let variants = option_variants(&inner_typ);
				if matches!(arg.0, Expr::None) {
					let val = self.make_enum(&variants, 0, &[]);
					return Ok((val, Typ::Option(Box::new(inner_typ))));
				}
				let (fv, at) = self.check_expr(arg, &inner_typ)?;
				if at != inner_typ {
					return Err(
						Diagnostic::new(format!("expected {inner_typ}, got {at}"), arg.1.into_range())
							.with_label("type mismatch"),
					);
				}
				let val = self.make_enum(&variants, 1, &[fv]);
				Ok((val, Typ::Option(Box::new(inner_typ))))
			}

			Expr::ResultInit { inner: (te, span), arg } => {
				let inner_typ = self.types().resolve(te, *span)?;
				let variants = result_variants(&inner_typ);
				let (fv, at) = self.check_expr(arg, &inner_typ)?;
				let disc = if at == inner_typ {
					0
				} else if at == Typ::Error {
					1
				} else {
					return Err(Diagnostic::new(
						format!("expected {inner_typ} or Error, got {at}"),
						arg.1.into_range(),
					)
					.with_label("type mismatch"));
				};
				let val = self.make_enum(&variants, disc, &[fv]);
				Ok((val, Typ::Result(Box::new(inner_typ))))
			}

			Expr::Ident(name) => {
				let local = self.vars.get(name).cloned().ok_or_else(|| {
					Diagnostic::new(format!("undefined variable `{name}`"), expr.1.into_range())
						.with_label("not found in scope")
				})?;
				Ok((self.b.use_var(local.var), local.typ))
			}

			Expr::Dollar => Ok(self.dollar()),

			Expr::Negative(e) => {
				let (v, typ) = self.expr(e)?;
				let out = match typ {
					Typ::Int(_) => self.b.ins().ineg(v),
					Typ::Float(_) => self.b.ins().fneg(v),
					_ => {
						return Err(Diagnostic::new(format!("cannot negate {typ}"), expr.1.into_range())
							.with_label(format!("this is {typ}")));
					}
				};
				Ok((out, typ))
			}

			Expr::Add(l, r) => self.binop(Op::Add, l, r, expr.1),
			Expr::Sub(l, r) => self.binop(Op::Sub, l, r, expr.1),
			Expr::Mul(l, r) => self.binop(Op::Mul, l, r, expr.1),
			Expr::Div(l, r) => self.binop(Op::Div, l, r, expr.1),
			Expr::Mod(l, r) => self.binop(Op::Mod, l, r, expr.1),

			Expr::Eq(l, r) => self.cmp(IntCC::Equal, FloatCC::Equal, l, r, expr.1),
			Expr::Ne(l, r) => self.cmp(IntCC::NotEqual, FloatCC::NotEqual, l, r, expr.1),
			Expr::Lt(l, r) => self.cmp(IntCC::SignedLessThan, FloatCC::LessThan, l, r, expr.1),
			Expr::Gt(l, r) => self.cmp(IntCC::SignedGreaterThan, FloatCC::GreaterThan, l, r, expr.1),
			Expr::Le(l, r) => self.cmp(IntCC::SignedLessThanOrEqual, FloatCC::LessThanOrEqual, l, r, expr.1),
			Expr::Ge(l, r) => self.cmp(
				IntCC::SignedGreaterThanOrEqual,
				FloatCC::GreaterThanOrEqual,
				l,
				r,
				expr.1,
			),

			Expr::And(l, r) => self.logical(true, l, r),
			Expr::Or(l, r) => self.logical(false, l, r),
			Expr::Not(e) => {
				let (v, typ) = self.expr(e)?;
				if typ != Typ::Bool {
					return Err(
						Diagnostic::new(format!("expected Bool, got {typ}"), expr.1.into_range())
							.with_label("`!` needs a Bool operand"),
					);
				}
				// a bool is always 0 or 1, so flipping the low bit negates it
				Ok((self.b.ins().bxor_imm(v, 1), Typ::Bool))
			}

			Expr::Call { name, args } => match self.builtin_call(name, args, expr.1)? {
				Some(result) => Ok(result),
				None => {
					let sig = self.funcs.get(name).cloned().ok_or_else(|| {
						Diagnostic::new(format!("undefined function `{name}`"), expr.1.into_range())
							.with_label("not defined")
					})?;
					self.call_sig(name, sig, None, args, expr.1)
				}
			},

			Expr::MethodCall { recv, method, args } => {
				// enum payload
				if let Expr::Ident(name) = &recv.0
					&& !self.vars.contains_key(name)
					&& self.enums.contains_key(name)
				{
					return if method == "from" {
						self.enum_from(name, args, expr.1)
					} else {
						self.construct_variant(name, method, args, expr.1)
					};
				}

				// method call is static when `recv` names a struct
				let (sname, recv_val) = if let Expr::Ident(name) = &recv.0
					&& !self.vars.contains_key(name)
					&& self.structs.contains_key(name)
				{
					(name.clone(), None)
				} else {
					let (recv_val, recv_typ) = self.expr(recv)?;
					if let Typ::Enum(enum_name) = &recv_typ {
						if method == "str" && args.is_empty() {
							let s = self.enum_name_str(self.enum_variants(enum_name), recv_val);
							return Ok((s, Typ::Str));
						}
						return Err(Diagnostic::new(
							format!("enum `{enum_name}` has no method `{method}`"),
							expr.1.into_range(),
						)
						.with_label("no such method"));
					}

					// `Error` trait
					if recv_typ == Typ::Error {
						if method == "message" && args.is_empty() {
							return Ok((recv_val, Typ::Str));
						}
						return Err(
							Diagnostic::new(format!("`Error` has no method `{method}`"), expr.1.into_range())
								.with_label("no such method"),
						);
					}
					match &recv_typ {
						Typ::Struct(name, _) => (name.clone(), Some(recv_val)),
						_ => {
							return Err(
								Diagnostic::new(format!("`{recv_typ}` has no methods"), recv.1.into_range())
									.with_label("methods are only defined on structs"),
							);
						}
					}
				};
				let key = format!("{sname}.{method}");
				let sig = self.funcs.get(&key).cloned().ok_or_else(|| {
					Diagnostic::new(format!("`{sname}` has no method `{method}`"), expr.1.into_range())
						.with_label("no such method")
				})?;
				self.call_sig(&key, sig, recv_val, args, expr.1)
			}

			// a tuple is a heap block of pointer-sized slots, one per field
			Expr::Tuple(elems) => {
				if elems.is_empty() {
					return Ok((self.b.ins().iconst(self.int, 0), Typ::Tuple(vec![])));
				}
				let ptr = self.call_alloc(elems.len());
				let mut fields = Vec::with_capacity(elems.len());
				for (i, (name, value)) in elems.iter().enumerate() {
					let (val, typ) = self.expr(value)?;
					self.b.ins().store(MemFlags::new(), val, ptr, (i * 8) as i32);
					fields.push((name.clone(), typ));
				}
				Ok((ptr, Typ::Tuple(fields)))
			}

			Expr::Field { tuple, field } => {
				// enum variants
				if let Expr::Ident(name) = &tuple.0
					&& !self.vars.contains_key(name)
					&& self.enums.contains_key(name)
				{
					return self.construct_variant(name, field, &[], expr.1);
				}

				let (ptr, typ) = self.expr(tuple)?;

				// arrays expose `.len` and numeric `.n` (sugar for `arr[n]`)
				if let Typ::Array(_) | Typ::FixedArray(..) = &typ {
					let elem = array_elem(&typ).clone();
					let (data, len) = self.array_parts(ptr, &typ);
					if field == "len" {
						let len = self.b.ins().ireduce(types::I32, len);
						return Ok((len, Typ::Int(32)));
					}
					return match field.parse::<i64>() {
						Ok(n) => {
							let idx = self.b.ins().iconst(self.int, n);
							Ok((self.load_index(data, len, &elem, idx), elem))
						}
						Err(_) => Err(
							Diagnostic::new(format!("arrays have no field `{field}`"), expr.1.into_range())
								.with_label("arrays only have `.len` and numeric indices"),
						),
					};
				}

				// structs are just fully-named tuples at the codegen level
				let typ = if let Typ::Struct(_, fields) = typ {
					Typ::Tuple(fields.into_iter().map(|f| (Some(f.name), f.typ)).collect())
				} else {
					typ
				};

				let fields = match &typ {
					Typ::Tuple(fields) => fields,
					_ => {
						return Err(
							Diagnostic::new(format!("cannot access a field of {typ}"), tuple.1.into_range())
								.with_label("not a tuple"),
						);
					}
				};
				let idx = match field.parse::<usize>() {
					Ok(i) if i < fields.len() => i,
					Ok(i) => {
						return Err(Diagnostic::new(
							format!("tuple index {i} out of range (len {})", fields.len()),
							expr.1.into_range(),
						)
						.with_label("no such field"));
					}
					Err(_) => fields
						.iter()
						.position(|(name, _)| name.as_deref() == Some(field.as_str()))
						.ok_or_else(|| {
							Diagnostic::new(format!("tuple has no field `{field}`"), expr.1.into_range())
								.with_label("no such field")
						})?,
				};
				let field_typ = fields[idx].1.clone();
				let cl = cl_type(&field_typ, self.int);
				let v = self.b.ins().load(cl, MemFlags::new(), ptr, (idx * 8) as i32);
				Ok((v, field_typ))
			}

			Expr::Array(elems) => {
				if elems.is_empty() {
					return Err(
						Diagnostic::new("empty array literals aren't supported yet", expr.1.into_range())
							.with_label("needs at least one element to infer its type"),
					);
				}
				let mut elem_typ: Option<Typ> = None;
				let mut vals = Vec::with_capacity(elems.len());
				for e in elems {
					let (val, typ) = self.expr(e)?;
					match &elem_typ {
						Some(t) if t != &typ => {
							return Err(Diagnostic::new(
								format!("array elements must share a type: expected {t}, got {typ}"),
								e.1.into_range(),
							)
							.with_label("mismatched element type"));
						}
						_ => elem_typ = Some(typ),
					}
					vals.push(val);
				}
				let elem = elem_typ.unwrap();
				let size = elem_size(&elem);
				let data = self.call_alloc_bytes(elems.len() as i64 * size);
				for (i, val) in vals.into_iter().enumerate() {
					self.b.ins().store(MemFlags::new(), val, data, (i as i64 * size) as i32);
				}
				let len = self.b.ins().iconst(self.int, elems.len() as i64);
				let header = self.make_array(data, len);
				Ok((header, Typ::Array(Box::new(elem))))
			}

			Expr::ArrayInit((te, span)) => {
				let typ = self.types().resolve(te, *span)?;
				Ok((self.zero(&typ), typ))
			}

			Expr::Index { collection, index } => {
				let (ptr, typ) = self.array_operand(collection, "index")?;
				let elem = array_elem(&typ).clone();
				let idx = self.int_value(index, "array index")?;
				let idx = self.b.ins().sextend(self.int, idx);
				let (data, len) = self.array_parts(ptr, &typ);
				Ok((self.load_index(data, len, &elem, idx), elem))
			}

			Expr::Slice { collection, start, end } => {
				let (ptr, typ) = self.array_operand(collection, "slice")?;
				if let Typ::FixedArray(..) = typ {
					return Err(Diagnostic::new(
						"slicing fixed arrays is not supported yet",
						collection.1.into_range(),
					)
					.with_label("only dynamic arrays can be sliced for now"));
				}
				let elem = array_elem(&typ).clone();
				let start = match start {
					Some(e) => {
						let v = self.int_value(e, "slice start")?;
						self.b.ins().sextend(self.int, v)
					}
					None => self.b.ins().iconst(self.int, 0),
				};
				let end = match end {
					Some(e) => {
						let v = self.int_value(e, "slice end")?;
						self.b.ins().sextend(self.int, v)
					}
					None => self.array_len(ptr),
				};
				let size = self.b.ins().iconst(self.int, elem_size(&elem));
				let func = self.import_fn(
					runtime::SLICE,
					&[self.int, self.int, self.int, self.int],
					Some(self.int),
				);
				let call = self.b.ins().call(func, &[ptr, start, end, size]);
				Ok((self.b.inst_results(call)[0], Typ::Array(Box::new(elem))))
			}

			Expr::If { cond, then, els } => match self.conditional(cond, then, els.as_deref(), None, expr.1)? {
				Some((v, t)) => Ok((v, t)),
				None => Err(Diagnostic::new("this `if` never produces a value", expr.1.into_range())
					.with_label("every branch returns, but a value is needed here")),
			},

			Expr::Match {
				subject,
				arms,
				else_body,
			} => match self.match_expr(subject, arms, else_body.as_deref(), None, expr.1)? {
				Some((v, t)) => Ok((v, t)),
				None => Err(
					Diagnostic::new("this `match` never produces a value", expr.1.into_range())
						.with_label("every arm returns, but a value is needed here"),
				),
			},

			Expr::OrElse { value, body } => self.or_else(value, body, expr.1),
			Expr::PropagateNone(value) => self.propagate(value, false, expr.1),
			Expr::PropagateErr(value) => self.propagate(value, true, expr.1),

			Expr::Loop { cond, body } => match self.loop_expr(cond.as_deref(), body)? {
				Some(vt) => Ok(vt),
				None => Err(
					Diagnostic::new("this `loop` never produces a value", expr.1.into_range())
						.with_label("an infinite loop with no `break` yields nothing"),
				),
			},

			Expr::For { pat, iter, body } => self.for_loop(pat, iter, body, expr.1),

			Expr::In(lhs, rhs) => {
				let (rhs_val, rhs_typ) = self.expr(rhs)?;

				// substring check
				if rhs_typ == Typ::Str {
					let (lhs_val, lhs_typ) = self.expr(lhs)?;
					if lhs_typ != Typ::Str {
						return Err(
							Diagnostic::new(format!("cannot search {lhs_typ} in Str"), lhs.1.into_range())
								.with_label("type mismatch: value must be Str"),
						);
					}
					let func = self.import_fn(runtime::STR_CONTAINS, &[self.int, self.int], Some(self.int));
					let call = self.b.ins().call(func, &[rhs_val, lhs_val]);
					return Ok((self.b.inst_results(call)[0], Typ::Bool));
				}

				let elem = match rhs_typ {
					Typ::Array(ref e) => (**e).clone(),
					_ => {
						return Err(Diagnostic::new(
							format!("right side of `in` must be an array or Str, got {rhs_typ}"),
							rhs.1.into_range(),
						)
						.with_label("not an array or string"));
					}
				};
				let (val, val_typ) = self.expr(lhs)?;
				if val_typ != elem {
					return Err(Diagnostic::new(
						format!("cannot search {val_typ} in {elem} array"),
						lhs.1.into_range(),
					)
					.with_label("type mismatch"));
				}

				let arr = rhs_val;
				let len = self.array_len(arr);
				let data = self.array_data(arr);

				let found = self.b.declare_var(self.int);
				let i = self.b.declare_var(self.int);
				let zero = self.b.ins().iconst(self.int, 0);
				self.b.def_var(found, zero);
				self.b.def_var(i, zero);

				let (header, body, found_block, continue_block, exit) = (
					self.b.create_block(),
					self.b.create_block(),
					self.b.create_block(),
					self.b.create_block(),
					self.b.create_block(),
				);
				self.b.ins().jump(header, &[]);

				self.b.switch_to_block(header);
				let iv = self.b.use_var(i);
				let more = self.b.ins().icmp(IntCC::SignedLessThan, iv, len);
				self.b.ins().brif(more, body, &[], exit, &[]);
				self.b.seal_block(body);

				self.b.switch_to_block(body);
				let iv = self.b.use_var(i);
				let off = self.b.ins().imul_imm(iv, elem_size(&elem));
				let addr = self.b.ins().iadd(data, off);
				let elem_val = self.b.ins().load(cl_type(&elem, self.int), MemFlags::new(), addr, 0);
				let equal = self.emit_eq(val, elem_val, &elem);
				self.b.ins().brif(equal, found_block, &[], continue_block, &[]);
				self.b.seal_block(found_block);
				self.b.seal_block(continue_block);

				self.b.switch_to_block(found_block);
				let one = self.b.ins().iconst(self.int, 1);
				self.b.def_var(found, one);
				self.b.ins().jump(exit, &[]);
				self.b.seal_block(exit);

				self.b.switch_to_block(continue_block);
				let iv = self.b.use_var(i);
				let next = self.b.ins().iadd_imm(iv, 1);
				self.b.def_var(i, next);
				self.b.ins().jump(header, &[]);
				self.b.seal_block(header);

				self.b.switch_to_block(exit);
				Ok((self.b.use_var(found), Typ::Bool))
			}

			Expr::StructLit { name, fields } => {
				// `Self {}` inside a method resolves to the impl's type
				let name = match name.as_str() {
					"Self" => self.self_type.clone().ok_or_else(|| {
						Diagnostic::new("`Self` is only valid in an impl block", expr.1.into_range())
							.with_label("no enclosing impl")
					})?,
					_ => name.clone(),
				};
				if self.enums.contains_key(name.as_str()) {
					if !fields.is_empty() {
						return Err(Diagnostic::new(
							format!("enum `{name}` only supports `{name}{{}}` with no fields"),
							expr.1.into_range(),
						)
						.with_label("not a struct"));
					}
					let typ = Typ::Enum(name.clone());
					return Ok((self.zero(&typ), typ));
				}
				let struct_fields = self.structs.get(name.as_str()).cloned().ok_or_else(|| {
					Diagnostic::new(format!("unknown struct `{name}`"), expr.1.into_range()).with_label("not defined")
				})?;
				let size = (struct_fields.len() * 8) as u32;
				let slot = self
					.b
					.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, size, 0));
				let ptr = self.b.ins().stack_addr(self.int, slot, 0);

				for (i, f) in struct_fields.iter().enumerate() {
					let init = if let Some(default_expr) = &f.default {
						let (val, vtyp) = self.expr(default_expr)?;
						if vtyp != f.typ {
							return Err(Diagnostic::new(
								format!("default value type mismatch: expected {}, got {vtyp}", f.typ),
								default_expr.1.into_range(),
							)
							.with_label("type mismatch"));
						}
						val
					} else {
						self.zero(&f.typ)
					};
					self.b.ins().store(MemFlags::new(), init, ptr, (i * 8) as i32);
				}

				if !fields.is_empty() {
					let positional = fields[0].0.is_none();
					if positional {
						if fields.len() != struct_fields.len() {
							return Err(Diagnostic::new(
								format!(
									"`{name}` has {} fields but {} values were provided",
									struct_fields.len(),
									fields.len()
								),
								expr.1.into_range(),
							)
							.with_label("wrong number of fields"));
						}
						for (i, (_, value)) in fields.iter().enumerate() {
							let expected = struct_fields[i].typ.clone();
							let (val, vtyp) = self.check_expr(value, &expected)?;
							if vtyp != expected {
								return Err(Diagnostic::new(
									format!("expected {expected}, got {vtyp}"),
									value.1.into_range(),
								)
								.with_label("type mismatch"));
							}
							self.b.ins().store(MemFlags::new(), val, ptr, (i * 8) as i32);
						}
					} else {
						for (field_name, value) in fields {
							let fname = field_name.as_deref().ok_or_else(|| {
								Diagnostic::new("cannot mix named and positional fields", value.1.into_range())
									.with_label("missing field name")
							})?;
							let idx = struct_fields.iter().position(|f| f.name == fname).ok_or_else(|| {
								Diagnostic::new(format!("`{name}` has no field `{fname}`"), value.1.into_range())
									.with_label("no such field")
							})?;
							let expected = struct_fields[idx].typ.clone();
							let (val, vtyp) = self.check_expr(value, &expected)?;
							if vtyp != expected {
								return Err(Diagnostic::new(
									format!("expected {expected}, got {vtyp}"),
									value.1.into_range(),
								)
								.with_label("type mismatch"));
							}
							self.b.ins().store(MemFlags::new(), val, ptr, (idx * 8) as i32);
						}
					}
				}
				Ok((ptr, Typ::Struct(name.clone(), struct_fields)))
			}

			Expr::Range { start, end } => {
				let start_val = match start {
					Some(s) => self.int_value(s, "range start")?,
					None => self.b.ins().iconst(cl_int_for_width(32), 0),
				};
				let end_val = match end {
					Some(e) => self.int_value(e, "range end")?,
					None => self.b.ins().iconst(cl_int_for_width(32), 0),
				};
				let ptr = self.call_alloc(2);
				let cl = self.b.func.dfg.value_type(start_val);
				let s_ext = if cl == self.int {
					start_val
				} else {
					self.b.ins().sextend(self.int, start_val)
				};
				let e_ext = if cl == self.int {
					end_val
				} else {
					self.b.ins().sextend(self.int, end_val)
				};
				self.b.ins().store(MemFlags::new(), s_ext, ptr, 0);
				self.b.ins().store(MemFlags::new(), e_ext, ptr, 8);
				Ok((ptr, Typ::Range))
			}

			Expr::Bind { .. } => unreachable!("bind in expression position"),
			Expr::Assign { .. } => unreachable!("assign in expression position"),
			Expr::IndexAssign { .. } => unreachable!("index assign in expression position"),
			Expr::Fn { .. } => unreachable!("fn definition in expression position"),
			Expr::StructDef { .. } => unreachable!("struct definition in expression position"),
			Expr::EnumDef { .. } => unreachable!("enum definition in expression position"),
			Expr::Impl { .. } => unreachable!("impl block in expression position"),
			Expr::TypeAlias { .. } => unreachable!("type alias in expression position"),
			Expr::FieldAssign { .. } => unreachable!("field assign in expression position"),
			Expr::Return(..) => unreachable!("return in expression position"),
			Expr::Break | Expr::Continue => unreachable!("break/continue in expression position"),
			Expr::Append { .. } => unreachable!("append in expression position"),
			Expr::Doc(_) => unreachable!("doc comment in expression position"),
		}
	}
}
