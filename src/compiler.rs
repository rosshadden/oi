use std::collections::HashMap;

use cranelift::codegen;
use cranelift::prelude::*;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{DataDescription, FuncId, Linkage, Module};

use crate::ast::Expr;
use crate::runtime;

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
	pub fn compile(&mut self, program: &[Expr]) -> Result<*const u8, String> {
		let int = self.module.target_config().pointer_type();

		// split the top level into fn defs and loose statements
		let mut main_body: Option<&[Expr]> = None;
		let mut others: Vec<(&str, &[Expr])> = vec![];
		let mut loose: Vec<&Expr> = vec![];
		for item in program {
			match item {
				Expr::Fn { name, body } if name == "main" => main_body = Some(body),
				Expr::Fn { name, body } => others.push((name.as_str(), body)),
				other => loose.push(other),
			}
		}

		// every named function is compiled
		// only the entrypoint prints its result
		for &(name, body) in &others {
			let stmts: Vec<&Expr> = body.iter().collect();
			self.compile_fn(int, &format!("oi_{name}"), &stmts, false)?;
		}

		// `main` is the entrypoint if present
		// otherwise top-level statements run as if wrapped in an implicit `main`
		let entry: Vec<&Expr> = match main_body {
			Some(body) => {
				if !loose.is_empty() {
					return Err("top-level statements are not allowed alongside `fn main`".into());
				}
				body.iter().collect()
			}
			None => loose,
		};
		let id = self.compile_fn(int, "__oi_main", &entry, true)?;

		self.module.finalize_definitions().unwrap();
		Ok(self.module.get_finalized_function(id))
	}

	// Declare and define a function from a list of statements.
	fn compile_fn(
		&mut self,
		int: types::Type,
		name: &str,
		stmts: &[&Expr],
		print_last: bool,
	) -> Result<FuncId, String> {
		self.translate(int, stmts, print_last);
		let id = self
			.module
			.declare_function(name, Linkage::Local, &self.ctx.func.signature)
			.map_err(|e| e.to_string())?;
		self.module
			.define_function(id, &mut self.ctx)
			.map_err(|e| e.to_string())?;
		self.module.clear_context(&mut self.ctx);
		Ok(id)
	}

	fn translate(&mut self, int: types::Type, stmts: &[&Expr], print_last: bool) {
		let mut b = FunctionBuilder::new(&mut self.ctx.func, &mut self.builder_ctx);
		let block = b.create_block();
		b.switch_to_block(block);
		b.seal_block(block);

		let mut trans = Translator {
			int,
			b,
			vars: HashMap::new(),
			module: &mut self.module,
			string_idx: &mut self.string_idx,
		};

		let mut last = (trans.b.ins().iconst(int, 0), Typ::Int);
		for &stmt in stmts {
			match stmt {
				Expr::Assign { name, value, .. } => {
					let (val, typ) = trans.expr(value);
					// a variable takes the type of its first assigned value
					let var = match trans.vars.get(name) {
						Some(&(var, _)) => var,
						None => {
							let cl = trans.b.func.dfg.value_type(val);
							let var = trans.b.declare_var(cl);
							trans.vars.insert(name.clone(), (var, typ));
							var
						}
					};
					trans.b.def_var(var, val);
				}
				e => last = trans.expr(e),
			}
		}

		if print_last {
			let (val, typ) = last;
			trans.emit_print(val, typ);
		}
		trans.b.ins().return_(&[]);
		trans.b.finalize();
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

struct Translator<'a> {
	int: types::Type,
	b: FunctionBuilder<'a>,
	vars: HashMap<String, (Variable, Typ)>,
	module: &'a mut JITModule,
	string_idx: &'a mut usize,
}

impl<'a> Translator<'a> {
	fn expr(&mut self, expr: &Expr) -> (Value, Typ) {
		match expr {
			Expr::Int(n) => (self.b.ins().iconst(self.int, *n as i64), Typ::Int),
			Expr::Bool(v) => (self.b.ins().iconst(self.int, *v as i64), Typ::Bool),
			Expr::Float(x) => (self.b.ins().f64const(*x), Typ::Float),

			Expr::String(s) => {
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
				let gv = self.module.declare_data_in_func(id, &mut self.b.func);
				(self.b.ins().symbol_value(self.int, gv), Typ::Str)
			}

			Expr::Ident(name) => {
				let (var, typ) = *self
					.vars
					.get(name)
					.unwrap_or_else(|| panic!("undefined: {name}"));
				(self.b.use_var(var), typ)
			}

			Expr::Negative(e) => {
				let (v, typ) = self.expr(e);
				let out = match typ {
					Typ::Int => self.b.ins().ineg(v),
					Typ::Float => self.b.ins().fneg(v),
					_ => panic!("cannot negate {typ:?}"),
				};
				(out, typ)
			}

			Expr::Add(l, r) => self.binop(Op::Add, l, r),
			Expr::Sub(l, r) => self.binop(Op::Sub, l, r),
			Expr::Mul(l, r) => self.binop(Op::Mul, l, r),
			Expr::Div(l, r) => self.binop(Op::Div, l, r),

			Expr::Assign { .. } => unreachable!("assign in expression position"),
			Expr::Fn { .. } => unreachable!("fn definition in expression position"),
		}
	}

	// Add binary op instruction.
	fn binop(&mut self, op: Op, l: &Expr, r: &Expr) -> (Value, Typ) {
		let (lv, lt) = self.expr(l);
		let (rv, rt) = self.expr(r);

		// string concatenation
		if let (Op::Add, Typ::Str, Typ::Str) = (op, lt, rt) {
			return (self.call_concat(lv, rv), Typ::Str);
		}

		// no int/float mixing for now
		// NOTE: I might go with V-style promotion eventually.
		let float = match (lt, rt) {
			(Typ::Int, Typ::Int) => false,
			(Typ::Float, Typ::Float) => true,
			_ => panic!("cannot {op:?} {lt:?} and {rt:?}"),
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
		(out, if float { Typ::Float } else { Typ::Int })
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
