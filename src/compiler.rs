use std::collections::HashMap;

use cranelift::codegen;
use cranelift::prelude::*;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{DataDescription, FuncId, Linkage, Module};

use crate::ast::{Expr, ForIter, Param, Pattern, Span, Spanned};
use crate::diagnostics::Diagnostic;
use crate::runtime;

// A top-level named function awaiting compilation.
type FnItem<'a> = (
	&'a str,
	&'a [Param],
	&'a Option<Spanned<String>>,
	&'a [Spanned<Expr>],
);

pub struct Compiler {
	builder_ctx: FunctionBuilderContext,
	ctx: codegen::Context,
	data_description: DataDescription,
	module: JITModule,
	// counter for unique string data labels across all functions
	string_idx: usize,
}

impl Default for Compiler {
	fn default() -> Self {
		let mut flag_builder = settings::builder();
		flag_builder.set("use_colocated_libcalls", "false").unwrap();
		flag_builder.set("is_pic", "false").unwrap();
		let isa = cranelift_native::builder()
			.unwrap_or_else(|e| panic!("unsupported host: {e}"))
			.finish(settings::Flags::new(flag_builder))
			.unwrap();
		let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
		builder.symbol(runtime::STR_CONCAT, runtime::str_concat as *const u8);
		builder.symbol(runtime::ALLOC, runtime::alloc as *const u8);
		builder.symbol(runtime::PRINT, runtime::print as *const u8);
		builder.symbol(runtime::WRITE, runtime::write as *const u8);
		builder.symbol(runtime::WRITE_SEP, runtime::write_sep as *const u8);
		builder.symbol(runtime::SLICE, runtime::slice as *const u8);
		builder.symbol(runtime::PANIC_OOB, runtime::panic_oob as *const u8);
		builder.symbol(runtime::ARRAY_RESERVE, runtime::array_reserve as *const u8);
		builder.symbol(runtime::ARRAY_EXTEND, runtime::array_extend as *const u8);
		builder.symbol(runtime::STR_EQ, runtime::str_eq as *const u8);
		builder.symbol(runtime::STR_CONTAINS, runtime::str_contains as *const u8);
		builder.symbol(runtime::ASSERT_FAIL, runtime::assert_fail as *const u8);

		let module = JITModule::new(builder);
		Self {
			builder_ctx: FunctionBuilderContext::new(),
			ctx: module.make_context(),
			data_description: DataDescription::new(),
			module,
			string_idx: 0,
		}
	}
}

impl Compiler {
	pub fn compile(&mut self, program: &[Spanned<Expr>]) -> Result<*const u8, Diagnostic> {
		let int = self.module.target_config().pointer_type();

		// split the top level into fn defs and loose statements
		let mut main_body: Option<&[Spanned<Expr>]> = None;
		let mut others: Vec<FnItem> = vec![];
		let mut loose: Vec<&Spanned<Expr>> = vec![];
		for item in program {
			match &item.0 {
				Expr::Fn { name, body, .. } if name == "main" => main_body = Some(body),
				Expr::Fn {
					name,
					params,
					ret,
					body,
				} => others.push((name.as_str(), params, ret, body)),
				_ => loose.push(item),
			}
		}

		// compile each named fn, recording its signature so the rest can call it
		let mut funcs: HashMap<String, FnSig> = HashMap::new();
		for &(name, params, ret, body) in &others {
			// resolve declared param and return types up front
			let params: Vec<(String, Typ)> = params
				.iter()
				.map(|p| Ok((p.name.clone(), typ_from_name(&p.typ, p.span)?)))
				.collect::<Result<_, Diagnostic>>()?;
			let ret = ret
				.as_ref()
				.map(|(typ, span)| Ok::<_, Diagnostic>((typ_from_name(typ, *span)?, *span)))
				.transpose()?;
			let stmts: Vec<&Spanned<Expr>> = body.iter().collect();
			let (id, ret) =
				self.compile_fn(int, &format!("oi_{name}"), &params, ret, &stmts, &funcs)?;
			let param_typs = params.iter().map(|(_, t)| t.clone()).collect();
			funcs.insert(
				name.to_string(),
				FnSig {
					id,
					params: param_typs,
					ret,
				},
			);
		}

		// `main` is the entrypoint if present
		// otherwise the loose statements run as if wrapped in an implicit `main`
		let entry: Vec<&Spanned<Expr>> = match main_body {
			Some(body) => {
				if let Some(first) = loose.first() {
					return Err(Diagnostic::new(
						"top-level statements are not allowed alongside `fn main`",
						first.1.into_range(),
					)
					.with_label("move this inside a function")
					.with_note(
						"`fn main` is the entrypoint, so loose statements have nowhere to run",
					));
				}
				body.iter().collect()
			}
			None => loose,
		};
		// the program prints whatever it returns
		let (entry_id, typ) = self.compile_fn(int, "oi_main", &[], None, &entry, &funcs)?;
		let id = self.compile_entry(int, entry_id, typ, &funcs);

		self.module
			.finalize_definitions()
			.expect("finalize definitions");
		Ok(self.module.get_finalized_function(id))
	}

	// Compile a fn body, which returns its final value to its caller.
	fn compile_fn(
		&mut self,
		int: types::Type,
		name: &str,
		params: &[(String, Typ)],
		ret: Option<(Typ, Span)>,
		stmts: &[&Spanned<Expr>],
		funcs: &HashMap<String, FnSig>,
	) -> Result<(FuncId, Typ), Diagnostic> {
		let typ = self.translate(int, params, ret, stmts, funcs)?;
		let id = self.finish_fn(name);
		Ok((id, typ))
	}

	// Run the entrypoint and print its return.
	fn compile_entry(
		&mut self,
		int: types::Type,
		entry: FuncId,
		typ: Typ,
		funcs: &HashMap<String, FnSig>,
	) -> FuncId {
		let mut b = FunctionBuilder::new(&mut self.ctx.func, &mut self.builder_ctx);
		let block = b.create_block();
		b.switch_to_block(block);
		b.seal_block(block);

		let mut trans = Translator {
			int,
			b,
			vars: HashMap::new(),
			module: &mut self.module,
			funcs,
			string_idx: &mut self.string_idx,
			ret: None,
			loops: vec![],
		};

		let callee = trans.module.declare_func_in_func(entry, trans.b.func);
		let call = trans.b.ins().call(callee, &[]);
		if let Some(val) = trans.b.inst_results(call).first().copied() {
			trans.emit_print(val, &typ, true);
		}
		trans.b.ins().return_(&[]);
		trans.b.finalize();

		self.finish_fn("__oi_main")
	}

	// Declare and define whatever is in the current ctx, then reset it.
	fn finish_fn(&mut self, name: &str) -> FuncId {
		let id = self
			.module
			.declare_function(name, Linkage::Local, &self.ctx.func.signature)
			.expect("declare function");
		self.module
			.define_function(id, &mut self.ctx)
			.expect("define function");
		self.module.clear_context(&mut self.ctx);
		id
	}

	fn translate(
		&mut self,
		int: types::Type,
		params: &[(String, Typ)],
		ret: Option<(Typ, Span)>,
		stmts: &[&Spanned<Expr>],
		funcs: &HashMap<String, FnSig>,
	) -> Result<Typ, Diagnostic> {
		let mut b = FunctionBuilder::new(&mut self.ctx.func, &mut self.builder_ctx);
		// declare the parameter types before the entry block claims them
		for (_, typ) in params {
			b.func
				.signature
				.params
				.push(AbiParam::new(cl_type(typ, int)));
		}
		let block = b.create_block();
		b.append_block_params_for_function_params(block);
		b.switch_to_block(block);
		b.seal_block(block);

		// a span to blame if the fall-through value mismatches a declared return type
		let decl_span = ret.as_ref().map(|(_, s)| *s);

		let mut trans = Translator {
			int,
			b,
			vars: HashMap::new(),
			module: &mut self.module,
			funcs,
			string_idx: &mut self.string_idx,
			ret,
			loops: vec![],
		};

		// bind each parameter to a variable holding its incoming block param
		let param_vals: Vec<Value> = trans.b.block_params(block).to_vec();
		for ((name, typ), val) in params.iter().zip(param_vals) {
			let cl = trans.b.func.dfg.value_type(val);
			let var = trans.b.declare_var(cl);
			trans.b.def_var(var, val);
			trans.vars.insert(
				name.clone(),
				Local {
					var,
					typ: typ.clone(),
					mutable: false,
				},
			);
		}

		// the fn returns its final value
		if let Some((val, typ)) = trans.block(stmts)? {
			let span = stmts
				.last()
				.map(|s| s.1)
				.or(decl_span)
				.unwrap_or((0..0).into());
			trans.emit_return(val, typ, span)?;
		}
		trans.b.finalize();

		// the return type, declared or inferred, is the value type the fn produces
		Ok(trans.ret.map(|(t, _)| t).unwrap_or(Typ::Int))
	}
}

#[derive(Clone, Copy, Debug)]
enum Op {
	Add,
	Sub,
	Mul,
	Div,
	Mod,
}

// An expression's Oi type.
// A tuple field is an optional name paired with its type.
// An array carries its element type. Its length is only known at runtime.
// TODO: `PartialEq` compares tuple field names too right now, but comparisons need to ignore them
#[derive(Clone, PartialEq, Debug)]
enum Typ {
	Int,
	Float,
	Bool,
	Str,
	Tuple(Vec<(Option<String>, Typ)>),
	Array(Box<Typ>),
}

// The cranelift type backing an Oi type.
// ints are i32, floats are f64, everything else is pointer-sized for now.
fn cl_type(typ: &Typ, int: types::Type) -> types::Type {
	match typ {
		Typ::Int => types::I32,
		Typ::Float => types::F64,
		_ => int,
	}
}

// Bytes per element in a packed array buffer.
fn elem_size(typ: &Typ) -> i64 {
	if *typ == Typ::Int { 4 } else { 8 }
}

// Resolve a declared type name to an Oi type.
fn typ_from_name(name: &str, span: Span) -> Result<Typ, Diagnostic> {
	Ok(match name {
		"int" => Typ::Int,
		"f64" | "float" => Typ::Float,
		"bool" => Typ::Bool,
		"string" | "str" => Typ::Str,
		_ => {
			return Err(
				Diagnostic::new(format!("unknown type `{name}`"), span.into_range())
					.with_label("not a known type"),
			);
		}
	})
}

// A compiled function's calling info.
#[derive(Clone)]
struct FnSig {
	id: FuncId,
	params: Vec<Typ>,
	ret: Typ,
}

// A local variable.
#[derive(Clone)]
struct Local {
	var: Variable,
	typ: Typ,
	mutable: bool,
}

// An in-progress loop's jump targets.
// `continue` jumps to `top` and `break` jumps to `exit`
struct LoopFrame {
	top: Block,
	exit: Option<Block>,
}

struct Translator<'a> {
	int: types::Type,
	b: FunctionBuilder<'a>,
	vars: HashMap<String, Local>,
	module: &'a mut JITModule,
	funcs: &'a HashMap<String, FnSig>,
	string_idx: &'a mut usize,
	// the fn's return type (locked in by the first return, and later returns must agree)
	ret: Option<(Typ, Span)>,
	loops: Vec<LoopFrame>,
}

impl<'a> Translator<'a> {
	// Evaluate statements in the current cranelift block.
	// The block's value is its final expression.
	// `None` means the block diverged before reaching the end.
	fn block(&mut self, stmts: &[&Spanned<Expr>]) -> Result<Option<(Value, Typ)>, Diagnostic> {
		let mut last = (self.b.ins().iconst(types::I32, 0), Typ::Int);
		for &stmt in stmts {
			match &stmt.0 {
				Expr::Bind {
					mutable,
					name,
					value,
				} => {
					let (val, typ) = self.expr(value)?;
					// `:=` always declares a fresh binding, shadowing any earlier one
					let cl = self.b.func.dfg.value_type(val);
					let var = self.b.declare_var(cl);
					self.b.def_var(var, val);
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
					// a binding keeps the type it was declared with
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
					self.b.def_var(local.var, val);
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
						// single-element append: grow if full, then write and increment len
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
						// array extend: delegate entirely to the runtime
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
						// a bare `return` yields the zero value of the return type
						None => {
							let typ = self.ret.as_ref().map_or(Typ::Int, |(t, _)| t.clone());
							(self.zero(&typ), typ)
						}
					};
					self.emit_return(val, typ, stmt.1)?;
					return Ok(None);
				}

				// an `if` statement may diverge (every branch returns)
				Expr::If { cond, then, els } => {
					match self.conditional(cond, then, els.as_deref(), stmt.1)? {
						Some((v, t)) => last = (v, t),
						None => return Ok(None),
					}
				}

				Expr::Loop { cond, body } => match self.loop_expr(cond.as_deref(), body)? {
					Some((v, t)) => last = (v, t),
					None => return Ok(None),
				},

				// a for-loop is finite, so it always falls through
				// TODO: revisit this after adding the Iterator trait
				Expr::For { pat, iter, body } => last = self.for_loop(pat, iter, body, stmt.1)?,

				// `break`/`continue` end the current block, so the rest of it is unreachable
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
					// the first `break` creates the loop's exit block
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

				_ => last = self.expr(stmt)?,
			}
		}
		Ok(Some(last))
	}

	// Emit a return of `val`.
	// The first return fixes the fn's type, and later returns must agree.
	fn emit_return(&mut self, val: Value, typ: Typ, span: Span) -> Result<(), Diagnostic> {
		if let Some((declared, _)) = &self.ret
			&& &typ != declared
		{
			return Err(Diagnostic::new(
				format!("expected {declared:?} return value, got {typ:?}"),
				span.into_range(),
			)
			.with_label("wrong return type"));
		}
		// the cranelift signature takes its return type from the first return
		if self.b.func.signature.returns.is_empty() {
			self.b
				.func
				.signature
				.returns
				.push(AbiParam::new(cl_type(&typ, self.int)));
		}
		self.b.ins().return_(&[val]);
		// an undeclared fn infers its return type from this first return
		if self.ret.is_none() {
			self.ret = Some((typ, span));
		}
		Ok(())
	}

	// An `if`/`else`, lowered to branch&merge, yielding the value of the chosen branch.
	// A branch may diverge, in which case it doesn't reach the merge.
	// If every branch diverges, the whole `if` diverges (`None`).
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

		// the result var and merge block are created once a branch falls through
		// a branch that diverges contributes neither
		let mut result: Option<Variable> = None;
		let mut result_typ: Option<Typ> = None;
		let mut merge: Option<Block> = None;

		// branch-local bindings must not leak into the enclosing scope
		let saved = self.vars.clone();

		// then branch
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

		// else branch
		self.b.switch_to_block(else_block);
		let else_flow = match els {
			Some(els) => {
				let refs: Vec<&Spanned<Expr>> = els.iter().collect();
				self.block(&refs)?
			}
			// no `else` yields the zero value of the if expression's type
			None => {
				let t = result_typ.clone().unwrap_or(Typ::Int);
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
			// both branches diverged, and control never reaches a merge
			None => Ok(None),
		}
	}

	// Lower a `loop`.
	fn loop_expr(
		&mut self,
		cond: Option<&Spanned<Expr>>,
		body: &[Spanned<Expr>],
	) -> Result<Option<(Value, Typ)>, Diagnostic> {
		let top = self.b.create_block();
		self.b.ins().jump(top, &[]);
		self.b.switch_to_block(top);

		// a conditional loop branches at the top into the body or out to `exit`
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
		// bindings made inside the loop must not leak past it
		let saved = self.vars.clone();
		let refs: Vec<&Spanned<Expr>> = body.iter().collect();
		let flow = self.block(&refs)?;
		self.vars = saved;
		let frame = self.loops.pop().expect("loop frame");

		// loop back to the top unless the body diverged
		if flow.is_some() {
			self.b.ins().jump(top, &[]);
		}
		self.b.seal_block(top);

		match frame.exit {
			// the loop has an exit (a false condition or a `break`), so it falls through to it
			Some(exit) => {
				self.b.switch_to_block(exit);
				self.b.seal_block(exit);
				Ok(Some((self.b.ins().iconst(types::I32, 0), Typ::Int)))
			}
			// an infinite loop with no `break` never falls through
			None => Ok(None),
		}
	}

	fn for_loop(
		&mut self,
		pat: &Pattern,
		iter: &ForIter,
		body: &[Spanned<Expr>],
		span: Span,
	) -> Result<(Value, Typ), Diagnostic> {
		// (data ptr, elem type) for array iteration
		// `None` for a range
		let (counter, limit, arr_src): (_, _, Option<(Value, Typ)>) = match iter {
			ForIter::Range(s, e) => {
				let start = self.int_value(s, "range start")?;
				let end = self.int_value(e, "range end")?;
				let v = self.b.declare_var(types::I32);
				self.b.def_var(v, start);
				(v, end, None)
			}
			ForIter::Iter(e) => {
				let (arr, typ) = self.expr(e)?;
				let Typ::Array(elem) = typ else {
					return Err(Diagnostic::new(
						format!("cannot iterate over {typ:?}"),
						e.1.into_range(),
					)
					.with_label("not iterable"));
				};
				let zero = self.b.ins().iconst(self.int, 0);
				let len = self.array_len(arr);
				let data = self.array_data(arr);
				let v = self.b.declare_var(self.int);
				self.b.def_var(v, zero);
				(v, len, Some((data, *elem)))
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
			None => (iv, Typ::Int),
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
		Ok((self.b.ins().iconst(types::I32, 0), Typ::Int))
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

	// Lower an expresson.
	fn expr(&mut self, expr: &Spanned<Expr>) -> Result<(Value, Typ), Diagnostic> {
		match &expr.0 {
			Expr::Int(n) => Ok((self.b.ins().iconst(types::I32, *n as i64), Typ::Int)),
			Expr::Bool(v) => Ok((self.b.ins().iconst(self.int, *v as i64), Typ::Bool)),
			Expr::Float(x) => Ok((self.b.ins().f64const(*x), Typ::Float)),
			Expr::String(s) => Ok((self.str_const(s), Typ::Str)),

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
					Typ::Int => self.b.ins().ineg(v),
					Typ::Float => self.b.ins().fneg(v),
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

			// TODO: migrate to `assert!` macro once we, you know, have macros
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
				// evaluate each argument, checking it against the declared parameter type
				let mut vals = Vec::with_capacity(args.len());
				for (arg, expected) in args.iter().zip(&sig.params) {
					let (val, typ) = self.expr(arg)?;
					if &typ != expected {
						return Err(Diagnostic::new(
							format!("expected {expected:?} argument, got {typ:?}"),
							arg.1.into_range(),
						)
						.with_label("wrong argument type"));
					}
					vals.push(val);
				}
				let func = self.module.declare_func_in_func(sig.id, self.b.func);
				let call = self.b.ins().call(func, &vals);
				Ok((self.b.inst_results(call)[0], sig.ret))
			}

			// a tuple is a heap block of pointer-sized slots, one per field
			Expr::Tuple(elems) => {
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
						return Ok((len, Typ::Int));
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
				// an integer field is an index, anything else a field name
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
				// every element must share one type
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

				// fill a fresh element buffer, then wrap it in a handle
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

			// a slice is a view of an array that shares the parent's elements and memory space
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

			// an `if` nested inside an expression must yield a value on every branch
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

				let arr = rhs_val;
				let arr_typ = rhs_typ;
				let elem = match arr_typ {
					Typ::Array(ref e) => (**e).clone(),
					_ => {
						return Err(Diagnostic::new(
							format!("right side of `in` must be an array or Str, got {arr_typ:?}"),
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

				let len = self.array_len(arr);
				let data = self.array_data(arr);

				// result: false until a match is found
				let found = self.b.declare_var(self.int);
				let zero = self.b.ins().iconst(self.int, 0);
				self.b.def_var(found, zero);
				let i = self.b.declare_var(self.int);
				self.b.def_var(i, zero);

				let header = self.b.create_block();
				let body = self.b.create_block();
				let found_block = self.b.create_block();
				let continue_block = self.b.create_block();
				let exit = self.b.create_block();

				self.b.ins().jump(header, &[]);

				// header: loop while i < len
				self.b.switch_to_block(header);
				let iv = self.b.use_var(i);
				let more = self.b.ins().icmp(IntCC::SignedLessThan, iv, len);
				self.b.ins().brif(more, body, &[], exit, &[]);
				self.b.seal_block(body);

				// body: compare element, then branch to found_block or continue_block
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

				// found: set result = true, jump to exit
				self.b.switch_to_block(found_block);
				let one = self.b.ins().iconst(self.int, 1);
				self.b.def_var(found, one);
				self.b.ins().jump(exit, &[]);
				self.b.seal_block(exit);

				// continue: advance i and loop back
				self.b.switch_to_block(continue_block);
				let iv = self.b.use_var(i);
				let next = self.b.ins().iadd_imm(iv, 1);
				self.b.def_var(i, next);
				self.b.ins().jump(header, &[]);
				self.b.seal_block(header);

				self.b.switch_to_block(exit);
				Ok((self.b.use_var(found), Typ::Bool))
			}

			Expr::Bind { .. } => unreachable!("bind in expression position"),
			Expr::Assign { .. } => unreachable!("assign in expression position"),
			Expr::IndexAssign { .. } => unreachable!("index assign in expression position"),
			Expr::Fn { .. } => unreachable!("fn definition in expression position"),
			Expr::Return(..) => unreachable!("return in expression position"),
			Expr::Break | Expr::Continue => {
				unreachable!("break/continue in expression position")
			}
			Expr::Append { .. } => unreachable!("append in expression position"),
		}
	}

	// Emit a 0-terminated string constant and return a pointer to it.
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

	// Compare two values of the same type.
	fn emit_eq(&mut self, a: Value, b: Value, typ: &Typ) -> Value {
		match typ {
			Typ::Float => self.b.ins().fcmp(FloatCC::Equal, a, b),
			Typ::Str => {
				let func = self.import_fn(runtime::STR_EQ, &[self.int, self.int], Some(self.int));
				let call = self.b.ins().call(func, &[a, b]);
				self.b.inst_results(call)[0]
			}
			_ => self.b.ins().icmp(IntCC::Equal, a, b),
		}
	}

	// The zero value for an Oi type.
	fn zero(&mut self, typ: &Typ) -> Value {
		match typ {
			Typ::Float => self.b.ins().f64const(0.0),
			Typ::Str => self.str_const(""),
			Typ::Int => self.b.ins().iconst(types::I32, 0),
			Typ::Bool => self.b.ins().iconst(self.int, 0),
			// unreachable until composite return types exist, returns are scalar names for now
			Typ::Tuple(_) => unreachable!("tuple return types aren't supported yet"),
			Typ::Array(_) => unreachable!("array return types aren't supported yet"),
		}
	}

	// Add binary op instruction.
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

		// no int/float mixing for now
		// NOTE: I might go with V-style promotion eventually.
		let float = match (&lt, &rt) {
			(Typ::Int, Typ::Int) => false,
			(Typ::Float, Typ::Float) => true,
			_ => {
				return Err(Diagnostic::new(
					format!("cannot {op:?} {lt:?} and {rt:?}"),
					span.into_range(),
				)
				.with_label("operands have mismatched types"));
			}
		};
		if let (Op::Mod, true) = (op, float) {
			// TODO: apparently cranelift has no float remainder
			return Err(Diagnostic::new(
				"`%` is not yet supported on floats".to_string(),
				span.into_range(),
			)
			.with_label("only integer operands"));
		}
		let b = self.b.ins();
		let out = match (op, float) {
			(Op::Add, true) => b.fadd(lv, rv),
			(Op::Add, false) => b.iadd(lv, rv),
			(Op::Sub, true) => b.fsub(lv, rv),
			(Op::Sub, false) => b.isub(lv, rv),
			(Op::Mul, true) => b.fmul(lv, rv),
			(Op::Mul, false) => b.imul(lv, rv),
			(Op::Div, true) => b.fdiv(lv, rv),
			(Op::Div, false) => b.sdiv(lv, rv),
			(Op::Mod, false) => b.srem(lv, rv),
			(Op::Mod, true) => unreachable!("float `%` rejected above"),
		};
		Ok((out, if float { Typ::Float } else { Typ::Int }))
	}

	// Add a comparison instruction, yielding a Bool.
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

		let raw = match (&lt, &rt) {
			(Typ::Int, Typ::Int) | (Typ::Bool, Typ::Bool) => self.b.ins().icmp(icc, lv, rv),
			(Typ::Float, Typ::Float) => self.b.ins().fcmp(fcc, lv, rv),
			(Typ::Str, Typ::Str) if icc == IntCC::Equal || icc == IntCC::NotEqual => {
				let eq = self.emit_eq(lv, rv, &Typ::Str);
				// emit_eq returns 1 for equal, so we invert for Ne
				if icc == IntCC::NotEqual {
					self.b.ins().icmp_imm(IntCC::Equal, eq, 0)
				} else {
					// eq is already i64 from str_eq, but needs to be the raw bool width
					// wrap it in an icmp so the uextend below works the same in every situation
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

	// Lower boolean operators, yielding a Bool.
	// Short-circuits (the right side is only evaluated when the left doesn't already decide the result).
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

		// the result defaults to the short-circuit value: `false` for `&&`, `true` for `||`
		let result = self.b.declare_var(self.int);
		let short = self.b.ins().iconst(self.int, if and { 0 } else { 1 });
		self.b.def_var(result, short);

		// `&&` evaluates the right side when the left is true, `||` when it's false
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

	// Declare an imported runtime fn in the current function and return its ref.
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

	// Call the runtime string concat.
	fn call_concat(&mut self, a: Value, b: Value) -> Value {
		let func = self.import_fn(runtime::STR_CONCAT, &[self.int, self.int], Some(self.int));
		let call = self.b.ins().call(func, &[a, b]);
		self.b.inst_results(call)[0]
	}

	// Allocate `n` pointer-sized slots (used for handles and tuple fields).
	fn call_alloc(&mut self, n: usize) -> Value {
		self.call_alloc_bytes((n * 8) as i64)
	}

	// Allocate exactly `bytes` bytes and return the pointer.
	fn call_alloc_bytes(&mut self, bytes: i64) -> Value {
		let func = self.import_fn(runtime::ALLOC, &[self.int], Some(self.int));
		let size = self.b.ins().iconst(self.int, bytes);
		let call = self.b.ins().call(func, &[size]);
		self.b.inst_results(call)[0]
	}

	// An array value is a pointer to a 2-word handle: `{ data @ 0, len @ 8 }`.
	// `data` points to a separate buffer of pointer-sized element slots, which a slice shares with its parent.
	// Slicing copies only the handle, not the elements.
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

	// Evaluate an array operand, returning its pointer and element type.
	// `what` names the operation for the error (ex: "index", "slice").
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

	// Evaluate an expression that must be an Int (an index or slice bound).
	fn int_value(&mut self, e: &Spanned<Expr>, what: &str) -> Result<Value, Diagnostic> {
		let (v, t) = self.expr(e)?;
		if t != Typ::Int {
			return Err(Diagnostic::new(
				format!("{what} must be Int, got {t:?}"),
				e.1.into_range(),
			)
			.with_label("not an Int"));
		}
		Ok(v)
	}

	// Bounds-check `idx` (pointer-sized) against the array's length, then load that element.
	fn load_index(&mut self, header: Value, elem: &Typ, idx: Value) -> Value {
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
		let addr = self.b.ins().iadd(data, off);
		self.b
			.ins()
			.load(cl_type(elem, self.int), MemFlags::new(), addr, 0)
	}

	// Bounds-check `idx` (pointer-sized), then store `val` at that element position.
	fn store_index(&mut self, header: Value, elem: &Typ, idx: Value, val: Value) {
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
		let addr = self.b.ins().iadd(data, off);
		self.b.ins().store(MemFlags::new(), val, addr, 0);
	}

	// Write a literal text fragment (delimiter, field) with no newline.
	fn write_lit(&mut self, s: &str) {
		let ptr = self.str_const(s);
		self.emit_value(runtime::WRITE, runtime::Tag::Raw, ptr);
	}

	// Call a runtime printer with a type tag.
	fn emit_value(&mut self, func_name: &str, tag: runtime::Tag, bits: Value) {
		let tag = self.b.ins().iconst(self.int, tag as i64);
		let func = self.import_fn(func_name, &[self.int, self.int], None);
		self.b.ins().call(func, &[tag, bits]);
	}

	// Print value.
	// Top level adds a newline.
	fn emit_print(&mut self, val: Value, typ: &Typ, top: bool) {
		if let Typ::Tuple(fields) = typ {
			self.write_lit("(");
			for (i, (name, ft)) in fields.iter().enumerate() {
				if i > 0 {
					self.write_lit(", ");
				}
				if let Some(name) = name {
					self.write_lit(&format!("{name}: "));
				}
				let cl = cl_type(ft, self.int);
				let fv = self.b.ins().load(cl, MemFlags::new(), val, (i * 8) as i32);
				self.emit_print(fv, ft, false);
			}
			self.write_lit(")");
			if top {
				self.write_lit("\n");
			}
			return;
		}

		// an array's length is only known at runtime, so walk it with an emitted loop
		if let Typ::Array(elem) = typ {
			self.write_lit("[");
			let len = self.array_len(val);
			let data = self.array_data(val);
			let i = self.b.declare_var(self.int);
			let zero = self.b.ins().iconst(self.int, 0);
			self.b.def_var(i, zero);

			let header = self.b.create_block();
			let body = self.b.create_block();
			let exit = self.b.create_block();
			self.b.ins().jump(header, &[]);

			// header
			// loop while `i < len`
			self.b.switch_to_block(header);
			let iv = self.b.use_var(i);
			let more = self.b.ins().icmp(IntCC::SignedLessThan, iv, len);
			self.b.ins().brif(more, body, &[], exit, &[]);
			self.b.seal_block(body);
			self.b.seal_block(exit);

			// body
			// a runtime helper writes the ", " before all but the first element,
			// then element `i` is loaded from byte offset `i * 8` in the data buffer
			self.b.switch_to_block(body);
			let iv = self.b.use_var(i);
			let sep = self.import_fn(runtime::WRITE_SEP, &[self.int], None);
			self.b.ins().call(sep, &[iv]);
			let off = self.b.ins().imul_imm(iv, elem_size(elem));
			let addr = self.b.ins().iadd(data, off);
			let ev = self
				.b
				.ins()
				.load(cl_type(elem, self.int), MemFlags::new(), addr, 0);
			self.emit_print(ev, elem, false);
			let next = self.b.ins().iadd_imm(iv, 1);
			self.b.def_var(i, next);
			self.b.ins().jump(header, &[]);
			self.b.seal_block(header);

			self.b.switch_to_block(exit);
			self.write_lit("]");
			if top {
				self.write_lit("\n");
			}
			return;
		}

		let tag = match typ {
			Typ::Bool => runtime::Tag::Bool,
			Typ::Int => runtime::Tag::Int,
			Typ::Float => runtime::Tag::Float,
			Typ::Str => runtime::Tag::Str,
			Typ::Tuple(_) => unreachable!("tuple handled above"),
			Typ::Array(_) => unreachable!("array handled above"),
		};
		// normalize to pointer-sized before passing to the runtime
		let bits = match typ {
			Typ::Float => self.b.ins().bitcast(self.int, MemFlags::new(), val),
			Typ::Int => self.b.ins().sextend(self.int, val),
			_ => val,
		};
		let func = if top { runtime::PRINT } else { runtime::WRITE };
		self.emit_value(func, tag, bits);
	}
}
