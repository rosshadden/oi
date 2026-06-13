use std::collections::HashMap;

use cranelift::codegen;
use cranelift::prelude::*;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{DataDescription, Linkage, Module};

use crate::ast::Expr;

pub struct Compiler {
	builder_ctx: FunctionBuilderContext,
	ctx: codegen::Context,
	data_description: DataDescription,
	module: JITModule,
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
		let builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());

		let module = JITModule::new(builder);
		Self {
			builder_ctx: FunctionBuilderContext::new(),
			ctx: module.make_context(),
			data_description: DataDescription::new(),
			module,
		}
	}
}

impl Compiler {
	pub fn compile(&mut self, program: &[Expr]) -> Result<*const u8, String> {
		let int = self.module.target_config().pointer_type();
		self.ctx.func.signature.returns.push(AbiParam::new(int));
		self.translate(int, program)?;
		let id = self
			.module
			.declare_function("__oi_main", Linkage::Local, &self.ctx.func.signature)
			.map_err(|e| e.to_string())?;
		self.module
			.define_function(id, &mut self.ctx)
			.map_err(|e| e.to_string())?;
		self.module.clear_context(&mut self.ctx);
		self.module.finalize_definitions().unwrap();
		let code = self.module.get_finalized_function(id);
		Ok(code)
	}

	fn translate(&mut self, int: types::Type, program: &[Expr]) -> Result<(), String> {
		let mut b = FunctionBuilder::new(&mut self.ctx.func, &mut self.builder_ctx);
		let block = b.create_block();
		b.switch_to_block(block);
		b.seal_block(block);

		let vars = declare_vars(int, &mut b, program);
		let mut trans = Translator {
			int,
			b,
			vars,
			module: &mut self.module,
			string_idx: 0,
		};

		let mut last = trans.b.ins().iconst(int, 0);
		for stmt in program {
			match stmt {
				Expr::Assign { name, value, .. } => {
					let val = trans.expr(value);
					let var = trans.vars[name.as_str()];
					trans.b.def_var(var, val);
				}
				e => last = trans.expr(e),
			}
		}

		trans.b.ins().return_(&[last]);
		trans.b.finalize();
		Ok(())
	}
}

struct Translator<'a> {
	int: types::Type,
	b: FunctionBuilder<'a>,
	vars: HashMap<String, Variable>,
	module: &'a mut JITModule,
	string_idx: usize,
}

impl<'a> Translator<'a> {
	fn expr(&mut self, expr: &Expr) -> Value {
		match expr {
			Expr::Int(n) => self.b.ins().iconst(self.int, *n as i64),
			Expr::Bool(v) => self.b.ins().iconst(self.int, *v as i64),
			Expr::Float(_) => todo!("float"),

			Expr::String(s) => {
				let mut bytes = s.as_bytes().to_vec();
				bytes.push(0);
				let name = format!("__str_{}", self.string_idx);
				self.string_idx += 1;
				let id = self
					.module
					.declare_data(&name, Linkage::Local, false, false)
					.unwrap();
				let mut desc = DataDescription::new();
				desc.define(bytes.into_boxed_slice());
				self.module.define_data(id, &desc).unwrap();
				let gv = self.module.declare_data_in_func(id, &mut self.b.func);
				self.b.ins().symbol_value(self.int, gv)
			}

			Expr::Ident(name) => self.b.use_var(
				*self
					.vars
					.get(name)
					.unwrap_or_else(|| panic!("undefined: {name}")),
			),

			Expr::Negative(e) => {
				let v = self.expr(e);
				self.b.ins().ineg(v)
			}

			Expr::Add(l, r) => {
				let (lv, rv) = (self.expr(l), self.expr(r));
				self.b.ins().iadd(lv, rv)
			}
			Expr::Sub(l, r) => {
				let (lv, rv) = (self.expr(l), self.expr(r));
				self.b.ins().isub(lv, rv)
			}
			Expr::Mul(l, r) => {
				let (lv, rv) = (self.expr(l), self.expr(r));
				self.b.ins().imul(lv, rv)
			}
			Expr::Div(l, r) => {
				let (lv, rv) = (self.expr(l), self.expr(r));
				self.b.ins().sdiv(lv, rv)
			}

			Expr::Assign { .. } => unreachable!("assign in expression position"),
		}
	}
}

fn declare_vars(
	int: types::Type,
	b: &mut FunctionBuilder,
	stmts: &[Expr],
) -> HashMap<String, Variable> {
	let mut vars = HashMap::new();
	for stmt in stmts {
		declare_vars_in_expr(int, b, &mut vars, stmt);
	}
	vars
}

fn declare_vars_in_expr(
	int: types::Type,
	b: &mut FunctionBuilder,
	vars: &mut HashMap<String, Variable>,
	expr: &Expr,
) {
	match expr {
		Expr::Assign { name, value, .. } => {
			vars.entry(name.clone())
				.or_insert_with(|| b.declare_var(int));
			declare_vars_in_expr(int, b, vars, value);
		}
		Expr::Negative(e) => declare_vars_in_expr(int, b, vars, e),
		Expr::Add(l, r) | Expr::Sub(l, r) | Expr::Mul(l, r) | Expr::Div(l, r) => {
			declare_vars_in_expr(int, b, vars, l);
			declare_vars_in_expr(int, b, vars, r);
		}
		_ => {}
	}
}
