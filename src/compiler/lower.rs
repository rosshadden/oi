use std::collections::HashMap;
use std::ops::Range;

use cranelift::codegen;
use cranelift::codegen::ir::immediates::{Ieee16, Ieee128};
use cranelift::codegen::ir::{StackSlotData, StackSlotKind};
use cranelift::prelude::*;
use cranelift_jit::JITModule;
use cranelift_module::{DataDescription, Linkage, Module};

use super::{
	FieldDef, FnSig, Local, LoopFrame, Op, Typ, VariantInfo, cl_int_for_width, cl_type, elem_size,
	enum_boxed, enum_slots, resolve_type,
};
use crate::ast::{Expr, MatchArm, Pattern, Span, Spanned};
use crate::diagnostics::Diagnostic;
use crate::runtime;

pub(super) struct Translator<'a> {
	pub int: types::Type,
	pub b: FunctionBuilder<'a>,
	pub vars: HashMap<String, Local>,
	pub module: &'a mut JITModule,
	pub funcs: &'a HashMap<String, FnSig>,
	pub structs: &'a HashMap<String, Vec<FieldDef>>,
	pub enums: &'a HashMap<String, Vec<VariantInfo>>,
	pub string_idx: &'a mut usize,
	pub atoms: &'a mut HashMap<String, ()>,
	pub ret: Option<(Typ, Span)>,
	pub loops: Vec<LoopFrame>,
	pub self_type: Option<String>,
}

impl<'a> Translator<'a> {
	// Look up a mutable local.
	fn mutable_local(
		&self,
		name: &str,
		span: Range<usize>,
		verb: &str,
		immutable_verb: &str,
		allow: &str,
		suggest_declare: bool,
	) -> Result<Local, Diagnostic> {
		let local = self.vars.get(name).cloned().ok_or_else(|| {
			let d = Diagnostic::new(
				format!("cannot {verb} undefined variable `{name}`"),
				span.clone(),
			)
			.with_label("not found in scope");
			if suggest_declare {
				d.with_note(format!("declare it first with `{name} := ...`"))
			} else {
				d
			}
		})?;
		if !local.mutable {
			return Err(Diagnostic::new(
				format!("cannot {immutable_verb} immutable `{name}`"),
				span,
			)
			.with_label("declared without `mut`")
			.with_note(format!("use `mut {name} := ...` to allow {allow}")));
		}
		Ok(local)
	}

	// Evaluate a block of statements, returning the final value.
	// Returns None if the block diverged (every path returned/broke).
	pub fn block(&mut self, stmts: &[&Spanned<Expr>]) -> Result<Option<(Value, Typ)>, Diagnostic> {
		let mut last = (self.b.ins().iconst(self.int, 0), Typ::Tuple(vec![]));
		for &stmt in stmts {
			match &stmt.0 {
				Expr::Bind {
					mutable,
					name,
					typ,
					value,
				} => {
					let annot = typ
						.as_ref()
						.map(|(t, span)| {
							resolve_type(t, *span, self.structs, self.enums, &HashMap::new())
						})
						.transpose()?;
					let (val, typ) = match (value, annot) {
						(Some(value), Some(target)) => match self.coerce_lit(value, &target)? {
							Some(val) => (val, target),
							None => {
								let (val, found) = self.expr(value)?;
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
					self.vars.insert(
						name.clone(),
						Local {
							var,
							typ,
							mutable: *mutable,
						},
					);
				}

				Expr::Assign { name, value } => {
					let local = self.mutable_local(
						name,
						stmt.1.into_range(),
						"assign to",
						"assign to",
						"assignment",
						true,
					)?;
					let (val, typ) = self.check_expr(value, &local.typ)?;
					if typ != local.typ {
						return Err(Diagnostic::new(
							format!(
								"cannot assign {typ:?} to `{name}`, which is {:?}",
								local.typ
							),
							value.1.into_range(),
						)
						.with_label("type mismatch"));
					}
					if let Typ::Struct(_, ref fields) = typ {
						let fields = fields.clone();
						let dst = self.b.use_var(local.var);
						for (i, f) in fields.iter().enumerate() {
							let cl = cl_type(&f.typ, self.int);
							let fv = self.b.ins().load(cl, MemFlags::new(), val, (i * 8) as i32);
							self.b.ins().store(MemFlags::new(), fv, dst, (i * 8) as i32);
						}
					} else {
						self.b.def_var(local.var, val);
					}
				}

				Expr::IndexAssign { name, index, value } => {
					let local = self.mutable_local(
						name,
						stmt.1.into_range(),
						"assign to",
						"assign to element of",
						"assignment",
						true,
					)?;
					let elem = match &local.typ {
						Typ::Array(e) | Typ::FixedArray(e, _) => (**e).clone(),
						_ => {
							return Err(Diagnostic::new(
								format!("`{name}` is not an array"),
								stmt.1.into_range(),
							)
							.with_label("not an array"));
						}
					};
					let ptr = self.b.use_var(local.var);
					let idx = self.int_value(index, "array index")?;
					let idx = self.b.ins().sextend(self.int, idx);
					let (val, vtyp) = self.expr(value)?;
					if vtyp != elem {
						return Err(Diagnostic::new(
							format!("cannot assign {vtyp:?} to element of {elem:?} array"),
							value.1.into_range(),
						)
						.with_label("type mismatch"));
					}
					let (data, len) = self.array_parts(ptr, &local.typ);
					self.store_index(data, len, &elem, idx, val);
				}

				Expr::Append { name, value } => {
					let local = self.mutable_local(
						name,
						stmt.1.into_range(),
						"append to",
						"append to",
						"append",
						false,
					)?;
					let elem = match &local.typ {
						Typ::Array(e) => (**e).clone(),
						_ => {
							return Err(Diagnostic::new(
								format!("`{name}` is not an array"),
								stmt.1.into_range(),
							)
							.with_label("not an array"));
						}
					};
					let (val, vtyp) = self.expr(value)?;
					let size = self.b.ins().iconst(self.int, elem_size(&elem));
					let ptr = self.b.use_var(local.var);

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
						let func = self.import_fn(
							runtime::ARRAY_RESERVE,
							&[self.int, self.int, self.int],
							None,
						);
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
						let func = self.import_fn(
							runtime::ARRAY_EXTEND,
							&[self.int, self.int, self.int],
							None,
						);
						self.b.ins().call(func, &[ptr, val, size]);
					} else {
						return Err(Diagnostic::new(
							format!("cannot append {vtyp:?} to {elem:?} array"),
							value.1.into_range(),
						)
						.with_label("type mismatch"));
					}
				}

				Expr::Return(value) => {
					let (val, typ) = match value {
						Some(e) => self.expr(e)?,
						None => {
							let typ = self
								.ret
								.as_ref()
								.map_or(Typ::Tuple(vec![]), |(t, _)| t.clone());
							(self.zero(&typ), typ)
						}
					};
					self.emit_return(val, typ, stmt.1)?;
					return Ok(None);
				}

				Expr::If { cond, then, els } => {
					match self.conditional(cond, then, els.as_deref(), stmt.1)? {
						Some((v, t)) => last = (v, t),
						None => return Ok(None),
					}
				}

				Expr::Match {
					subject,
					arms,
					else_body,
				} => match self.match_expr(subject, arms, else_body.as_deref(), stmt.1)? {
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
					let local = self.mutable_local(
						name,
						stmt.1.into_range(),
						"assign field of",
						"assign field of",
						"field assignment",
						false,
					)?;
					let fields = match &local.typ {
						Typ::Struct(_, fields) => fields.clone(),
						_ => {
							return Err(Diagnostic::new(
								format!("`{name}` is not a struct"),
								stmt.1.into_range(),
							)
							.with_label("not a struct"));
						}
					};
					let idx = fields
						.iter()
						.position(|f| &f.name == field)
						.ok_or_else(|| {
							Diagnostic::new(
								format!("struct has no field `{field}`"),
								stmt.1.into_range(),
							)
							.with_label("no such field")
						})?;
					let (val, vtyp) = self.expr(value)?;
					if vtyp != fields[idx].typ {
						return Err(Diagnostic::new(
							format!(
								"cannot assign {vtyp:?} to field `{field}` of type {:?}",
								fields[idx].typ
							),
							value.1.into_range(),
						)
						.with_label("type mismatch"));
					}
					let ptr = self.b.use_var(local.var);
					self.b
						.ins()
						.store(MemFlags::new(), val, ptr, (idx * 8) as i32);
				}

				Expr::Break => {
					let exit = match self.loops.last() {
						Some(frame) => frame.exit,
						None => {
							return Err(Diagnostic::new(
								"`break` outside of a loop",
								stmt.1.into_range(),
							)
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
							return Err(Diagnostic::new(
								"`continue` outside of a loop",
								stmt.1.into_range(),
							)
							.with_label("not inside a loop"));
						}
					};
					self.b.ins().jump(top, &[]);
					return Ok(None);
				}

				Expr::Doc(_) => {}

				_ => last = self.expr(stmt)?,
			}
		}
		Ok(Some(last))
	}

	// The first return fixes the fn's type, and later returns must agree.
	pub fn emit_return(&mut self, val: Value, typ: Typ, span: Span) -> Result<(), Diagnostic> {
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
					self.b
						.ins()
						.store(MemFlags::new(), fv, heap, (i * 8) as i32);
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
			self.b
				.func
				.signature
				.returns
				.push(AbiParam::new(cl_type(&typ, self.int)));
		}
		self.b.ins().return_(&[final_val]);
		if self.ret.is_none() {
			self.ret = Some((typ, span));
		}
		Ok(())
	}

	// `if`/`else` lowered to branch&merge, yielding value of the chosen branch.
	// A diverging branch contributes nothing to the merge.
	// If all branches diverge, returns None.
	fn conditional(
		&mut self,
		cond: &Spanned<Expr>,
		then: &[Spanned<Expr>],
		els: Option<&[Spanned<Expr>]>,
		span: Span,
	) -> Result<Option<(Value, Typ)>, Diagnostic> {
		let (cv, ct) = self.expr(cond)?;
		if ct != Typ::Bool {
			return Err(Diagnostic::new(
				format!("`if` condition must be Bool, got {ct:?}"),
				cond.1.into_range(),
			)
			.with_label("not a Bool"));
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
		let then_refs: Vec<&Spanned<Expr>> = then.iter().collect();
		let then_flow = self.block(&then_refs)?;
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
			Some(els) => {
				let refs: Vec<&Spanned<Expr>> = els.iter().collect();
				self.block(&refs)?
			}
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
						format!("`if` branches have mismatched types: {rt:?} and {t:?}"),
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
	fn match_expr(
		&mut self,
		subject: &Spanned<Expr>,
		arms: &[MatchArm],
		else_body: Option<&[Spanned<Expr>]>,
		span: Span,
	) -> Result<Option<(Value, Typ)>, Diagnostic> {
		let (sv, st) = self.expr(subject)?;
		let sv_var = self.b.declare_var(cl_type(&st, self.int));
		self.b.def_var(sv_var, sv);

		// ensure enum match covers every variant
		if let Typ::Enum(ename) = &st {
			let pats = || arms.iter().flat_map(|a| &a.patterns);
			let catch_all =
				else_body.is_some() || pats().any(|p| matches!(&p.0, Expr::Ident(w) if w == "_"));
			if !catch_all {
				let covered = pats()
					.map(|p| self.enum_pattern(p, ename).map(|(d, _)| d))
					.collect::<Result<Vec<_>, _>>()?;
				let missing: Vec<_> = self.enums[ename]
					.iter()
					.filter(|v| !covered.contains(&v.disc))
					.map(|v| v.name.clone())
					.collect();
				if !missing.is_empty() {
					let msg = format!("non-exhaustive match, missing: {}", missing.join(", "));
					return Err(Diagnostic::new(msg, span.into_range())
						.with_label("cover these variants or add `else`"));
				}
			}
		}

		let merge = self.b.create_block();
		let mut result: Option<(Variable, Typ)> = None;

		// pre-create each arm's entry block so each arm knows where to fall through to on failure
		let arm_entries: Vec<Block> = arms.iter().map(|_| self.b.create_block()).collect();
		let else_blk = self.b.create_block();
		self.b
			.ins()
			.jump(arm_entries.first().copied().unwrap_or(else_blk), &[]);

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
				} else if let Typ::Enum(enum_name) = &st {
					let (disc, b) = self.enum_pattern(pat, enum_name)?;
					if arm.patterns.len() == 1 {
						binds = b;
					}
					let sv = self.b.use_var(sv_var);
					let tag = self.enum_tag(enum_name, sv);
					let disc = self.b.ins().iconst(self.int, disc);
					self.b.ins().icmp(IntCC::Equal, tag, disc)
				} else if let (Typ::Tuple(fields), Expr::Tuple(elems)) = (&st, &pat.0) {
					if elems.len() != fields.len() {
						let msg = format!(
							"tuple pattern has {} elements, subject has {}",
							elems.len(),
							fields.len()
						);
						return Err(
							Diagnostic::new(msg, pat.1.into_range()).with_label("arity mismatch")
						);
					}
					if arm.patterns.len() == 1 {
						let pairs = elems.iter().zip(fields).map(|((_, e), (_, t))| (e, t));
						binds = field_binds(pairs, 0, 8)?;
					}
					self.b.ins().iconst(types::I8, 1)
				} else if let (
					Typ::Struct(sname, fdefs),
					Expr::StructLit {
						name: pname,
						fields,
					},
				) = (&st, &pat.0)
				{
					if arm.patterns.len() == 1 {
						binds = struct_pattern(fdefs, pname, sname, fields, pat.1)?;
					}
					self.b.ins().iconst(types::I8, 1)
				} else if let (Typ::Array(elem) | Typ::FixedArray(elem, _), Expr::Array(elems)) =
					(&st, &pat.0)
				{
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
							format!("match pattern ({pt:?}) does not match subject ({st:?})"),
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
			let flow = self.block(&arm.body.iter().collect::<Vec<_>>())?;
			self.vars = saved;
			if let Some(vt) = flow {
				self.match_contribute(vt, &mut result, merge, span)?;
			}
		}

		self.b.switch_to_block(else_blk);
		self.b.seal_block(else_blk);
		let else_flow = if let Some(els) = else_body {
			let saved = self.vars.clone();
			let flow = self.block(&els.iter().collect::<Vec<_>>())?;
			self.vars = saved;
			flow
		} else {
			let t = result
				.as_ref()
				.map_or(Typ::Tuple(vec![]), |(_, t)| t.clone());
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
	fn match_contribute(
		&mut self,
		(v, t): (Value, Typ),
		result: &mut Option<(Variable, Typ)>,
		merge: Block,
		span: Span,
	) -> Result<(), Diagnostic> {
		match result {
			Some((_, rt)) if rt != &t => Err(Diagnostic::new(
				format!("`match` arms have mismatched types: {rt:?} and {t:?}"),
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

	fn loop_expr(
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
						format!("`loop` condition must be Bool, got {ct:?}"),
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
		let refs: Vec<&Spanned<Expr>> = body.iter().collect();
		let flow = self.block(&refs)?;
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

	fn for_loop(
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
				return Err(Diagnostic::new(
					format!("cannot iterate over {typ}"),
					iter.1.into_range(),
				)
				.with_label("not iterable"));
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
					self.b
						.ins()
						.load(cl_type(elem, self.int), MemFlags::new(), addr, 0),
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
		let refs: Vec<&Spanned<Expr>> = body.iter().collect();
		let flow = self.block(&refs)?;
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

	fn bind_pattern(
		&mut self,
		pat: &Pattern,
		val: Value,
		typ: &Typ,
		span: Span,
	) -> Result<(), Diagnostic> {
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
						format!("cannot destructure {typ:?} with a tuple pattern"),
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
					let fv = self.b.ins().load(
						cl_type(ftyp, self.int),
						MemFlags::new(),
						val,
						(i * 8) as i32,
					);
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

	// Widen/narrow integers.
	// Sign-extend `val` to i64, clamp to `[low, hi]`.
	fn clamp_to_width(
		&mut self,
		val: Value,
		extend_signed: bool,
		low: Option<(i64, bool)>,
		hi: i64,
		hi_unsigned: bool,
		target_cl: types::Type,
	) -> Value {
		let src_cl = self.b.func.dfg.value_type(val);
		let v64 = if src_cl == types::I64 {
			val
		} else if extend_signed {
			self.b.ins().sextend(types::I64, val)
		} else {
			self.b.ins().uextend(types::I64, val)
		};
		let v64 = match low {
			Some((low, lo_unsigned)) => {
				let lo_c = self.b.ins().iconst(types::I64, low);
				let cc = if lo_unsigned {
					IntCC::UnsignedLessThan
				} else {
					IntCC::SignedLessThan
				};
				let lt = self.b.ins().icmp(cc, v64, lo_c);
				self.b.ins().select(lt, lo_c, v64)
			}
			None => v64,
		};
		let hi_c = self.b.ins().iconst(types::I64, hi);
		let cc = if hi_unsigned {
			IntCC::UnsignedGreaterThan
		} else {
			IntCC::SignedGreaterThan
		};
		let gt = self.b.ins().icmp(cc, v64, hi_c);
		let v64 = self.b.ins().select(gt, hi_c, v64);
		if target_cl == types::I64 {
			v64
		} else {
			self.b.ins().ireduce(target_cl, v64)
		}
	}

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

			Expr::Ident(name) => {
				let local = self.vars.get(name).cloned().ok_or_else(|| {
					Diagnostic::new(format!("undefined variable `{name}`"), expr.1.into_range())
						.with_label("not found in scope")
				})?;
				Ok((self.b.use_var(local.var), local.typ))
			}

			Expr::Negative(e) => {
				let (v, typ) = self.expr(e)?;
				let out = match typ {
					Typ::Int(_) => self.b.ins().ineg(v),
					Typ::Float(_) => self.b.ins().fneg(v),
					_ => {
						return Err(Diagnostic::new(
							format!("cannot negate {typ:?}"),
							expr.1.into_range(),
						)
						.with_label(format!("this is {typ:?}")));
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
			Expr::Gt(l, r) => {
				self.cmp(IntCC::SignedGreaterThan, FloatCC::GreaterThan, l, r, expr.1)
			}
			Expr::Le(l, r) => self.cmp(
				IntCC::SignedLessThanOrEqual,
				FloatCC::LessThanOrEqual,
				l,
				r,
				expr.1,
			),
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
					return Err(Diagnostic::new(
						format!("expected Bool, got {typ:?}"),
						expr.1.into_range(),
					)
					.with_label("`!` needs a Bool operand"));
				}
				// a bool is always 0 or 1, so flipping the low bit negates it
				Ok((self.b.ins().bxor_imm(v, 1), Typ::Bool))
			}

			Expr::Call { name, args }
				if matches!(name.as_str(), "print" | "write" | "eprint" | "ewrite") =>
			{
				if args.is_empty() {
					return Err(Diagnostic::new(
						format!("`{name}` takes at least 1 argument"),
						expr.1.into_range(),
					)
					.with_label("missing argument"));
				}
				let stderr = matches!(name.as_str(), "eprint" | "ewrite");
				let newline = matches!(name.as_str(), "print" | "eprint");
				for (i, arg) in args.iter().enumerate() {
					if i > 0 {
						self.write_lit(" ", stderr);
					}
					let (val, typ) = self.expr(arg)?;
					self.emit_print(val, &typ, false, stderr);
				}
				if newline {
					self.write_lit("\n", stderr);
				}
				Ok((self.b.ins().iconst(self.int, 0), Typ::Tuple(vec![])))
			}

			// TODO: migrate to `assert!` macro once we have macros
			Expr::Call { name, args } if name == "assert" => {
				if args.is_empty() || args.len() > 2 {
					return Err(Diagnostic::new(
						format!("`assert` takes 1 or 2 arguments, got {}", args.len()),
						expr.1.into_range(),
					)
					.with_label("wrong number of arguments"));
				}
				let (cond, cond_typ) = self.expr(&args[0])?;
				if cond_typ != Typ::Bool {
					return Err(Diagnostic::new(
						format!("`assert` condition must be Bool, got {cond_typ:?}"),
						args[0].1.into_range(),
					)
					.with_label("not a Bool"));
				}
				let msg = if args.len() == 2 {
					let (msg_val, msg_typ) = self.expr(&args[1])?;
					if msg_typ != Typ::Str {
						return Err(Diagnostic::new(
							format!("`assert` message must be Str, got {msg_typ:?}"),
							args[1].1.into_range(),
						)
						.with_label("not a Str"));
					}
					msg_val
				} else {
					self.str_const("assertion failed")
				};

				let fail_block = self.b.create_block();
				let ok_block = self.b.create_block();
				self.b.ins().brif(cond, ok_block, &[], fail_block, &[]);
				self.b.seal_block(fail_block);
				self.b.seal_block(ok_block);

				self.b.switch_to_block(fail_block);
				let func = self.import_fn(runtime::ASSERT_FAIL, &[self.int], None);
				self.b.ins().call(func, &[msg]);
				self.b.ins().trap(TrapCode::HEAP_OUT_OF_BOUNDS);

				self.b.switch_to_block(ok_block);
				Ok((cond, Typ::Bool))
			}

			Expr::Call { name, args } if matches!(name.as_str(), "isize" | "usize") => {
				let signed = name == "isize";
				if args.len() != 1 {
					return Err(Diagnostic::new(
						format!("`{name}` cast takes exactly 1 argument"),
						expr.1.into_range(),
					)
					.with_label("wrong number of arguments"));
				}
				let (val, typ) = self.expr(&args[0])?;
				let out = match (&typ, signed) {
					(Typ::ISize, true) | (Typ::USize, false) => val,
					// isize -> usize: clamp negative to 0
					(Typ::ISize, false) => {
						let zero = self.b.ins().iconst(self.int, 0);
						let lt = self.b.ins().icmp(IntCC::SignedLessThan, val, zero);
						self.b.ins().select(lt, zero, val)
					}
					// usize -> isize: saturate at isize::MAX
					(Typ::USize, true) => {
						let max_v = self.b.ins().iconst(self.int, i64::MAX);
						let gt = self.b.ins().icmp(IntCC::UnsignedGreaterThan, val, max_v);
						self.b.ins().select(gt, max_v, val)
					}
					// int -> isize: sign-extend
					(Typ::Int(_), true) => {
						let src_cl = cl_type(&typ, self.int);
						if src_cl == self.int {
							val
						} else {
							self.b.ins().sextend(self.int, val)
						}
					}
					// uint -> usize: zero-extend
					(Typ::UInt(_), false) => {
						let src_cl = cl_type(&typ, self.int);
						if src_cl == self.int {
							val
						} else {
							self.b.ins().uextend(self.int, val)
						}
					}
					// int -> usize: sign-extend then clamp negative to 0
					(Typ::Int(_), false) => {
						let src_cl = cl_type(&typ, self.int);
						let v = if src_cl == self.int {
							val
						} else {
							self.b.ins().sextend(self.int, val)
						};
						let zero = self.b.ins().iconst(self.int, 0);
						let lt = self.b.ins().icmp(IntCC::SignedLessThan, v, zero);
						self.b.ins().select(lt, zero, v)
					}
					// uint -> isize: zero-extend then saturate at isize::MAX
					(Typ::UInt(_), true) => {
						let src_cl = cl_type(&typ, self.int);
						let v = if src_cl == self.int {
							val
						} else {
							self.b.ins().uextend(self.int, val)
						};
						let max_v = self.b.ins().iconst(self.int, i64::MAX);
						let gt = self.b.ins().icmp(IntCC::UnsignedGreaterThan, v, max_v);
						self.b.ins().select(gt, max_v, v)
					}
					_ => {
						return Err(Diagnostic::new(
							format!("cannot cast {typ:?} to {name}"),
							args[0].1.into_range(),
						)
						.with_label("not an integer"));
					}
				};
				let out_typ = if signed { Typ::ISize } else { Typ::USize };
				Ok((out, out_typ))
			}

			Expr::Call { name, args } if name == "int" => self.expr(&(
				Expr::Call {
					name: "i32".to_owned(),
					args: args.clone(),
				},
				expr.1,
			)),

			Expr::Call { name, args } if name == "float" => self.expr(&(
				Expr::Call {
					name: "f64".to_owned(),
					args: args.clone(),
				},
				expr.1,
			)),

			Expr::Call { name, args }
				if name.starts_with('i')
					&& name[1..]
						.parse::<u16>()
						.ok()
						.is_some_and(|w| w > 0 && w <= 64) =>
			{
				let target: u16 = name[1..].parse().unwrap();
				if args.len() != 1 {
					return Err(Diagnostic::new(
						format!("`{name}` cast takes exactly 1 argument"),
						expr.1.into_range(),
					)
					.with_label("wrong number of arguments"));
				}
				let (val, typ) = self.expr(&args[0])?;
				let target_cl = cl_type(&Typ::Int(target), self.int);
				let out = match &typ {
					Typ::Int(w) if *w == target => val,
					Typ::Int(_) => self.clamp_to_width(
						val,
						true,
						Some((int_min(target), false)),
						int_max(target),
						false,
						target_cl,
					),
					Typ::Enum(enum_name) => {
						let tag = self.enum_tag(enum_name, val);
						if target_cl == types::I64 {
							tag
						} else {
							self.b.ins().ireduce(target_cl, tag)
						}
					}
					_ => {
						return Err(Diagnostic::new(
							format!("cannot cast {typ:?} to i{target}"),
							args[0].1.into_range(),
						)
						.with_label("not an integer"));
					}
				};
				Ok((out, Typ::Int(target)))
			}

			Expr::Call { name, args }
				if name.starts_with('u')
					&& name[1..]
						.parse::<u16>()
						.ok()
						.is_some_and(|w| w > 0 && w <= 64) =>
			{
				let target: u16 = name[1..].parse().unwrap();
				if args.len() != 1 {
					return Err(Diagnostic::new(
						format!("`{name}` cast takes exactly 1 argument"),
						expr.1.into_range(),
					)
					.with_label("wrong number of arguments"));
				}
				let (val, typ) = self.expr(&args[0])?;
				let target_cl = cl_type(&Typ::UInt(target), self.int);
				let out = match &typ {
					Typ::UInt(w) if *w == target => val,
					Typ::UInt(_) => {
						self.clamp_to_width(val, false, None, uint_max(target), true, target_cl)
					}
					Typ::Int(_) => self.clamp_to_width(
						val,
						true,
						Some((0, false)),
						uint_max(target),
						true,
						target_cl,
					),
					_ => {
						return Err(Diagnostic::new(
							format!("cannot cast {typ:?} to u{target}"),
							args[0].1.into_range(),
						)
						.with_label("not an integer"));
					}
				};
				Ok((out, Typ::UInt(target)))
			}

			Expr::Call { name, args }
				if matches!(name.as_str(), "f16" | "f32" | "f64" | "f128") =>
			{
				let target: u16 = match name.as_str() {
					"f16" => 16,
					"f32" => 32,
					"f128" => 128,
					_ => 64,
				};
				if args.len() != 1 {
					return Err(Diagnostic::new(
						format!("`{name}` cast takes exactly 1 argument"),
						expr.1.into_range(),
					)
					.with_label("wrong number of arguments"));
				}
				if target == 16 || target == 128 {
					return Err(Diagnostic::new(
						format!("f{target} casts are not yet supported by the JIT backend"),
						expr.1.into_range(),
					)
					.with_label("not yet implemented"));
				}
				let (val, typ) = self.expr(&args[0])?;
				let target_cl = cl_type(&Typ::Float(target), self.int);
				let out = match &typ {
					Typ::Float(w) if *w == target => val,
					Typ::Float(_) if target == 64 => self.b.ins().fpromote(types::F64, val),
					Typ::Float(_) => self.b.ins().fdemote(types::F32, val),
					Typ::Int(_) => self.b.ins().fcvt_from_sint(target_cl, val),
					_ => {
						return Err(Diagnostic::new(
							format!("cannot cast {typ:?} to f{target}"),
							args[0].1.into_range(),
						)
						.with_label("not a number"));
					}
				};
				Ok((out, Typ::Float(target)))
			}

			Expr::Call { name, args } => {
				let sig = self.funcs.get(name).cloned().ok_or_else(|| {
					Diagnostic::new(format!("undefined function `{name}`"), expr.1.into_range())
						.with_label("not defined")
				})?;
				self.call_sig(name, sig, None, args, expr.1)
			}

			Expr::MethodCall { recv, method, args } => {
				// enum payload
				if let Expr::Ident(name) = &recv.0
					&& !self.vars.contains_key(name)
					&& self.enums.contains_key(name)
				{
					return self.construct_variant(name, method, args, expr.1);
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
							let s = self.enum_name_str(enum_name, recv_val);
							return Ok((s, Typ::Str));
						}
						return Err(Diagnostic::new(
							format!("enum `{enum_name}` has no method `{method}`"),
							expr.1.into_range(),
						)
						.with_label("no such method"));
					}
					match &recv_typ {
						Typ::Struct(name, _) => (name.clone(), Some(recv_val)),
						_ => {
							return Err(Diagnostic::new(
								format!("`{recv_typ}` has no methods"),
								recv.1.into_range(),
							)
							.with_label("methods are only defined on structs"));
						}
					}
				};
				let key = format!("{sname}.{method}");
				let sig = self.funcs.get(&key).cloned().ok_or_else(|| {
					Diagnostic::new(
						format!("`{sname}` has no method `{method}`"),
						expr.1.into_range(),
					)
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
					self.b
						.ins()
						.store(MemFlags::new(), val, ptr, (i * 8) as i32);
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
						Err(_) => Err(Diagnostic::new(
							format!("arrays have no field `{field}`"),
							expr.1.into_range(),
						)
						.with_label("arrays only have `.len` and numeric indices")),
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
						return Err(Diagnostic::new(
							format!("cannot access a field of {typ:?}"),
							tuple.1.into_range(),
						)
						.with_label("not a tuple"));
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
							Diagnostic::new(
								format!("tuple has no field `{field}`"),
								expr.1.into_range(),
							)
							.with_label("no such field")
						})?,
				};
				let field_typ = fields[idx].1.clone();
				let cl = cl_type(&field_typ, self.int);
				let v = self
					.b
					.ins()
					.load(cl, MemFlags::new(), ptr, (idx * 8) as i32);
				Ok((v, field_typ))
			}

			Expr::Array(elems) => {
				if elems.is_empty() {
					return Err(Diagnostic::new(
						"empty array literals aren't supported yet",
						expr.1.into_range(),
					)
					.with_label("needs at least one element to infer its type"));
				}
				let mut elem_typ: Option<Typ> = None;
				let mut vals = Vec::with_capacity(elems.len());
				for e in elems {
					let (val, typ) = self.expr(e)?;
					match &elem_typ {
						Some(t) if t != &typ => {
							return Err(Diagnostic::new(
								format!(
									"array elements must share a type: expected {t:?}, got {typ:?}"
								),
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
					self.b
						.ins()
						.store(MemFlags::new(), val, data, (i as i64 * size) as i32);
				}
				let len = self.b.ins().iconst(self.int, elems.len() as i64);
				let header = self.make_array(data, len);
				Ok((header, Typ::Array(Box::new(elem))))
			}

			Expr::ArrayInit((te, span)) => {
				let typ = resolve_type(te, *span, self.structs, self.enums, &HashMap::new())?;
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

			Expr::Slice {
				collection,
				start,
				end,
			} => {
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

			Expr::If { cond, then, els } => {
				match self.conditional(cond, then, els.as_deref(), expr.1)? {
					Some((v, t)) => Ok((v, t)),
					None => Err(Diagnostic::new(
						"this `if` never produces a value",
						expr.1.into_range(),
					)
					.with_label("every branch returns, but a value is needed here")),
				}
			}

			Expr::Match {
				subject,
				arms,
				else_body,
			} => match self.match_expr(subject, arms, else_body.as_deref(), expr.1)? {
				Some((v, t)) => Ok((v, t)),
				None => Err(Diagnostic::new(
					"this `match` never produces a value",
					expr.1.into_range(),
				)
				.with_label("every arm returns, but a value is needed here")),
			},

			Expr::Loop { cond, body } => match self.loop_expr(cond.as_deref(), body)? {
				Some(vt) => Ok(vt),
				None => Err(Diagnostic::new(
					"this `loop` never produces a value",
					expr.1.into_range(),
				)
				.with_label("an infinite loop with no `break` yields nothing")),
			},

			Expr::For { pat, iter, body } => self.for_loop(pat, iter, body, expr.1),

			Expr::In(lhs, rhs) => {
				let (rhs_val, rhs_typ) = self.expr(rhs)?;

				// substring check
				if rhs_typ == Typ::Str {
					let (lhs_val, lhs_typ) = self.expr(lhs)?;
					if lhs_typ != Typ::Str {
						return Err(Diagnostic::new(
							format!("cannot search {lhs_typ:?} in Str"),
							lhs.1.into_range(),
						)
						.with_label("type mismatch: value must be Str"));
					}
					let func = self.import_fn(
						runtime::STR_CONTAINS,
						&[self.int, self.int],
						Some(self.int),
					);
					let call = self.b.ins().call(func, &[rhs_val, lhs_val]);
					return Ok((self.b.inst_results(call)[0], Typ::Bool));
				}

				let elem = match rhs_typ {
					Typ::Array(ref e) => (**e).clone(),
					_ => {
						return Err(Diagnostic::new(
							format!("right side of `in` must be an array or Str, got {rhs_typ:?}"),
							rhs.1.into_range(),
						)
						.with_label("not an array or string"));
					}
				};
				let (val, val_typ) = self.expr(lhs)?;
				if val_typ != elem {
					return Err(Diagnostic::new(
						format!("cannot search {val_typ:?} in {elem:?} array"),
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
				let elem_val =
					self.b
						.ins()
						.load(cl_type(&elem, self.int), MemFlags::new(), addr, 0);
				let equal = self.emit_eq(val, elem_val, &elem);
				self.b
					.ins()
					.brif(equal, found_block, &[], continue_block, &[]);
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
						Diagnostic::new(
							"`Self` is only valid in an impl block",
							expr.1.into_range(),
						)
						.with_label("no enclosing impl")
					})?,
					_ => name.clone(),
				};
				let struct_fields = self.structs.get(name.as_str()).cloned().ok_or_else(|| {
					Diagnostic::new(format!("unknown struct `{name}`"), expr.1.into_range())
						.with_label("not defined")
				})?;
				let size = (struct_fields.len() * 8) as u32;
				let slot = self.b.create_sized_stack_slot(StackSlotData::new(
					StackSlotKind::ExplicitSlot,
					size,
					0,
				));
				let ptr = self.b.ins().stack_addr(self.int, slot, 0);

				for (i, f) in struct_fields.iter().enumerate() {
					let init = if let Some(default_expr) = &f.default {
						let (val, vtyp) = self.expr(default_expr)?;
						if vtyp != f.typ {
							return Err(Diagnostic::new(
								format!(
									"default value type mismatch: expected {:?}, got {vtyp:?}",
									f.typ
								),
								default_expr.1.into_range(),
							)
							.with_label("type mismatch"));
						}
						val
					} else {
						self.zero(&f.typ)
					};
					self.b
						.ins()
						.store(MemFlags::new(), init, ptr, (i * 8) as i32);
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
									format!("expected {expected:?}, got {vtyp:?}"),
									value.1.into_range(),
								)
								.with_label("type mismatch"));
							}
							self.b
								.ins()
								.store(MemFlags::new(), val, ptr, (i * 8) as i32);
						}
					} else {
						for (field_name, value) in fields {
							let fname = field_name.as_deref().ok_or_else(|| {
								Diagnostic::new(
									"cannot mix named and positional fields",
									value.1.into_range(),
								)
								.with_label("missing field name")
							})?;
							let idx = struct_fields
								.iter()
								.position(|f| f.name == fname)
								.ok_or_else(|| {
									Diagnostic::new(
										format!("`{name}` has no field `{fname}`"),
										value.1.into_range(),
									)
									.with_label("no such field")
								})?;
							let expected = struct_fields[idx].typ.clone();
							let (val, vtyp) = self.check_expr(value, &expected)?;
							if vtyp != expected {
								return Err(Diagnostic::new(
									format!("expected {expected:?}, got {vtyp:?}"),
									value.1.into_range(),
								)
								.with_label("type mismatch"));
							}
							self.b
								.ins()
								.store(MemFlags::new(), val, ptr, (idx * 8) as i32);
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

	fn str_const(&mut self, s: &str) -> Value {
		let mut bytes = s.as_bytes().to_vec();
		bytes.push(0);
		let name = format!("__str_{}", *self.string_idx);
		*self.string_idx += 1;
		let id = self
			.module
			.declare_data(&name, Linkage::Local, false, false)
			.unwrap();
		let mut desc = DataDescription::new();
		desc.define(bytes.into_boxed_slice());
		self.module.define_data(id, &desc).unwrap();
		let gv = self.module.declare_data_in_func(id, self.b.func);
		self.b.ins().symbol_value(self.int, gv)
	}

	// Intern an atom name to a pointer-sized symbol.
	fn atom_const(&mut self, name: &str) -> Value {
		let sym = format!("__atom_{name}");
		if !self.atoms.contains_key(name) {
			let id = self
				.module
				.declare_data(&sym, Linkage::Local, false, false)
				.unwrap();
			let mut bytes = format!(":{name}").into_bytes();
			bytes.push(0);
			let mut desc = DataDescription::new();
			desc.define(bytes.into_boxed_slice());
			self.module.define_data(id, &desc).unwrap();
			self.atoms.insert(name.to_string(), ());
		}
		let id = self
			.module
			.declare_data(&sym, Linkage::Local, false, false)
			.unwrap();
		let gv = self.module.declare_data_in_func(id, self.b.func);
		self.b.ins().symbol_value(self.int, gv)
	}

	fn emit_eq(&mut self, a: Value, b: Value, typ: &Typ) -> Value {
		match typ {
			Typ::Float(_) => self.b.ins().fcmp(FloatCC::Equal, a, b),
			Typ::Str => {
				let func = self.import_fn(runtime::STR_EQ, &[self.int, self.int], Some(self.int));
				let call = self.b.ins().call(func, &[a, b]);
				self.b.inst_results(call)[0]
			}
			_ => self.b.ins().icmp(IntCC::Equal, a, b),
		}
	}

	// Sign-extend the low `w` bits of `val` within its Cranelift container.
	// A no-op for standard widths (8, 16, 32, 64).
	fn reduce_int(&mut self, val: Value, w: u16) -> Value {
		let cl = cl_type(&Typ::Int(w), self.int);
		let shift = cl.bits() as i64 - w as i64;
		if shift == 0 {
			return val;
		}
		let shift_v = self.b.ins().iconst(cl, shift);
		let up = self.b.ins().ishl(val, shift_v);
		self.b.ins().sshr(up, shift_v)
	}

	// Zero-extend (mask) `val` to exactly `w` bits within its Cranelift container.
	fn reduce_uint(&mut self, val: Value, w: u16) -> Value {
		let cl = cl_type(&Typ::UInt(w), self.int);
		if cl.bits() as u16 == w {
			return val;
		}
		let mask = ((1u64 << w) - 1) as i64;
		let mask_v = self.b.ins().iconst(cl, mask);
		self.b.ins().band(val, mask_v)
	}

	fn zero(&mut self, typ: &Typ) -> Value {
		match typ {
			Typ::Float(16) => self.b.ins().f16const(Ieee16::with_bits(0)),
			Typ::Float(32) => self.b.ins().f32const(0.0),
			Typ::Float(64) => self.b.ins().f64const(0.0),
			Typ::Float(128) => {
				let c = self
					.b
					.func
					.dfg
					.constants
					.insert(Ieee128::with_bits(0).into());
				self.b.ins().f128const(c)
			}
			Typ::Float(w) => panic!("unsupported float width f{w}"),
			Typ::Str => self.str_const(""),
			Typ::Atom => self.atom_const(""),
			Typ::Int(w) => self.b.ins().iconst(cl_type(&Typ::Int(*w), self.int), 0),
			Typ::UInt(w) => self.b.ins().iconst(cl_type(&Typ::UInt(*w), self.int), 0),
			Typ::Bool | Typ::ISize | Typ::USize => self.b.ins().iconst(self.int, 0),
			// default to first variant
			Typ::Enum(name) => {
				let disc = self
					.enums
					.get(name)
					.and_then(|vs| vs.first())
					.map_or(0, |v| v.disc);
				self.make_enum(name, disc, &[])
			}
			Typ::Tuple(fields) if fields.is_empty() => self.b.ins().iconst(self.int, 0),
			Typ::Struct(_, fields) => {
				let fields = fields.clone();
				let size = (fields.len() * 8) as u32;
				let slot = self.b.create_sized_stack_slot(StackSlotData::new(
					StackSlotKind::ExplicitSlot,
					size,
					0,
				));
				let ptr = self.b.ins().stack_addr(self.int, slot, 0);
				for (i, f) in fields.iter().enumerate() {
					let z = self.zero(&f.typ);
					self.b.ins().store(MemFlags::new(), z, ptr, (i * 8) as i32);
				}
				ptr
			}
			Typ::Tuple(fields) => {
				let fields = fields.clone();
				let ptr = self.call_alloc(fields.len());
				for (i, (_, ftyp)) in fields.iter().enumerate() {
					let z = self.zero(ftyp);
					self.b.ins().store(MemFlags::new(), z, ptr, (i * 8) as i32);
				}
				ptr
			}
			Typ::Array(_) => {
				let zero = self.b.ins().iconst(self.int, 0);
				self.make_array(zero, zero)
			}
			Typ::FixedArray(elem, n) => {
				let elem = (**elem).clone();
				let stride = elem_size(&elem);
				let slot = self.b.create_sized_stack_slot(StackSlotData::new(
					StackSlotKind::ExplicitSlot,
					(*n as i64 * stride) as u32,
					0,
				));
				let ptr = self.b.ins().stack_addr(self.int, slot, 0);
				for i in 0..*n {
					let z = self.zero(&elem);
					self.b
						.ins()
						.store(MemFlags::new(), z, ptr, (i as i64 * stride) as i32);
				}
				ptr
			}
			Typ::Range => {
				let ptr = self.call_alloc(2);
				let z = self.b.ins().iconst(self.int, 0);
				self.b.ins().store(MemFlags::new(), z, ptr, 0);
				self.b.ins().store(MemFlags::new(), z, ptr, 8);
				ptr
			}
		}
	}

	// A numeric literal takes the binding's declared type directly.
	fn coerce_lit(
		&mut self,
		value: &Spanned<Expr>,
		target: &Typ,
	) -> Result<Option<Value>, Diagnostic> {
		let (neg, inner) = match &value.0 {
			Expr::Negative(e) => (true, &e.0),
			v => (false, v),
		};
		let oob = |n| {
			Diagnostic::new(
				format!("{n} is out of range for {target}"),
				value.1.into_range(),
			)
			.with_label(format!("doesn't fit in {target}"))
		};
		let v = match (inner, target) {
			(Expr::Int(n), Typ::Int(w)) => {
				let n = if neg { -*n } else { *n };
				if n < int_min(*w) || n > int_max(*w) {
					return Err(oob(n));
				}
				self.b.ins().iconst(cl_int_for_width(*w), n)
			}
			(Expr::Int(n), Typ::UInt(w)) => {
				let n = if neg { -*n } else { *n };
				if n < 0 || (*w < 64 && n > uint_max(*w)) {
					return Err(oob(n));
				}
				self.b.ins().iconst(cl_int_for_width(*w), n)
			}
			(Expr::Int(n), Typ::ISize) => self.b.ins().iconst(self.int, if neg { -*n } else { *n }),
			(Expr::Int(n), Typ::USize) => {
				let n = if neg { -*n } else { *n };
				if n < 0 {
					return Err(oob(n));
				}
				self.b.ins().iconst(self.int, n)
			}
			(Expr::Int(n), Typ::Float(w)) => {
				self.float_lit((if neg { -*n } else { *n }) as f64, *w, value.1)?
			}
			(Expr::Float(x), Typ::Float(w)) => {
				self.float_lit(if neg { -*x } else { *x }, *w, value.1)?
			}
			(Expr::Atom(name), Typ::Enum(typ)) => {
				self.construct_variant(typ, name, &[], value.1)?.0
			}
			(Expr::EnumShorthand { variant, args }, Typ::Enum(typ)) => {
				self.construct_variant(typ, variant, args, value.1)?.0
			}
			_ => return Ok(None),
		};
		Ok(Some(v))
	}

	// The tag of an enum value.
	fn enum_tag(&mut self, name: &str, val: Value) -> Value {
		if enum_boxed(self.enums.get(name).map(Vec::as_slice).unwrap_or(&[])) {
			self.b.ins().load(self.int, MemFlags::new(), val, 0)
		} else {
			val
		}
	}

	// Build a variant value.
	// A bare discriminant for fieldless enums, and a heap where that's not possible.
	fn make_enum(&mut self, name: &str, disc: i64, fields: &[Value]) -> Value {
		let slots = enum_slots(self.enums.get(name).map(Vec::as_slice).unwrap_or(&[]));
		if slots == 1 {
			return self.b.ins().iconst(self.int, disc);
		}
		let ptr = self.call_alloc(slots);
		let tag = self.b.ins().iconst(self.int, disc);
		self.b.ins().store(MemFlags::new(), tag, ptr, 0);
		for (i, fv) in fields.iter().enumerate() {
			self.b
				.ins()
				.store(MemFlags::new(), *fv, ptr, ((i + 1) * 8) as i32);
		}
		ptr
	}

	// A match pattern's discriminant and payload binds.
	fn enum_pattern(
		&self,
		pat: &Spanned<Expr>,
		enum_name: &str,
	) -> Result<(i64, Vec<Bind>), Diagnostic> {
		let bad = |msg| Err(Diagnostic::new(msg, pat.1.into_range()).with_label("bad pattern"));
		let (variant, args): (&str, &[Spanned<Expr>]) = match &pat.0 {
			Expr::EnumShorthand { variant, args } => (variant, args),
			Expr::Atom(v) => (v, &[]),
			Expr::Field { tuple, field } if matches!(tuple.0, Expr::Ident(_)) => (field, &[]),
			_ => return bad(format!("`{enum_name}` is matched by its variants")),
		};
		let Some(v) = self.enums[enum_name].iter().find(|v| v.name == variant) else {
			return bad(format!("enum `{enum_name}` has no variant `{variant}`"));
		};
		let binds = field_binds(args.iter().zip(&v.payload), 8, 8)?;
		Ok((v.disc, binds))
	}

	fn range_pattern(
		&mut self,
		sv: Value,
		st: &Typ,
		start: Option<&Spanned<Expr>>,
		end: Option<&Spanned<Expr>>,
		span: Span,
	) -> Result<Value, Diagnostic> {
		let Typ::Int(_) = st else {
			let msg = format!("range patterns need an integer subject, got {st}");
			return Err(Diagnostic::new(msg, span.into_range()).with_label("not an integer"));
		};
		let mut cond = self.b.ins().iconst(types::I8, 1);
		for (bound, cc) in [
			(start, IntCC::SignedGreaterThanOrEqual),
			(end, IntCC::SignedLessThan),
		] {
			if let Some(e) = bound {
				let (bv, _) = self.check_expr(e, st)?;
				let c = self.b.ins().icmp(cc, sv, bv);
				cond = self.b.ins().band(cond, c);
			}
		}
		Ok(cond)
	}

	// Make and check enum variant.
	fn construct_variant(
		&mut self,
		name: &str,
		variant: &str,
		args: &[Spanned<Expr>],
		span: Span,
	) -> Result<(Value, Typ), Diagnostic> {
		let v = self
			.enums
			.get(name)
			.and_then(|vs| vs.iter().find(|v| v.name == variant))
			.ok_or_else(|| {
				Diagnostic::new(
					format!("enum `{name}` has no variant `{variant}`"),
					span.into_range(),
				)
				.with_label("no such variant")
			})?;
		let (disc, payload) = (v.disc, v.payload.clone());
		if args.len() != payload.len() {
			let msg = format!(
				"`{name}.{variant}` takes {} field(s), got {}",
				payload.len(),
				args.len()
			);
			return Err(
				Diagnostic::new(msg, span.into_range()).with_label("wrong number of fields")
			);
		}
		let mut fields = Vec::with_capacity(args.len());
		for (arg, ft) in args.iter().zip(&payload) {
			let (fv, at) = self.check_expr(arg, ft)?;
			if at != *ft {
				let msg = format!("expected {ft}, got {at}");
				return Err(Diagnostic::new(msg, arg.1.into_range()).with_label("type mismatch"));
			}
			fields.push(fv);
		}
		let val = self.make_enum(name, disc, &fields);
		Ok((val, Typ::Enum(name.to_string())))
	}

	// Evaluate `value` against an expected type, resolving `.variant` shorthands and atoms via coercion.
	fn check_expr(
		&mut self,
		value: &Spanned<Expr>,
		target: &Typ,
	) -> Result<(Value, Typ), Diagnostic> {
		if matches!(value.0, Expr::EnumShorthand { .. } | Expr::Atom(_))
			&& let Some(v) = self.coerce_lit(value, target)?
		{
			return Ok((v, target.clone()));
		}
		self.expr(value)
	}

	fn float_lit(&mut self, x: f64, w: u16, span: Span) -> Result<Value, Diagnostic> {
		match w {
			32 => Ok(self.b.ins().f32const(x as f32)),
			64 => Ok(self.b.ins().f64const(x)),
			_ => Err(Diagnostic::new(
				format!("f{w} literals aren't supported by the JIT backend yet"),
				span.into_range(),
			)
			.with_label("not yet implemented")),
		}
	}

	fn struct_copy(&mut self, src: Value, fields: &[FieldDef]) -> Value {
		let size = (fields.len() * 8) as u32;
		let slot = self.b.create_sized_stack_slot(StackSlotData::new(
			StackSlotKind::ExplicitSlot,
			size,
			0,
		));
		let dst = self.b.ins().stack_addr(self.int, slot, 0);
		for (i, f) in fields.iter().enumerate() {
			let cl = cl_type(&f.typ, self.int);
			let fv = self.b.ins().load(cl, MemFlags::new(), src, (i * 8) as i32);
			self.b.ins().store(MemFlags::new(), fv, dst, (i * 8) as i32);
		}
		dst
	}

	fn fixed_copy(&mut self, src: Value, elem: &Typ, n: usize) -> Value {
		let stride = elem_size(elem);
		let cl = cl_type(elem, self.int);
		let slot = self.b.create_sized_stack_slot(StackSlotData::new(
			StackSlotKind::ExplicitSlot,
			(n as i64 * stride) as u32,
			0,
		));
		let dst = self.b.ins().stack_addr(self.int, slot, 0);
		for i in 0..n {
			let off = (i as i64 * stride) as i32;
			let v = self.b.ins().load(cl, MemFlags::new(), src, off);
			self.b.ins().store(MemFlags::new(), v, dst, off);
		}
		dst
	}

	fn binop(
		&mut self,
		op: Op,
		l: &Spanned<Expr>,
		r: &Spanned<Expr>,
		span: Span,
	) -> Result<(Value, Typ), Diagnostic> {
		let (lv, lt) = self.expr(l)?;
		let (rv, rt) = self.expr(r)?;

		// string concatenation
		if let (Op::Add, Typ::Str, Typ::Str) = (op, &lt, &rt) {
			return Ok((self.call_concat(lv, rv), Typ::Str));
		}

		#[derive(Clone, Copy)]
		enum NumKind {
			Int,
			UInt,
			Float,
		}
		// NOTE: might go with V-style int/float promotion eventually
		let kind = match (&lt, &rt) {
			(Typ::Int(lw), Typ::Int(rw)) if lw == rw => NumKind::Int,
			(Typ::ISize, Typ::ISize) => NumKind::Int,
			(Typ::UInt(lw), Typ::UInt(rw)) if lw == rw => NumKind::UInt,
			(Typ::USize, Typ::USize) => NumKind::UInt,
			(Typ::Float(lw), Typ::Float(rw)) if lw == rw => NumKind::Float,
			_ => {
				return Err(Diagnostic::new(
					format!("cannot {op:?} {lt:?} and {rt:?}"),
					span.into_range(),
				)
				.with_label("operands have mismatched types"));
			}
		};
		if let (Op::Mod, NumKind::Float) = (op, kind) {
			// TODO: cranelift has no float remainder
			return Err(Diagnostic::new(
				"`%` is not yet supported on floats".to_string(),
				span.into_range(),
			)
			.with_label("only integer operands"));
		}
		let b = self.b.ins();
		let out = match (op, kind) {
			(Op::Add, NumKind::Float) => b.fadd(lv, rv),
			(Op::Add, _) => b.iadd(lv, rv),
			(Op::Sub, NumKind::Float) => b.fsub(lv, rv),
			(Op::Sub, _) => b.isub(lv, rv),
			(Op::Mul, NumKind::Float) => b.fmul(lv, rv),
			(Op::Mul, _) => b.imul(lv, rv),
			(Op::Div, NumKind::Float) => b.fdiv(lv, rv),
			(Op::Div, NumKind::UInt) => b.udiv(lv, rv),
			(Op::Div, NumKind::Int) => b.sdiv(lv, rv),
			(Op::Mod, NumKind::Float) => unreachable!("float `%` rejected above"),
			(Op::Mod, NumKind::UInt) => b.urem(lv, rv),
			(Op::Mod, NumKind::Int) => b.srem(lv, rv),
		};
		// For non-standard widths, wrap the result back to the declared bit width.
		let out = match &lt {
			Typ::Int(w) if cl_type(&Typ::Int(*w), self.int).bits() as u16 != *w => {
				self.reduce_int(out, *w)
			}
			Typ::UInt(w) if cl_type(&Typ::UInt(*w), self.int).bits() as u16 != *w => {
				self.reduce_uint(out, *w)
			}
			_ => out,
		};
		Ok((out, lt))
	}

	fn cmp(
		&mut self,
		icc: IntCC,
		fcc: FloatCC,
		l: &Spanned<Expr>,
		r: &Spanned<Expr>,
		span: Span,
	) -> Result<(Value, Typ), Diagnostic> {
		// evaluate the typed/pinned side first so a `.variant` shorthand can borrow its enum type
		let ((lv, lt), (rv, rt)) = if let Expr::EnumShorthand { .. } = &l.0 {
			let (rv, rt) = self.expr(r)?;
			(self.check_expr(l, &rt)?, (rv, rt))
		} else {
			let (lv, lt) = self.expr(l)?;
			let rhs = self.check_expr(r, &lt)?;
			((lv, lt), rhs)
		};

		// () == ()
		if let (Typ::Tuple(lf), Typ::Tuple(rf)) = (&lt, &rt) {
			if lf.is_empty() && rf.is_empty() {
				let result = match icc {
					IntCC::Equal => self.b.ins().iconst(self.int, 1),
					IntCC::NotEqual => self.b.ins().iconst(self.int, 0),
					_ => {
						return Err(Diagnostic::new(
							"unit type `()` only supports `==` and `!=`",
							span.into_range(),
						)
						.with_label("unsupported comparison"));
					}
				};
				return Ok((result, Typ::Bool));
			}
		}

		let icc = if matches!(
			(&lt, &rt),
			(Typ::UInt(_), Typ::UInt(_)) | (Typ::USize, Typ::USize)
		) {
			unsigned_cc(icc)
		} else {
			icc
		};
		let raw = match (&lt, &rt) {
			(Typ::Int(_), Typ::Int(_))
			| (Typ::UInt(_), Typ::UInt(_))
			| (Typ::ISize, Typ::ISize)
			| (Typ::USize, Typ::USize)
			| (Typ::Bool, Typ::Bool)
			| (Typ::Atom, Typ::Atom) => self.b.ins().icmp(icc, lv, rv),
			(Typ::Enum(a), Typ::Enum(b)) if a == b => {
				if enum_boxed(self.enums.get(a).map(Vec::as_slice).unwrap_or(&[])) {
					return Err(Diagnostic::new(
						format!("`{a}` has payloads, so `==` isn't supported yet"),
						span.into_range(),
					)
					.with_label("match on the variant instead"));
				}
				self.b.ins().icmp(icc, lv, rv)
			}
			(Typ::Float(_), Typ::Float(_)) => self.b.ins().fcmp(fcc, lv, rv),
			(Typ::Str, Typ::Str) if icc == IntCC::Equal || icc == IntCC::NotEqual => {
				let eq = self.emit_eq(lv, rv, &Typ::Str);
				// emit_eq returns 1 for equal, invert for Ne
				// wrap in icmp so uextend below works consistently
				if icc == IntCC::NotEqual {
					self.b.ins().icmp_imm(IntCC::Equal, eq, 0)
				} else {
					self.b.ins().icmp_imm(IntCC::NotEqual, eq, 0)
				}
			}
			_ => {
				return Err(Diagnostic::new(
					format!("cannot compare {lt:?} and {rt:?}"),
					span.into_range(),
				)
				.with_label("operands have mismatched types"));
			}
		};
		let out = self.b.ins().uextend(self.int, raw);
		Ok((out, Typ::Bool))
	}

	// Short-circuits. `&&` only evaluates the right side when the left is true, and `||` does the inverse.
	fn logical(
		&mut self,
		and: bool,
		l: &Spanned<Expr>,
		r: &Spanned<Expr>,
	) -> Result<(Value, Typ), Diagnostic> {
		let (lv, lt) = self.expr(l)?;
		if lt != Typ::Bool {
			return Err(
				Diagnostic::new(format!("expected Bool, got {lt:?}"), l.1.into_range())
					.with_label("logical operators need Bool operands"),
			);
		}

		// result defaults to the short-circuit value
		let result = self.b.declare_var(self.int);
		let short = self.b.ins().iconst(self.int, if and { 0 } else { 1 });
		self.b.def_var(result, short);

		let rhs_block = self.b.create_block();
		let merge = self.b.create_block();
		let (then, els) = if and {
			(rhs_block, merge)
		} else {
			(merge, rhs_block)
		};
		self.b.ins().brif(lv, then, &[], els, &[]);

		self.b.switch_to_block(rhs_block);
		self.b.seal_block(rhs_block);
		let (rv, rt) = self.expr(r)?;
		if rt != Typ::Bool {
			return Err(
				Diagnostic::new(format!("expected Bool, got {rt:?}"), r.1.into_range())
					.with_label("logical operators need Bool operands"),
			);
		}
		self.b.def_var(result, rv);
		self.b.ins().jump(merge, &[]);

		self.b.switch_to_block(merge);
		self.b.seal_block(merge);
		Ok((self.b.use_var(result), Typ::Bool))
	}

	fn import_fn(
		&mut self,
		name: &str,
		params: &[types::Type],
		ret: Option<types::Type>,
	) -> codegen::ir::FuncRef {
		let mut sig = self.module.make_signature();
		for &p in params {
			sig.params.push(AbiParam::new(p));
		}
		if let Some(r) = ret {
			sig.returns.push(AbiParam::new(r));
		}
		let id = self
			.module
			.declare_function(name, Linkage::Import, &sig)
			.unwrap();
		self.module.declare_func_in_func(id, self.b.func)
	}

	// Emit a call to a resolved fn.
	fn call_sig(
		&mut self,
		name: &str,
		sig: FnSig,
		recv: Option<Value>,
		args: &[Spanned<Expr>],
		span: Span,
	) -> Result<(Value, Typ), Diagnostic> {
		let self_n = recv.is_some() as usize;
		if args.len() + self_n != sig.params.len() {
			return Err(Diagnostic::new(
				format!(
					"`{name}` expects {} argument(s), got {}",
					sig.params.len() - self_n,
					args.len()
				),
				span.into_range(),
			)
			.with_label("wrong number of arguments"));
		}
		let mut vals = Vec::with_capacity(args.len() + self_n);
		let mut expected = sig.params.iter();
		if let Some(recv) = recv {
			expected.next();
			vals.push(recv);
		}
		for arg in args {
			let (val, typ) = self.expr(arg)?;
			let want = expected.next().unwrap();
			if &typ != want {
				return Err(Diagnostic::new(
					format!("expected {want} argument, got {typ}"),
					arg.1.into_range(),
				)
				.with_label("wrong argument type"));
			}
			vals.push(val);
		}
		let func = self.module.declare_func_in_func(sig.id, self.b.func);
		let call = self.b.ins().call(func, &vals);
		let ret_val = if matches!(sig.ret, Typ::Tuple(ref f) if f.is_empty()) {
			self.b.ins().iconst(self.int, 0)
		} else {
			self.b.inst_results(call)[0]
		};
		Ok((ret_val, sig.ret))
	}

	fn call_concat(&mut self, a: Value, b: Value) -> Value {
		let func = self.import_fn(runtime::STR_CONCAT, &[self.int, self.int], Some(self.int));
		let call = self.b.ins().call(func, &[a, b]);
		self.b.inst_results(call)[0]
	}

	fn call_alloc(&mut self, n: usize) -> Value {
		self.call_alloc_bytes((n * 8) as i64)
	}

	fn call_alloc_bytes(&mut self, bytes: i64) -> Value {
		let func = self.import_fn(runtime::ALLOC, &[self.int], Some(self.int));
		let size = self.b.ins().iconst(self.int, bytes);
		let call = self.b.ins().call(func, &[size]);
		self.b.inst_results(call)[0]
	}

	// array handle: { data @ 0, len @ 8, cap @ 16 }
	fn array_data(&mut self, header: Value) -> Value {
		self.b.ins().load(self.int, MemFlags::new(), header, 0)
	}
	fn array_len(&mut self, header: Value) -> Value {
		self.b.ins().load(self.int, MemFlags::new(), header, 8)
	}
	fn array_cap(&mut self, header: Value) -> Value {
		self.b.ins().load(self.int, MemFlags::new(), header, 16)
	}
	fn make_array(&mut self, data: Value, len: Value) -> Value {
		let header = self.call_alloc(3);
		self.b.ins().store(MemFlags::new(), data, header, 0);
		self.b.ins().store(MemFlags::new(), len, header, 8);
		self.b.ins().store(MemFlags::new(), len, header, 16);
		header
	}

	// Evaluate an array-typed operand, returning its value and type.
	fn array_operand(
		&mut self,
		collection: &Spanned<Expr>,
		what: &str,
	) -> Result<(Value, Typ), Diagnostic> {
		let (ptr, typ) = self.expr(collection)?;
		match typ {
			Typ::Array(_) | Typ::FixedArray(..) => Ok((ptr, typ)),
			_ => Err(
				Diagnostic::new(format!("cannot {what} {typ:?}"), collection.1.into_range())
					.with_label("not an array"),
			),
		}
	}

	// (data pointer, length) for an array.
	fn array_parts(&mut self, val: Value, typ: &Typ) -> (Value, Value) {
		match typ {
			Typ::FixedArray(_, n) => (val, self.b.ins().iconst(self.int, *n as i64)),
			_ => (self.array_data(val), self.array_len(val)),
		}
	}

	fn int_value(&mut self, e: &Spanned<Expr>, what: &str) -> Result<Value, Diagnostic> {
		let (v, t) = self.expr(e)?;
		if !matches!(t, Typ::Int(_)) {
			return Err(Diagnostic::new(
				format!("{what} must be Int, got {t:?}"),
				e.1.into_range(),
			)
			.with_label("not an Int"));
		}
		Ok(v)
	}

	// Bounds-check `idx` and return the element address.
	fn elem_addr(&mut self, data: Value, len: Value, elem: &Typ, idx: Value) -> Value {
		let oob = self
			.b
			.ins()
			.icmp(IntCC::UnsignedGreaterThanOrEqual, idx, len);

		let panic_block = self.b.create_block();
		let ok_block = self.b.create_block();
		self.b.ins().brif(oob, panic_block, &[], ok_block, &[]);
		self.b.seal_block(panic_block);
		self.b.seal_block(ok_block);

		self.b.switch_to_block(panic_block);
		let func = self.import_fn(runtime::PANIC_OOB, &[self.int, self.int], None);
		self.b.ins().call(func, &[idx, len]);
		self.b.ins().trap(TrapCode::HEAP_OUT_OF_BOUNDS);

		self.b.switch_to_block(ok_block);
		let off = self.b.ins().imul_imm(idx, elem_size(elem));
		self.b.ins().iadd(data, off)
	}

	fn load_index(&mut self, data: Value, len: Value, elem: &Typ, idx: Value) -> Value {
		let addr = self.elem_addr(data, len, elem, idx);
		self.b
			.ins()
			.load(cl_type(elem, self.int), MemFlags::new(), addr, 0)
	}

	fn store_index(&mut self, data: Value, len: Value, elem: &Typ, idx: Value, val: Value) {
		let addr = self.elem_addr(data, len, elem, idx);
		self.b.ins().store(MemFlags::new(), val, addr, 0);
	}

	pub fn write_lit(&mut self, s: &str, stderr: bool) {
		let ptr = self.str_const(s);
		self.emit_frag(runtime::Tag::Raw, ptr, 0, false, stderr);
	}

	fn emit_frag(&mut self, tag: runtime::Tag, bits: Value, width: u16, quote: bool, stderr: bool) {
		let tag = self.b.ins().iconst(self.int, tag as i64);
		let width = self.b.ins().iconst(self.int, width as i64);
		let quote = self.b.ins().iconst(self.int, quote as i64);
		let stderr_v = self.b.ins().iconst(self.int, stderr as i64);
		let func = self.import_fn(
			runtime::WRITE,
			&[self.int, self.int, self.int, self.int, self.int],
			None,
		);
		self.b
			.ins()
			.call(func, &[tag, bits, width, quote, stderr_v]);
	}

	// Enum `Display`.
	fn enum_name_str(&mut self, name: &str, val: Value) -> Value {
		let tag = self.enum_tag(name, val);
		let variants = self.enums.get(name).cloned().unwrap_or_default();
		let mut ptr = self.str_const("");
		for v in &variants {
			let s = self.str_const(&v.name);
			let disc = self.b.ins().iconst(self.int, v.disc);
			let hit = self.b.ins().icmp(IntCC::Equal, tag, disc);
			ptr = self.b.ins().select(hit, s, ptr);
		}
		ptr
	}

	pub fn emit_print(&mut self, val: Value, typ: &Typ, quote: bool, stderr: bool) {
		match typ {
			Typ::Tuple(fields) => {
				self.write_lit("(", stderr);
				for (i, (name, ft)) in fields.iter().enumerate() {
					if i > 0 {
						self.write_lit(", ", stderr);
					}
					if let Some(name) = name {
						self.write_lit(&format!("{name}: "), stderr);
					}
					let cl = cl_type(ft, self.int);
					let fv = self.b.ins().load(cl, MemFlags::new(), val, (i * 8) as i32);
					self.emit_print(fv, ft, true, stderr);
				}
				self.write_lit(")", stderr);
			}

			// array length is only known at runtime, so emit a loop
			Typ::Array(elem) | Typ::FixedArray(elem, _) => {
				self.write_lit("[", stderr);
				let (data, len) = self.array_parts(val, typ);
				let i = self.b.declare_var(self.int);
				let zero = self.b.ins().iconst(self.int, 0);
				self.b.def_var(i, zero);

				let header = self.b.create_block();
				let body = self.b.create_block();
				let exit = self.b.create_block();
				self.b.ins().jump(header, &[]);

				self.b.switch_to_block(header);
				let iv = self.b.use_var(i);
				let more = self.b.ins().icmp(IntCC::SignedLessThan, iv, len);
				self.b.ins().brif(more, body, &[], exit, &[]);
				self.b.seal_block(body);
				self.b.seal_block(exit);

				self.b.switch_to_block(body);
				let iv = self.b.use_var(i);
				let stderr_v = self.b.ins().iconst(self.int, stderr as i64);
				let sep = self.import_fn(runtime::WRITE_SEP, &[self.int, self.int], None);
				self.b.ins().call(sep, &[iv, stderr_v]);
				let off = self.b.ins().imul_imm(iv, elem_size(elem));
				let addr = self.b.ins().iadd(data, off);
				let ev = self
					.b
					.ins()
					.load(cl_type(elem, self.int), MemFlags::new(), addr, 0);
				self.emit_print(ev, elem, true, stderr);
				let next = self.b.ins().iadd_imm(iv, 1);
				self.b.def_var(i, next);
				self.b.ins().jump(header, &[]);
				self.b.seal_block(header);

				self.b.switch_to_block(exit);
				self.write_lit("]", stderr);
			}

			Typ::Struct(sname, fields) => {
				let sname = sname.clone();
				let fields = fields.clone();
				self.write_lit(&format!("{sname}{{"), stderr);
				for (i, f) in fields.iter().enumerate() {
					if i > 0 {
						self.write_lit(", ", stderr);
					}
					self.write_lit(&format!("{}: ", f.name), stderr);
					let cl = cl_type(&f.typ, self.int);
					let fv = self.b.ins().load(cl, MemFlags::new(), val, (i * 8) as i32);
					self.emit_print(fv, &f.typ, true, stderr);
				}
				self.write_lit("}", stderr);
			}

			Typ::Atom => {
				self.emit_frag(runtime::Tag::Raw, val, 0, false, stderr);
			}

			Typ::Enum(name) => {
				let ptr = self.enum_name_str(name, val);
				self.emit_frag(runtime::Tag::Raw, ptr, 0, false, stderr);
			}

			Typ::Range => {
				let cl = cl_int_for_width(32);
				let start = self.b.ins().load(cl, MemFlags::new(), val, 0);
				let end = self.b.ins().load(cl, MemFlags::new(), val, 8);
				self.emit_print(start, &Typ::Int(32), false, stderr);
				self.write_lit("..", stderr);
				self.emit_print(end, &Typ::Int(32), false, stderr);
			}

			_ => {
				let tag = match typ {
					Typ::Bool => runtime::Tag::Bool,
					Typ::Int(_) | Typ::ISize => runtime::Tag::Int,
					Typ::UInt(_) | Typ::USize => runtime::Tag::UInt,
					Typ::Float(_) => runtime::Tag::Float,
					Typ::Str => runtime::Tag::Str,
					Typ::Atom
					| Typ::Tuple(_)
					| Typ::Array(_)
					| Typ::FixedArray(..)
					| Typ::Struct(..)
					| Typ::Enum(_)
					| Typ::Range => {
						unreachable!("handled above")
					}
				};
				// normalize to pointer-sized before passing to the runtime
				let (bits, float_width) = match typ {
					Typ::Float(16) => {
						let i16v = self.b.ins().bitcast(types::I16, MemFlags::new(), val);
						(self.b.ins().uextend(self.int, i16v), 16)
					}
					Typ::Float(32) => {
						let i32v = self.b.ins().bitcast(types::I32, MemFlags::new(), val);
						(self.b.ins().uextend(self.int, i32v), 32)
					}
					Typ::Float(64) => (self.b.ins().bitcast(self.int, MemFlags::new(), val), 64),
					Typ::Float(128) => {
						panic!("f128 printing not yet supported by the JIT backend")
					}
					Typ::Float(w) => panic!("unsupported float width f{w}"),
					Typ::Int(w) if *w < 64 => (self.b.ins().sextend(self.int, val), 0),
					Typ::UInt(w) if *w < 64 => (self.b.ins().uextend(self.int, val), 0),
					_ => (val, 0),
				};
				self.emit_frag(tag, bits, float_width, quote, stderr);
			}
		}
	}
}

// A destructured binding.
// `(name, type, offset)`
type Bind = (String, Typ, i32);

// Create `Bind`s from idents.
// `base` is the first offset, `stride` the step between fields.
fn field_binds<'a>(
	elems: impl Iterator<Item = (&'a Spanned<Expr>, &'a Typ)>,
	base: i32,
	stride: i32,
) -> Result<Vec<Bind>, Diagnostic> {
	elems
		.enumerate()
		.map(|(i, (e, t))| match &e.0 {
			Expr::Ident(n) => Ok((n.clone(), t.clone(), base + i as i32 * stride)),
			_ => Err(
				Diagnostic::new("patterns must bind names", e.1.into_range())
					.with_label("not a name"),
			),
		})
		.collect()
}

// A struct pattern's field bindings.
fn struct_pattern(
	fdefs: &[FieldDef],
	pname: &str,
	sname: &str,
	entries: &[(Option<String>, Spanned<Expr>)],
	span: Span,
) -> Result<Vec<Bind>, Diagnostic> {
	if pname != sname {
		let msg = format!("pattern is `{pname}` but subject is `{sname}`");
		return Err(Diagnostic::new(msg, span.into_range()).with_label("type mismatch"));
	}
	entries
		.iter()
		.map(|(fname, e)| {
			let Expr::Ident(local) = &e.0 else {
				return Err(
					Diagnostic::new("struct patterns must bind names", e.1.into_range())
						.with_label("not a name"),
				);
			};
			let field = fname.as_deref().unwrap_or(local);
			let idx = fdefs.iter().position(|f| f.name == field).ok_or_else(|| {
				Diagnostic::new(
					format!("struct `{sname}` has no field `{field}`"),
					e.1.into_range(),
				)
				.with_label("no such field")
			})?;
			Ok((local.clone(), fdefs[idx].typ.clone(), idx as i32 * 8))
		})
		.collect()
}

// The element type of an array.
fn array_elem(typ: &Typ) -> &Typ {
	match typ {
		Typ::Array(e) | Typ::FixedArray(e, _) => e,
		_ => unreachable!("not an array type"),
	}
}

fn uint_max(width: u16) -> i64 {
	if width >= 64 {
		u64::MAX as i64
	} else {
		((1u64 << width) - 1) as i64
	}
}

fn int_min(width: u16) -> i64 {
	if width >= 64 {
		i64::MIN
	} else {
		-(1i64 << (width - 1))
	}
}

fn int_max(width: u16) -> i64 {
	if width >= 64 {
		i64::MAX
	} else {
		(1i64 << (width - 1)) - 1
	}
}

fn unsigned_cc(icc: IntCC) -> IntCC {
	match icc {
		IntCC::SignedLessThan => IntCC::UnsignedLessThan,
		IntCC::SignedLessThanOrEqual => IntCC::UnsignedLessThanOrEqual,
		IntCC::SignedGreaterThan => IntCC::UnsignedGreaterThan,
		IntCC::SignedGreaterThanOrEqual => IntCC::UnsignedGreaterThanOrEqual,
		other => other,
	}
}
