use std::collections::HashMap;

use cranelift::codegen;
use cranelift::codegen::ir::immediates::{Ieee16, Ieee128};
use cranelift::codegen::ir::{StackSlotData, StackSlotKind};
use cranelift::prelude::*;
use cranelift_jit::JITModule;
use cranelift_module::{DataDescription, Linkage, Module};

use super::{FieldDef, FnSig, Local, LoopFrame, Op, Typ, cl_int_for_width, cl_type, elem_size};
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
	pub string_idx: &'a mut usize,
	pub atoms: &'a mut HashMap<String, ()>,
	pub ret: Option<(Typ, Span)>,
	pub loops: Vec<LoopFrame>,
}

impl<'a> Translator<'a> {
	// Evaluate a block of statements, returning the final value.
	// Returns None if the block diverged (every path returned/broke).
	pub fn block(&mut self, stmts: &[&Spanned<Expr>]) -> Result<Option<(Value, Typ)>, Diagnostic> {
		let mut last = (self.b.ins().iconst(self.int, 0), Typ::Tuple(vec![]));
		for &stmt in stmts {
			match &stmt.0 {
				Expr::Bind {
					mutable,
					name,
					value,
				} => {
					let (val, typ) = self.expr(value)?;
					let (final_val, cl) = if let Typ::Struct(_, ref fields) = typ {
						let dst = self.struct_copy(val, fields);
						(dst, self.int)
					} else {
						let cl = self.b.func.dfg.value_type(val);
						(val, cl)
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
					let (val, typ) = self.expr(value)?;
					let local = self.vars.get(name).cloned().ok_or_else(|| {
						Diagnostic::new(
							format!("cannot assign to undefined variable `{name}`"),
							stmt.1.into_range(),
						)
						.with_label("not found in scope")
						.with_note(format!("declare it first with `{name} := ...`"))
					})?;
					if !local.mutable {
						return Err(Diagnostic::new(
							format!("cannot assign to immutable `{name}`"),
							stmt.1.into_range(),
						)
						.with_label("declared without `mut`")
						.with_note(format!("use `mut {name} := ...` to allow assignment")));
					}
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
					let local = self.vars.get(name).cloned().ok_or_else(|| {
						Diagnostic::new(
							format!("cannot assign to undefined variable `{name}`"),
							stmt.1.into_range(),
						)
						.with_label("not found in scope")
						.with_note(format!("declare it first with `{name} := ...`"))
					})?;
					if !local.mutable {
						return Err(Diagnostic::new(
							format!("cannot assign to element of immutable `{name}`"),
							stmt.1.into_range(),
						)
						.with_label("declared without `mut`")
						.with_note(format!("use `mut {name} := ...` to allow assignment")));
					}
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
					self.store_index(ptr, &elem, idx, val);
				}

				Expr::Append { name, value } => {
					let local = self.vars.get(name).cloned().ok_or_else(|| {
						Diagnostic::new(
							format!("cannot append to undefined variable `{name}`"),
							stmt.1.into_range(),
						)
						.with_label("not found in scope")
					})?;
					if !local.mutable {
						return Err(Diagnostic::new(
							format!("cannot append to immutable `{name}`"),
							stmt.1.into_range(),
						)
						.with_label("declared without `mut`")
						.with_note(format!("use `mut {name} := ...` to allow append")));
					}
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
					let local = self.vars.get(name).cloned().ok_or_else(|| {
						Diagnostic::new(
							format!("cannot assign field of undefined variable `{name}`"),
							stmt.1.into_range(),
						)
						.with_label("not found in scope")
					})?;
					if !local.mutable {
						return Err(Diagnostic::new(
							format!("cannot assign field of immutable `{name}`"),
							stmt.1.into_range(),
						)
						.with_label("declared without `mut`")
						.with_note(format!("use `mut {name} := ...` to allow field assignment")));
					}
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
		// structs live on the callee's stack, so copy to heap before returning
		let final_val = if let Typ::Struct(_, ref fields) = typ {
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
		} else {
			val
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

			for (j, pat) in arm.patterns.iter().enumerate() {
				let sv = self.b.use_var(sv_var);
				let (pv, pt) = self.expr(pat)?;
				if pt != st {
					return Err(Diagnostic::new(
						format!("match pattern ({pt:?}) does not match subject ({st:?})"),
						pat.1.into_range(),
					)
					.with_label("type mismatch"));
				}
				let eq = self.emit_eq(sv, pv, &st);
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

		Ok(if result.is_some() {
			self.b.switch_to_block(merge);
			self.b.seal_block(merge);
			let (var, typ) = result.unwrap();
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
					Typ::Int(w) => {
						let src_cl = cl_type(&Typ::Int(*w), self.int);
						let v64 = if src_cl == types::I64 {
							val
						} else {
							self.b.ins().sextend(types::I64, val)
						};
						let lo = self.b.ins().iconst(types::I64, int_min(target));
						let hi = self.b.ins().iconst(types::I64, int_max(target));
						let lt = self.b.ins().icmp(IntCC::SignedLessThan, v64, lo);
						let v = self.b.ins().select(lt, lo, v64);
						let gt = self.b.ins().icmp(IntCC::SignedGreaterThan, v, hi);
						let v = self.b.ins().select(gt, hi, v);
						if target_cl == types::I64 {
							v
						} else {
							self.b.ins().ireduce(target_cl, v)
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
					Typ::UInt(w) => {
						// widen/narrow
						// sign-extend to i64, clamp to [0, target_max], ireduce
						let src_cl = cl_type(&Typ::UInt(*w), self.int);
						let v64 = if src_cl == types::I64 {
							val
						} else {
							self.b.ins().uextend(types::I64, val)
						};
						let max_v = self.b.ins().iconst(types::I64, uint_max(target));
						let gt = self.b.ins().icmp(IntCC::UnsignedGreaterThan, v64, max_v);
						let v = self.b.ins().select(gt, max_v, v64);
						if target_cl == types::I64 {
							v
						} else {
							self.b.ins().ireduce(target_cl, v)
						}
					}
					Typ::Int(w) => {
						let src_cl = cl_type(&Typ::Int(*w), self.int);
						let v64 = if src_cl == types::I64 {
							val
						} else {
							self.b.ins().sextend(types::I64, val)
						};
						let zero = self.b.ins().iconst(types::I64, 0);
						let max_v = self.b.ins().iconst(types::I64, uint_max(target));
						let lt = self.b.ins().icmp(IntCC::SignedLessThan, v64, zero);
						let v = self.b.ins().select(lt, zero, v64);
						let gt = self.b.ins().icmp(IntCC::UnsignedGreaterThan, v, max_v);
						let v = self.b.ins().select(gt, max_v, v);
						if target_cl == types::I64 {
							v
						} else {
							self.b.ins().ireduce(target_cl, v)
						}
					}
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
				if args.len() != sig.params.len() {
					return Err(Diagnostic::new(
						format!(
							"`{name}` expects {} argument(s), got {}",
							sig.params.len(),
							args.len()
						),
						expr.1.into_range(),
					)
					.with_label("wrong number of arguments"));
				}
				let mut vals = Vec::with_capacity(args.len());
				for (arg, expected) in args.iter().zip(&sig.params) {
					let (val, typ) = self.expr(arg)?;
					if &typ != expected {
						return Err(Diagnostic::new(
							format!("expected {expected} argument, got {typ}"),
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
				let (ptr, typ) = self.expr(tuple)?;

				// arrays expose `.len` and numeric `.n` (sugar for `arr[n]`)
				if let Typ::Array(elem) = &typ {
					let elem = (**elem).clone();
					if field == "len" {
						let raw = self.array_len(ptr);
						let len = self.b.ins().ireduce(types::I32, raw);
						return Ok((len, Typ::Int(32)));
					}
					return match field.parse::<i64>() {
						Ok(n) => {
							let idx = self.b.ins().iconst(self.int, n);
							Ok((self.load_index(ptr, &elem, idx), elem))
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

			Expr::Index { collection, index } => {
				let (ptr, elem) = self.array_operand(collection, "index")?;
				let idx = self.int_value(index, "array index")?;
				let idx = self.b.ins().sextend(self.int, idx);
				Ok((self.load_index(ptr, &elem, idx), elem))
			}

			Expr::Slice {
				collection,
				start,
				end,
			} => {
				let (ptr, elem) = self.array_operand(collection, "slice")?;
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
						if &vtyp != &f.typ {
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
							let (val, vtyp) = self.expr(value)?;
							let expected = &struct_fields[i].typ;
							if &vtyp != expected {
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
							let (val, vtyp) = self.expr(value)?;
							let expected = &struct_fields[idx].typ;
							if &vtyp != expected {
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
			Typ::Range => {
				let ptr = self.call_alloc(2);
				let z = self.b.ins().iconst(self.int, 0);
				self.b.ins().store(MemFlags::new(), z, ptr, 0);
				self.b.ins().store(MemFlags::new(), z, ptr, 8);
				ptr
			}
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
		let (lv, lt) = self.expr(l)?;
		let (rv, rt) = self.expr(r)?;

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

	fn array_operand(
		&mut self,
		collection: &Spanned<Expr>,
		what: &str,
	) -> Result<(Value, Typ), Diagnostic> {
		let (ptr, typ) = self.expr(collection)?;
		match typ {
			Typ::Array(elem) => Ok((ptr, *elem)),
			_ => Err(
				Diagnostic::new(format!("cannot {what} {typ:?}"), collection.1.into_range())
					.with_label("not an array"),
			),
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
	fn elem_addr(&mut self, header: Value, elem: &Typ, idx: Value) -> Value {
		let len = self.array_len(header);
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
		let data = self.array_data(header);
		let off = self.b.ins().imul_imm(idx, elem_size(elem));
		self.b.ins().iadd(data, off)
	}

	fn load_index(&mut self, header: Value, elem: &Typ, idx: Value) -> Value {
		let addr = self.elem_addr(header, elem, idx);
		self.b
			.ins()
			.load(cl_type(elem, self.int), MemFlags::new(), addr, 0)
	}

	fn store_index(&mut self, header: Value, elem: &Typ, idx: Value, val: Value) {
		let addr = self.elem_addr(header, elem, idx);
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
			Typ::Array(elem) => {
				self.write_lit("[", stderr);
				let len = self.array_len(val);
				let data = self.array_data(val);
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
					Typ::Atom | Typ::Tuple(_) | Typ::Array(_) | Typ::Struct(..) | Typ::Range => {
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
