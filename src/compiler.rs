use std::collections::HashMap;

use cranelift::codegen;
use cranelift::prelude::*;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{DataDescription, FuncId, Linkage, Module};

use crate::ast::{Expr, Param, Span, Spanned};
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
		builder.symbol(runtime::PRINT_BOOL, runtime::print_bool as *const u8);
		builder.symbol(runtime::PRINT_INT, runtime::print_int as *const u8);
		builder.symbol(runtime::PRINT_FLOAT, runtime::print_float as *const u8);
		builder.symbol(runtime::PRINT_STR, runtime::print_str as *const u8);
		builder.symbol(runtime::STR_CONCAT, runtime::str_concat as *const u8);

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
			let param_typs = params.iter().map(|(_, t)| *t).collect();
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
		};

		let callee = trans.module.declare_func_in_func(entry, trans.b.func);
		let call = trans.b.ins().call(callee, &[]);
		let val = trans.b.inst_results(call)[0];
		trans.emit_print(val, typ);
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
				.push(AbiParam::new(cl_type(*typ, int)));
		}
		let block = b.create_block();
		b.append_block_params_for_function_params(block);
		b.switch_to_block(block);
		b.seal_block(block);

		let mut trans = Translator {
			int,
			b,
			vars: HashMap::new(),
			module: &mut self.module,
			funcs,
			string_idx: &mut self.string_idx,
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
					typ: *typ,
					mutable: false,
				},
			);
		}

		let mut last = (trans.b.ins().iconst(int, 0), Typ::Int);
		let mut last_span: Option<Span> = None;
		for &stmt in stmts {
			match &stmt.0 {
				Expr::Bind {
					mutable,
					name,
					value,
				} => {
					let (val, typ) = trans.expr(value)?;
					// `:=` always declares a fresh binding, shadowing any earlier one
					let cl = trans.b.func.dfg.value_type(val);
					let var = trans.b.declare_var(cl);
					trans.b.def_var(var, val);
					trans.vars.insert(
						name.clone(),
						Local {
							var,
							typ,
							mutable: *mutable,
						},
					);
				}

				Expr::Assign { name, value } => {
					let (val, typ) = trans.expr(value)?;
					let local = trans.vars.get(name).copied().ok_or_else(|| {
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
					trans.b.def_var(local.var, val);
				}

				Expr::Return(value) => {
					last = match value {
						Some(e) => trans.expr(e)?,
						// a bare `return` yields the zero value of the return type
						None => {
							let typ = ret.map_or(Typ::Int, |(t, _)| t);
							(trans.zero(typ), typ)
						}
					};
					last_span = Some(stmt.1);
					break;
				}

				_ => {
					last = trans.expr(stmt)?;
					last_span = Some(stmt.1);
				}
			}
		}

		// the fn returns its final value
		// if a return type was declared, it must match
		let (val, typ) = last;
		if let Some((declared, decl_span)) = ret
			&& typ != declared
		{
			return Err(Diagnostic::new(
				format!("expected {declared:?} return value, got {typ:?}"),
				last_span.unwrap_or(decl_span).into_range(),
			)
			.with_label("wrong return type"));
		}
		trans
			.b
			.func
			.signature
			.returns
			.push(AbiParam::new(cl_type(typ, int)));
		trans.b.ins().return_(&[val]);
		trans.b.finalize();
		Ok(typ)
	}
}

#[derive(Clone, Copy, Debug)]
enum Op {
	Add,
	Sub,
	Mul,
	Div,
}

// An expression's Oi type.
// int, bool, and str are all i64 to cranelift, so this lets us distinguish.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Typ {
	Int,
	Float,
	Bool,
	Str,
}

// The cranelift type backing an Oi type.
// Floats are f64, everything else is pointer-sized.
fn cl_type(typ: Typ, int: types::Type) -> types::Type {
	if typ == Typ::Float { types::F64 } else { int }
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
#[derive(Clone, Copy)]
struct Local {
	var: Variable,
	typ: Typ,
	mutable: bool,
}

struct Translator<'a> {
	int: types::Type,
	b: FunctionBuilder<'a>,
	vars: HashMap<String, Local>,
	module: &'a mut JITModule,
	funcs: &'a HashMap<String, FnSig>,
	string_idx: &'a mut usize,
}

impl<'a> Translator<'a> {
	fn expr(&mut self, expr: &Spanned<Expr>) -> Result<(Value, Typ), Diagnostic> {
		match &expr.0 {
			Expr::Int(n) => Ok((self.b.ins().iconst(self.int, *n as i64), Typ::Int)),
			Expr::Bool(v) => Ok((self.b.ins().iconst(self.int, *v as i64), Typ::Bool)),
			Expr::Float(x) => Ok((self.b.ins().f64const(*x), Typ::Float)),
			Expr::String(s) => Ok((self.str_const(s), Typ::Str)),

			Expr::Ident(name) => {
				let local = self.vars.get(name).copied().ok_or_else(|| {
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
				for (arg, &expected) in args.iter().zip(&sig.params) {
					let (val, typ) = self.expr(arg)?;
					if typ != expected {
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

			Expr::Bind { .. } => unreachable!("bind in expression position"),
			Expr::Assign { .. } => unreachable!("assign in expression position"),
			Expr::Fn { .. } => unreachable!("fn definition in expression position"),
			Expr::Return(..) => unreachable!("return in expression position"),
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

	// The zero value for an Oi type.
	fn zero(&mut self, typ: Typ) -> Value {
		match typ {
			Typ::Float => self.b.ins().f64const(0.0),
			Typ::Str => self.str_const(""),
			Typ::Int | Typ::Bool => self.b.ins().iconst(self.int, 0),
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
		if let (Op::Add, Typ::Str, Typ::Str) = (op, lt, rt) {
			return Ok((self.call_concat(lv, rv), Typ::Str));
		}

		// no int/float mixing for now
		// NOTE: I might go with V-style promotion eventually.
		let float = match (lt, rt) {
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
		};
		Ok((out, if float { Typ::Float } else { Typ::Int }))
	}

	// Call the runtime string concat.
	fn call_concat(&mut self, a: Value, b: Value) -> Value {
		let mut sig = self.module.make_signature();
		sig.params.push(AbiParam::new(self.int));
		sig.params.push(AbiParam::new(self.int));
		sig.returns.push(AbiParam::new(self.int));
		let id = self
			.module
			.declare_function(runtime::STR_CONCAT, Linkage::Import, &sig)
			.unwrap();
		let func = self.module.declare_func_in_func(id, self.b.func);
		let call = self.b.ins().call(func, &[a, b]);
		self.b.inst_results(call)[0]
	}

	// Emit a call to the runtime print for the result's type.
	fn emit_print(&mut self, val: Value, typ: Typ) {
		let name = match typ {
			Typ::Bool => runtime::PRINT_BOOL,
			Typ::Int => runtime::PRINT_INT,
			Typ::Float => runtime::PRINT_FLOAT,
			Typ::Str => runtime::PRINT_STR,
		};
		let param = if typ == Typ::Float {
			types::F64
		} else {
			self.int
		};
		let mut sig = self.module.make_signature();
		sig.params.push(AbiParam::new(param));
		let id = self
			.module
			.declare_function(name, Linkage::Import, &sig)
			.unwrap();
		let func = self.module.declare_func_in_func(id, self.b.func);
		self.b.ins().call(func, &[val]);
	}
}
