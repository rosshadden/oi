use std::collections::HashMap;
use std::fmt;

use cranelift::codegen;
use cranelift::prelude::*;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module};

use crate::ast::{Expr, Param, Span, Spanned, TypeExpr};
use crate::diagnostics::Diagnostic;
use crate::runtime;

mod lower;
use lower::Translator;

type FnItem<'a> = (
	&'a str,
	&'a [Param],
	&'a Option<Spanned<TypeExpr>>,
	&'a [Spanned<Expr>],
);

// TODO: PartialEq compares tuple field names, but comparisons should ignore them
#[derive(Clone, PartialEq, Debug)]
pub(crate) enum Typ {
	Int(u16),
	UInt(u16),
	ISize,
	USize,
	Float(u16),
	Bool,
	Str,
	Atom,
	Tuple(Vec<(Option<String>, Typ)>),
	Array(Box<Typ>),
	Struct(String, Vec<FieldDef>),
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Op {
	Add,
	Sub,
	Mul,
	Div,
	Mod,
}

// A struct field definition.
#[derive(Clone, Debug)]
pub(crate) struct FieldDef {
	pub name: String,
	pub typ: Typ,
	pub default: Option<Spanned<Expr>>,
}

impl fmt::Display for Typ {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Typ::Int(32) => write!(f, "int"),
			Typ::Int(w) => write!(f, "i{w}"),
			Typ::UInt(w) => write!(f, "u{w}"),
			Typ::ISize => write!(f, "isize"),
			Typ::USize => write!(f, "usize"),
			Typ::Float(64) => write!(f, "float"),
			Typ::Float(w) => write!(f, "f{w}"),
			Typ::Bool => write!(f, "bool"),
			Typ::Str => write!(f, "str"),
			Typ::Atom => write!(f, "atom"),
			Typ::Tuple(fields) if fields.is_empty() => write!(f, "()"),
			Typ::Tuple(_) => write!(f, "tuple"),
			Typ::Array(e) => write!(f, "[{e}]"),
			Typ::Struct(name, _) => write!(f, "{name}"),
		}
	}
}

impl PartialEq for FieldDef {
	fn eq(&self, other: &Self) -> bool {
		self.name == other.name && self.typ == other.typ
	}
}

pub(crate) fn cl_int_for_width(w: u16) -> types::Type {
	match w {
		1..=8 => types::I8,
		9..=16 => types::I16,
		17..=32 => types::I32,
		_ => types::I64,
	}
}

pub(crate) fn cl_type(typ: &Typ, int: types::Type) -> types::Type {
	match typ {
		Typ::Int(w) | Typ::UInt(w) => cl_int_for_width(*w),
		Typ::ISize | Typ::USize => int,
		Typ::Float(w) => match w {
			16 => types::F16,
			32 => types::F32,
			64 => types::F64,
			128 => types::F128,
			w => panic!("unsupported float width f{w}"),
		},
		_ => int,
	}
}

pub(crate) fn elem_size(typ: &Typ) -> i64 {
	match typ {
		Typ::Int(w) | Typ::UInt(w) => cl_int_for_width(*w).bytes() as i64,
		Typ::Float(w) => (*w as i64) / 8,
		_ => 8,
	}
}

pub(crate) fn resolve_type(
	te: &TypeExpr,
	span: Span,
	structs: &HashMap<String, Vec<FieldDef>>,
	aliases: &HashMap<String, TypeExpr>,
) -> Result<Typ, Diagnostic> {
	match te {
		TypeExpr::Name(name) => typ_from_name(name, span, structs, aliases),
		TypeExpr::Tuple(elems) => {
			let fields = elems
				.iter()
				.map(|e| Ok((None, resolve_type(e, span, structs, aliases)?)))
				.collect::<Result<Vec<_>, _>>()?;
			Ok(Typ::Tuple(fields))
		}
		TypeExpr::Array(elem) => Ok(Typ::Array(Box::new(resolve_type(
			elem, span, structs, aliases,
		)?))),
		TypeExpr::Fn(_, _) => Err(Diagnostic::new(
			"function types are not yet supported in codegen",
			span.into_range(),
		)
		.with_label("cannot use a function type here yet")),
	}
}

pub(crate) fn typ_from_name(
	name: &str,
	span: Span,
	structs: &HashMap<String, Vec<FieldDef>>,
	aliases: &HashMap<String, TypeExpr>,
) -> Result<Typ, Diagnostic> {
	match name {
		"int" => return Ok(Typ::Int(32)),
		"isize" => return Ok(Typ::ISize),
		"usize" => return Ok(Typ::USize),
		"float" => return Ok(Typ::Float(64)),
		"bool" => return Ok(Typ::Bool),
		"string" | "str" => return Ok(Typ::Str),
		"()" => return Ok(Typ::Tuple(vec![])),
		_ => {}
	}
	if let Some(rest) = name.strip_prefix('i') {
		if let Ok(w) = rest.parse::<u16>() {
			if w == 0 || w > 64 {
				return Err(Diagnostic::new(
					format!("integer width {w} out of range"),
					span.into_range(),
				)
				.with_label("width must be 1–64"));
			}
			return Ok(Typ::Int(w));
		}
	}
	if let Some(rest) = name.strip_prefix('u') {
		if let Ok(w) = rest.parse::<u16>() {
			if w == 0 || w > 64 {
				return Err(Diagnostic::new(
					format!("unsigned integer width {w} out of range"),
					span.into_range(),
				)
				.with_label("width must be 1–64"));
			}
			return Ok(Typ::UInt(w));
		}
	}
	if let Some(rest) = name.strip_prefix('f') {
		if let Ok(w) = rest.parse::<u16>() {
			return match w {
				16 => Ok(Typ::Float(16)),
				32 => Ok(Typ::Float(32)),
				64 => Ok(Typ::Float(64)),
				128 => Ok(Typ::Float(128)),
				_ => Err(Diagnostic::new(
					format!("unsupported float width f{w}"),
					span.into_range(),
				)
				.with_label("supported widths: f16, f32, f64, f128")),
			};
		}
	}
	if let Some(te) = aliases.get(name) {
		return resolve_type(te, span, structs, aliases);
	}
	if let Some(fields) = structs.get(name) {
		return Ok(Typ::Struct(name.to_string(), fields.clone()));
	}
	Err(
		Diagnostic::new(format!("unknown type `{name}`"), span.into_range())
			.with_label("not a known type"),
	)
}

#[derive(Clone)]
pub(crate) struct FnSig {
	pub id: FuncId,
	pub params: Vec<Typ>,
	pub ret: Typ,
}

#[derive(Clone)]
pub(crate) struct Local {
	pub var: Variable,
	pub typ: Typ,
	pub mutable: bool,
}

// `continue` jumps to `top`, `break` jumps to `exit`
pub(crate) struct LoopFrame {
	pub top: Block,
	pub exit: Option<Block>,
}

pub struct Compiler {
	builder_ctx: FunctionBuilderContext,
	ctx: codegen::Context,
	module: JITModule,
	string_idx: usize,
	atoms: HashMap<String, ()>,
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
			module,
			string_idx: 0,
			atoms: HashMap::new(),
		}
	}
}

impl Compiler {
	pub fn compile(&mut self, program: &[Spanned<Expr>]) -> Result<*const u8, Diagnostic> {
		let int = self.module.target_config().pointer_type();

		let mut struct_items: Vec<(&str, &[Param])> = vec![];
		let mut alias_items: Vec<(&str, &TypeExpr)> = vec![];
		let mut main_body: Option<&[Spanned<Expr>]> = None;
		let mut others: Vec<FnItem> = vec![];
		let mut loose: Vec<&Spanned<Expr>> = vec![];
		for item in program {
			match &item.0 {
				Expr::StructDef { name, fields } => {
					struct_items.push((name.as_str(), fields.as_slice()))
				}
				Expr::TypeAlias { name, typ } => alias_items.push((name.as_str(), typ)),
				Expr::Fn { name, body, .. } if name == "main" => main_body = Some(body),
				Expr::Fn {
					name,
					params,
					ret,
					body,
				} => others.push((name.as_str(), params, ret, body)),
				Expr::Doc(_) => {}
				_ => loose.push(item),
			}
		}

		let aliases: HashMap<String, TypeExpr> = alias_items
			.iter()
			.map(|(name, te)| (name.to_string(), (*te).clone()))
			.collect();

		let mut structs: HashMap<String, Vec<FieldDef>> = HashMap::new();
		let no_structs: HashMap<String, Vec<FieldDef>> = HashMap::new();
		for (name, fields) in &struct_items {
			let resolved = fields
				.iter()
				.map(|p| {
					typ_from_name(&p.typ, p.span, &no_structs, &aliases).map(|t| FieldDef {
						name: p.name.clone(),
						typ: t,
						default: p.default.clone(),
					})
				})
				.collect::<Result<Vec<_>, _>>()?;
			structs.insert(name.to_string(), resolved);
		}

		let mut funcs: HashMap<String, FnSig> = HashMap::new();
		for &(name, params, ret, body) in &others {
			let params: Vec<(String, Typ)> = params
				.iter()
				.map(|p| {
					Ok((
						p.name.clone(),
						typ_from_name(&p.typ, p.span, &structs, &aliases)?,
					))
				})
				.collect::<Result<_, Diagnostic>>()?;
			let ret = ret
				.as_ref()
				.map(|(te, span)| {
					Ok::<_, Diagnostic>((resolve_type(te, *span, &structs, &aliases)?, *span))
				})
				.transpose()?;
			let stmts: Vec<&Spanned<Expr>> = body.iter().collect();
			let (id, ret) = self.compile_fn(
				int,
				&format!("oi_{name}"),
				&params,
				ret,
				&stmts,
				&funcs,
				&structs,
			)?;
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

		let (entry_id, typ) =
			self.compile_fn(int, "oi_main", &[], None, &entry, &funcs, &structs)?;
		let id = self.compile_entry(int, entry_id, typ, &funcs, &structs);

		self.module
			.finalize_definitions()
			.expect("finalize definitions");
		Ok(self.module.get_finalized_function(id))
	}

	fn compile_fn(
		&mut self,
		int: types::Type,
		name: &str,
		params: &[(String, Typ)],
		ret: Option<(Typ, Span)>,
		stmts: &[&Spanned<Expr>],
		funcs: &HashMap<String, FnSig>,
		structs: &HashMap<String, Vec<FieldDef>>,
	) -> Result<(FuncId, Typ), Diagnostic> {
		let typ = self.translate(int, params, ret, stmts, funcs, structs)?;
		let id = self.finish_fn(name);
		Ok((id, typ))
	}

	fn compile_entry(
		&mut self,
		int: types::Type,
		entry: FuncId,
		typ: Typ,
		funcs: &HashMap<String, FnSig>,
		structs: &HashMap<String, Vec<FieldDef>>,
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
			structs,
			string_idx: &mut self.string_idx,
			atoms: &mut self.atoms,
			ret: None,
			loops: vec![],
		};

		let callee = trans.module.declare_func_in_func(entry, trans.b.func);
		let call = trans.b.ins().call(callee, &[]);
		if let Some(val) = trans.b.inst_results(call).first().copied() {
			trans.emit_print(val, &typ, false, false);
			trans.write_lit("\n", false);
		}
		trans.b.ins().return_(&[]);
		trans.b.finalize();

		self.finish_fn("__oi_main")
	}

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
		structs: &HashMap<String, Vec<FieldDef>>,
	) -> Result<Typ, Diagnostic> {
		let mut b = FunctionBuilder::new(&mut self.ctx.func, &mut self.builder_ctx);
		// declare param types before the entry block claims them
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

		let decl_span = ret.as_ref().map(|(_, s)| *s);

		let mut trans = Translator {
			int,
			b,
			vars: HashMap::new(),
			module: &mut self.module,
			funcs,
			structs,
			string_idx: &mut self.string_idx,
			atoms: &mut self.atoms,
			ret,
			loops: vec![],
		};

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

		if let Some((val, typ)) = trans.block(stmts)? {
			let span = stmts
				.last()
				.map(|s| s.1)
				.or(decl_span)
				.unwrap_or((0..0).into());
			trans.emit_return(val, typ, span)?;
		}
		trans.b.finalize();

		Ok(trans.ret.map(|(t, _)| t).unwrap_or(Typ::Tuple(vec![])))
	}
}
