use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt;

use cranelift::codegen;
use cranelift::prelude::*;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module};

use crate::ast::{EnumVariant, Expr, Param, Span, Spanned, TypeExpr, TypeParam};
use crate::diagnostics::Diagnostic;
use crate::runtime;

mod lower;
use lower::Translator;

struct FnItem<'a> {
	key: String,
	params: &'a [Param],
	params_tuple: bool,
	ret: &'a Option<Spanned<TypeExpr>>,
	body: &'a [Spanned<Expr>],
}

type EnumItem<'a> = (&'a str, &'a [EnumVariant]);

// resolved params with an optional return annotation
type ParamsRet = (Vec<(String, Typ, bool)>, Option<(Typ, Span)>);

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
	FixedArray(Box<Typ>, usize),
	Struct(String, Vec<FieldDef>),
	Enum(String),
	Option(Box<Typ>),
	Result(Box<Typ>),
	AtomSum(Vec<String>),
	Error,
	Range,
	Fn(Vec<Typ>, Box<Typ>),
	Closure(Vec<Typ>, Box<Typ>),
	Map(Box<Typ>, Box<Typ>),
}

// A struct field definition.
#[derive(Clone, Debug)]
pub(crate) struct FieldDef {
	pub name: String,
	pub typ: Typ,
	pub default: Option<Spanned<Expr>>,
}

impl Typ {
	pub fn unit() -> Typ {
		Typ::Tuple(vec![])
	}

	pub fn is_unit(&self) -> bool {
		matches!(self, Typ::Tuple(f) if f.is_empty())
	}
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
			Typ::Array(e) => write!(f, "[]{e}"),
			Typ::FixedArray(e, n) => write!(f, "[{n}]{e}"),
			Typ::Struct(name, _) => write!(f, "{name}"),
			Typ::Enum(name) => write!(f, "{name}"),
			Typ::Option(inner) => write!(f, "?{inner}"),
			Typ::Result(inner) => write!(f, "!{inner}"),
			Typ::AtomSum(names) => {
				write!(
					f,
					"{}",
					names.iter().map(|n| format!(":{n}")).collect::<Vec<_>>().join(" | ")
				)
			}
			Typ::Error => write!(f, "Error"),
			Typ::Range => write!(f, "range"),
			Typ::Fn(params, ret) | Typ::Closure(params, ret) => {
				write!(f, "fn(")?;
				for (i, p) in params.iter().enumerate() {
					if i > 0 {
						write!(f, ", ")?;
					}
					write!(f, "{p}")?;
				}
				write!(f, ") {ret}")
			}
			Typ::Map(k, v) => write!(f, "Map[{k}, {v}]"),
		}
	}
}

impl PartialEq for FieldDef {
	fn eq(&self, other: &Self) -> bool {
		self.name == other.name && self.typ == other.typ
	}
}

pub(crate) fn oi_symbol(name: &str) -> String {
	format!("oi_{}", name.replace('.', "__"))
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

#[derive(Clone)]
pub(crate) struct VariantInfo {
	pub name: String,
	pub disc: i64,
	pub payload: Vec<Typ>,
}

impl VariantInfo {
	pub fn new(name: impl Into<String>, disc: i64, payload: Vec<Typ>) -> Self {
		VariantInfo {
			name: name.into(),
			disc,
			payload,
		}
	}
}

// An enum is a tagged union if any variant has fields.
pub(crate) fn enum_boxed(variants: &[VariantInfo]) -> bool {
	variants.iter().any(|v| !v.payload.is_empty())
}

// Slot count of a boxed enum.
pub(crate) fn enum_slots(variants: &[VariantInfo]) -> usize {
	// the tag plus the widest variant's fields
	1 + variants.iter().map(|v| v.payload.len()).max().unwrap_or(0)
}

pub(crate) fn option_variants(inner: &Typ) -> Vec<VariantInfo> {
	vec![
		VariantInfo::new("none", 0, vec![]),
		VariantInfo::new("some", 1, vec![inner.clone()]),
	]
}

pub(crate) fn result_variants(inner: &Typ) -> Vec<VariantInfo> {
	vec![
		VariantInfo::new("ok", 0, vec![inner.clone()]),
		VariantInfo::new("err", 1, vec![Typ::Error]),
	]
}

// An atom sum type desugars to a bare enum.
pub(crate) fn atom_sum_variants(names: &[String]) -> Vec<VariantInfo> {
	names
		.iter()
		.enumerate()
		.map(|(disc, name)| VariantInfo::new(name.clone(), disc as i64, vec![]))
		.collect()
}

// Assign discriminants and resolve payload types against `types`.
fn build_variants(variants: &[EnumVariant], types: TypeCtx) -> Result<Vec<VariantInfo>, Diagnostic> {
	let mut next = 0;
	variants
		.iter()
		.map(|v| {
			let disc = v.disc.unwrap_or(next);
			next = disc + 1;
			let payload = v
				.payload
				.iter()
				.map(|(te, span)| types.resolve(te, *span))
				.collect::<Result<Vec<_>, _>>()?;
			Ok(VariantInfo {
				name: v.name.clone(),
				disc,
				payload,
			})
		})
		.collect()
}

// The named types in scope for resolution.
#[derive(Clone, Copy)]
pub(crate) struct TypeCtx<'a> {
	pub structs: &'a HashMap<String, Vec<FieldDef>>,
	pub enums: &'a HashMap<String, Vec<VariantInfo>>,
	pub aliases: &'a HashMap<String, TypeExpr>,
	pub type_params: &'a HashMap<String, Typ>,
	pub generics: &'a Generics,
	// keep track of generic instantiations to catch recursion
	depth: usize,
}

impl<'a> TypeCtx<'a> {
	pub fn new(
		structs: &'a HashMap<String, Vec<FieldDef>>,
		enums: &'a HashMap<String, Vec<VariantInfo>>,
		aliases: &'a HashMap<String, TypeExpr>,
		type_params: &'a HashMap<String, Typ>,
		generics: &'a Generics,
	) -> Self {
		TypeCtx {
			structs,
			enums,
			aliases,
			type_params,
			generics,
			depth: 0,
		}
	}
}

// Try to parse `name` as `<prefix><width>`.
fn int_width(
	name: &str,
	prefix: char,
	ctor: fn(u16) -> Typ,
	label: &str,
	span: Span,
) -> Option<Result<Typ, Diagnostic>> {
	let rest = name.strip_prefix(prefix)?;
	let w = rest.parse::<u16>().ok()?;
	if w == 0 || w > 64 {
		return Some(Err(Diagnostic::new(
			format!("{label} width {w} out of range"),
			span.into_range(),
		)
		.with_label("width must be 1-64")));
	}
	Some(Ok(ctor(w)))
}

// nested generic instantiations allowed before calling it recursive
const MAX_GENERIC_DEPTH: usize = 64;

impl TypeCtx<'_> {
	// Resolve a type expression to a concrete `Typ`.
	pub fn resolve(&self, te: &TypeExpr, span: Span) -> Result<Typ, Diagnostic> {
		match te {
			TypeExpr::Name(name) => self.named(name, span),
			TypeExpr::Tuple(elems) => {
				let fields = elems
					.iter()
					.map(|e| Ok((None, self.resolve(e, span)?)))
					.collect::<Result<Vec<_>, _>>()?;
				Ok(Typ::Tuple(fields))
			}
			TypeExpr::Array(elem) => Ok(Typ::Array(Box::new(self.resolve(elem, span)?))),
			TypeExpr::FixedArray(elem, n) => Ok(Typ::FixedArray(Box::new(self.resolve(elem, span)?), *n)),
			TypeExpr::Option(inner) => Ok(Typ::Option(Box::new(self.resolve(inner, span)?))),
			TypeExpr::Result(inner, err) => {
				if let Some(e) = err
					&& !matches!(e.as_ref(), TypeExpr::Name(n) if n == "Error")
				{
					return Err(
						Diagnostic::new("custom error types aren't supported yet", span.into_range())
							.with_label("`Error` is the only accepted error type"),
					);
				}
				Ok(Typ::Result(Box::new(self.resolve(inner, span)?)))
			}
			TypeExpr::AtomSum(names) => {
				let mut seen = HashSet::new();
				if let Some(dup) = names.iter().find(|n| !seen.insert(*n)) {
					return Err(
						Diagnostic::new(format!("duplicate atom `:{dup}` in sum type"), span.into_range())
							.with_label("repeated atom"),
					);
				}
				Ok(Typ::AtomSum(names.clone()))
			}
			TypeExpr::Fn(params, ret) => {
				let params = params.iter().map(|p| self.resolve(p, span)).collect::<Result<_, _>>()?;
				Ok(Typ::Fn(params, Box::new(self.resolve(ret, span)?)))
			}
			TypeExpr::Map(k, v) => Ok(Typ::Map(
				Box::new(self.resolve(k, span)?),
				Box::new(self.resolve(v, span)?),
			)),
			TypeExpr::Generic(name, args) => {
				if let Some(def) = self.generics.structs.get(name) {
					let subst = self.generic_subst(name, &def.type_params, args, span)?;
					return self.instantiate(name, def, &subst, span);
				}
				if let Some(def) = self.generics.enums.get(name) {
					let subst = self.generic_subst(name, &def.type_params, args, span)?;
					return self.instantiate_enum(name, def, &subst, span);
				}
				let msg = match self.structs.contains_key(name) || self.enums.contains_key(name) {
					true => format!("`{name}` is not generic"),
					false => format!("unknown type `{name}`"),
				};
				Err(Diagnostic::new(msg, span.into_range()).with_label("no type arguments expected here"))
			}
		}
	}

	// Resolve `args` against `params`, yielding a substitution map.
	fn generic_subst(
		&self,
		name: &str,
		params: &[TypeParam],
		args: &[TypeExpr],
		span: Span,
	) -> Result<HashMap<String, Typ>, Diagnostic> {
		if args.len() != params.len() {
			return Err(Diagnostic::new(
				format!("`{name}` expects {} type argument(s), got {}", params.len(), args.len()),
				span.into_range(),
			)
			.with_label("wrong number of type arguments"));
		}
		let mut subst = HashMap::new();
		for (param, arg) in params.iter().zip(args) {
			subst.insert(param.name.clone(), self.resolve(arg, span)?);
		}
		Ok(subst)
	}

	// Substitute `subst` into a generic struct's fields, yielding an ordinary `Typ::Struct`.
	pub fn instantiate(
		&self,
		name: &str,
		def: &GenericStructDef,
		subst: &HashMap<String, Typ>,
		span: Span,
	) -> Result<Typ, Diagnostic> {
		if self.depth > MAX_GENERIC_DEPTH {
			return Err(
				Diagnostic::new(format!("`{name}` recurses without end"), span.into_range())
					.with_label("would require infinitely nested fields"),
			);
		}
		let inner = TypeCtx {
			type_params: subst,
			depth: self.depth + 1,
			..*self
		};
		let fields = def
			.fields
			.iter()
			.map(|f| {
				Ok(FieldDef {
					name: f.name.clone(),
					typ: inner.resolve(&f.typ, f.span)?,
					default: f.default.clone(),
				})
			})
			.collect::<Result<Vec<_>, _>>()?;
		let concrete: Vec<Typ> = def.type_params.iter().map(|p| subst[&p.name].clone()).collect();
		let args: Vec<_> = concrete.iter().map(Typ::to_string).collect();
		let display = format!("{name}[{}]", args.join(", "));
		self.generics
			.instance_args
			.borrow_mut()
			.entry(display.clone())
			.or_insert(concrete);
		Ok(Typ::Struct(display, fields))
	}

	// Substitute `subst` into a generic enum's variants.
	pub fn instantiate_enum(
		&self,
		name: &str,
		def: &GenericEnumDef,
		subst: &HashMap<String, Typ>,
		span: Span,
	) -> Result<Typ, Diagnostic> {
		if self.depth > MAX_GENERIC_DEPTH {
			return Err(
				Diagnostic::new(format!("`{name}` recurses without end"), span.into_range())
					.with_label("would require infinitely nested variants"),
			);
		}
		let args: Vec<_> = def.type_params.iter().map(|p| subst[&p.name].to_string()).collect();
		let display = format!("{name}[{}]", args.join(", "));
		if self.generics.instances.borrow().contains_key(&display) {
			return Ok(Typ::Enum(display));
		}
		self.generics.instances.borrow_mut().insert(display.clone(), Vec::new());
		let inner = TypeCtx {
			type_params: subst,
			depth: self.depth + 1,
			..*self
		};
		let variants = build_variants(&def.variants, inner)?;
		self.generics.instances.borrow_mut().insert(display.clone(), variants);
		Ok(Typ::Enum(display))
	}

	// Resolve a named type.
	pub fn named(&self, name: &str, span: Span) -> Result<Typ, Diagnostic> {
		if let Some(typ) = self.type_params.get(name) {
			return Ok(typ.clone());
		}
		match name {
			"int" => return Ok(Typ::Int(32)),
			"isize" => return Ok(Typ::ISize),
			"usize" => return Ok(Typ::USize),
			"float" => return Ok(Typ::Float(64)),
			"bool" => return Ok(Typ::Bool),
			"string" | "str" => return Ok(Typ::Str),
			"range" => return Ok(Typ::Range),
			"()" => return Ok(Typ::unit()),
			_ => {}
		}
		if let Some(result) = int_width(name, 'i', Typ::Int, "integer", span) {
			return result;
		}
		if let Some(result) = int_width(name, 'u', Typ::UInt, "unsigned integer", span) {
			return result;
		}
		if let Some(rest) = name.strip_prefix('f')
			&& let Ok(w) = rest.parse::<u16>()
		{
			return match w {
				16 => Ok(Typ::Float(16)),
				32 => Ok(Typ::Float(32)),
				64 => Ok(Typ::Float(64)),
				128 => Ok(Typ::Float(128)),
				_ => Err(
					Diagnostic::new(format!("unsupported float width f{w}"), span.into_range())
						.with_label("supported widths: f16, f32, f64, f128"),
				),
			};
		}
		if let Some(te) = self.aliases.get(name) {
			return self.resolve(te, span);
		}
		if let Some(fields) = self.structs.get(name) {
			return Ok(Typ::Struct(name.to_string(), fields.clone()));
		}
		if self.enums.contains_key(name) {
			return Ok(Typ::Enum(name.to_string()));
		}
		if self.generics.structs.contains_key(name) || self.generics.enums.contains_key(name) {
			return Err(
				Diagnostic::new(format!("`{name}` needs type arguments"), span.into_range())
					.with_label(format!("try `{name}[...]`")),
			);
		}
		Err(Diagnostic::new(format!("unknown type `{name}`"), span.into_range()).with_label("not a known type"))
	}

	// Resolve a param list to `(name, type, mutable)` triples.
	pub fn resolve_params(&self, params: &[Param]) -> Result<Vec<(String, Typ, bool)>, Diagnostic> {
		params
			.iter()
			.map(|p| Ok((p.name.clone(), self.resolve(&p.typ, p.span)?, p.mutable)))
			.collect()
	}

	// Resolve a param list plus an optional return type annotation.
	pub fn resolve_params_ret(
		&self,
		params: &[Param],
		ret: &Option<Spanned<TypeExpr>>,
	) -> Result<ParamsRet, Diagnostic> {
		let params = self.resolve_params(params)?;
		let ret = ret
			.as_ref()
			.map(|(te, span)| Ok::<_, Diagnostic>((self.resolve(te, *span)?, *span)))
			.transpose()?;
		Ok((params, ret))
	}
}

#[derive(Clone)]
pub(crate) struct FnSig {
	pub id: FuncId,
	pub params: Vec<Typ>,
	pub ret: Typ,
}

// A generic free function, monomorphized per callsite.
#[derive(Clone)]
pub(crate) struct GenericFnDef {
	pub params: Vec<Param>,
	pub params_tuple: bool,
	pub ret: Option<Spanned<TypeExpr>>,
	pub body: Vec<Spanned<Expr>>,
	pub type_params: Vec<TypeParam>,
	pub captures: Vec<(String, Typ, bool)>,
}

// A monomorphized instance whose sig is declared but body not yet compiled.
pub(crate) type Pending = (String, GenericFnDef, HashMap<String, Typ>);

// A generic struct definition.
#[derive(Clone)]
pub(crate) struct GenericStructDef {
	pub type_params: Vec<TypeParam>,
	pub fields: Vec<Param>,
}

// A generic enum definition.
#[derive(Clone)]
pub(crate) struct GenericEnumDef {
	pub type_params: Vec<TypeParam>,
	pub variants: Vec<EnumVariant>,
}

// Generic type definitions.
#[derive(Default)]
pub(crate) struct Generics {
	pub structs: HashMap<String, GenericStructDef>,
	pub enums: HashMap<String, GenericEnumDef>,
	// enum instances keyed by display name (`Opt[int]`)
	pub instances: RefCell<HashMap<String, Vec<VariantInfo>>>,
	// struct instances' concrete type args keyed by display name (`Box[int]`)
	pub instance_args: RefCell<HashMap<String, Vec<Typ>>>,
}

// Rewrite `Self` type refs to the owning type.
fn replace_self(te: &TypeExpr, self_ty: &TypeExpr) -> TypeExpr {
	match te {
		TypeExpr::Name(n) if n == "Self" => self_ty.clone(),
		TypeExpr::Array(e) => TypeExpr::Array(Box::new(replace_self(e, self_ty))),
		TypeExpr::FixedArray(e, n) => TypeExpr::FixedArray(Box::new(replace_self(e, self_ty)), *n),
		TypeExpr::Option(e) => TypeExpr::Option(Box::new(replace_self(e, self_ty))),
		TypeExpr::Result(e, err) => TypeExpr::Result(Box::new(replace_self(e, self_ty)), err.clone()),
		TypeExpr::Tuple(es) => TypeExpr::Tuple(es.iter().map(|e| replace_self(e, self_ty)).collect()),
		TypeExpr::Fn(ps, r) => TypeExpr::Fn(
			ps.iter().map(|p| replace_self(p, self_ty)).collect(),
			Box::new(replace_self(r, self_ty)),
		),
		TypeExpr::Map(k, v) => TypeExpr::Map(Box::new(replace_self(k, self_ty)), Box::new(replace_self(v, self_ty))),
		TypeExpr::Generic(name, args) => {
			TypeExpr::Generic(name.clone(), args.iter().map(|a| replace_self(a, self_ty)).collect())
		}
		other => other.clone(),
	}
}

#[derive(Clone)]
pub(crate) struct Local {
	pub var: Variable,
	pub typ: Typ,
	pub mutable: bool,
	pub boxed: bool,
}

impl Local {
	pub fn plain(var: Variable, typ: Typ, mutable: bool) -> Self {
		Local {
			var,
			typ,
			mutable,
			boxed: false,
		}
	}
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
	atoms: HashSet<String>,
	generics: HashMap<String, GenericFnDef>,
	mono: HashMap<String, FnSig>,
	pending: Vec<Pending>,
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
		builder.symbol(runtime::PANIC, runtime::panic as *const u8);
		builder.symbol(runtime::MAP_NEW, runtime::map_new as *const u8);
		builder.symbol(runtime::MAP_GET, runtime::map_get as *const u8);
		builder.symbol(runtime::MAP_SET, runtime::map_set as *const u8);
		builder.symbol(runtime::MAP_DELETE, runtime::map_delete as *const u8);

		let module = JITModule::new(builder);
		Self {
			builder_ctx: FunctionBuilderContext::new(),
			ctx: module.make_context(),
			module,
			string_idx: 0,
			atoms: HashSet::new(),
			generics: HashMap::new(),
			mono: HashMap::new(),
			pending: Vec::new(),
		}
	}
}

impl Compiler {
	pub fn compile(&mut self, program: &[Spanned<Expr>]) -> Result<*const u8, Diagnostic> {
		let mut struct_items: Vec<(&str, &[Param])> = vec![];
		let mut generics = Generics::default();
		let mut enum_items: Vec<EnumItem> = vec![];
		let mut alias_items: Vec<(&str, &TypeExpr)> = vec![];
		let mut main_body: Option<&[Spanned<Expr>]> = None;
		let mut others: Vec<FnItem> = vec![];
		let mut loose_refs: Vec<&Spanned<Expr>> = vec![];
		for item in program {
			match &item.0 {
				Expr::StructDef {
					name,
					type_params,
					fields,
				} if !type_params.is_empty() => {
					generics.structs.insert(
						name.clone(),
						GenericStructDef {
							type_params: type_params.clone(),
							fields: fields.clone(),
						},
					);
				}
				Expr::StructDef { name, fields, .. } => struct_items.push((name.as_str(), fields.as_slice())),
				Expr::EnumDef {
					name,
					type_params,
					variants,
				} if !type_params.is_empty() => {
					generics.enums.insert(
						name.clone(),
						GenericEnumDef {
							type_params: type_params.clone(),
							variants: variants.clone(),
						},
					);
				}
				Expr::EnumDef { name, variants, .. } => enum_items.push((name.as_str(), variants.as_slice())),
				Expr::TypeAlias { name, typ } => alias_items.push((name.as_str(), typ)),
				Expr::Impl {
					typ,
					type_params,
					methods,
				} => {
					for m in methods {
						let Expr::Fn {
							name,
							type_params: mtp,
							params,
							params_tuple,
							ret,
							body,
						} = &m.0
						else {
							continue;
						};
						if type_params.is_empty() && mtp.is_empty() {
							others.push(FnItem {
								key: format!("{typ}.{name}"),
								params,
								params_tuple: *params_tuple,
								ret,
								body,
							});
							continue;
						}
						let self_ty = if type_params.is_empty() {
							TypeExpr::Name(typ.clone())
						} else {
							let args = type_params.iter().map(|p| TypeExpr::Name(p.name.clone())).collect();
							TypeExpr::Generic(typ.clone(), args)
						};
						let params = params
							.iter()
							.map(|p| Param {
								typ: replace_self(&p.typ, &self_ty),
								..p.clone()
							})
							.collect();
						let ret = ret.as_ref().map(|(te, span)| (replace_self(te, &self_ty), *span));
						let mut all_params = type_params.clone();
						all_params.extend(mtp.clone());
						self.generics.insert(
							format!("{typ}.{name}"),
							GenericFnDef {
								params,
								params_tuple: *params_tuple,
								ret,
								body: body.clone(),
								type_params: all_params,
								captures: vec![],
							},
						);
					}
				}
				Expr::Fn { name, body, .. } if name == "main" => main_body = Some(body),
				Expr::Fn {
					name,
					type_params,
					params,
					params_tuple,
					ret,
					body,
				} if !type_params.is_empty() => {
					self.generics.insert(
						name.clone(),
						GenericFnDef {
							params: params.clone(),
							params_tuple: *params_tuple,
							ret: ret.clone(),
							body: body.clone(),
							type_params: type_params.clone(),
							captures: vec![],
						},
					);
				}
				Expr::Fn {
					name,
					params,
					params_tuple,
					ret,
					body,
					..
				} => others.push(FnItem {
					key: name.clone(),
					params,
					params_tuple: *params_tuple,
					ret,
					body,
				}),
				Expr::Doc(_) => {}
				_ => loose_refs.push(item),
			}
		}

		let aliases: HashMap<String, TypeExpr> =
			alias_items.iter().map(|(name, te)| (name.to_string(), (*te).clone())).collect();

		// name-only registry
		let enum_names: HashMap<String, Vec<VariantInfo>> =
			enum_items.iter().map(|(name, _)| (name.to_string(), Vec::new())).collect();

		// TODO: struct fields can't reference other structs yet, so resolve against none
		let no_structs: HashMap<String, Vec<FieldDef>> = HashMap::new();
		let no_type_params: HashMap<String, Typ> = HashMap::new();
		let field_types = TypeCtx::new(&no_structs, &enum_names, &aliases, &no_type_params, &generics);
		let structs: HashMap<String, Vec<FieldDef>> = struct_items
			.iter()
			.map(|(name, fields)| {
				let resolved = fields
					.iter()
					.map(|p| {
						field_types.resolve(&p.typ, p.span).map(|t| FieldDef {
							name: p.name.clone(),
							typ: t,
							default: p.default.clone(),
						})
					})
					.collect::<Result<Vec<_>, _>>()?;
				Ok((name.to_string(), resolved))
			})
			.collect::<Result<_, Diagnostic>>()?;

		let variant_types = TypeCtx::new(&structs, &enum_names, &aliases, &no_type_params, &generics);
		let enums: HashMap<String, Vec<VariantInfo>> = enum_items
			.iter()
			.map(|(name, variants)| Ok((name.to_string(), build_variants(variants, variant_types)?)))
			.collect::<Result<_, _>>()?;

		// hoist functions with an explicit return type
		let int = self.module.target_config().pointer_type();
		let mut funcs: HashMap<String, FnSig> = HashMap::new();
		for item in &others {
			let Some((ret_te, ret_span)) = item.ret else { continue };
			let mut aliases = aliases.clone();
			if let Some(t) = item.key.rsplit_once('.').map(|(t, _)| t) {
				aliases.insert("Self".into(), TypeExpr::Name(t.into()));
			}
			let types = TypeCtx::new(&structs, &enums, &aliases, &no_type_params, &generics);
			let param_typs: Vec<Typ> = item
				.params
				.iter()
				.map(|p| types.resolve(&p.typ, p.span))
				.collect::<Result<_, _>>()?;
			let ret = types.resolve(ret_te, *ret_span)?;
			let mut sig = self.module.make_signature();
			sig.params.extend(param_typs.iter().map(|t| AbiParam::new(cl_type(t, int))));
			if !ret.is_unit() {
				sig.returns.push(AbiParam::new(cl_type(&ret, int)));
			}
			let sym = oi_symbol(&item.key);
			let id = self
				.module
				.declare_function(&sym, Linkage::Local, &sig)
				.expect("declare function");
			funcs.insert(
				item.key.clone(),
				FnSig {
					id,
					params: param_typs,
					ret,
				},
			);
		}

		for item in &others {
			let self_type = item.key.rsplit_once('.').map(|(t, _)| t);
			let mut aliases = aliases.clone();
			if let Some(t) = self_type {
				aliases.insert("Self".into(), TypeExpr::Name(t.into()));
			}
			let types = TypeCtx::new(&structs, &enums, &aliases, &no_type_params, &generics);
			let (params, ret) = types.resolve_params_ret(item.params, item.ret)?;
			let sym = oi_symbol(&item.key);
			let ret = self.translate(
				&params,
				item.params_tuple,
				ret,
				item.body,
				&funcs,
				types,
				self_type,
				false,
				&[],
			)?;
			let id = self.finish_fn(&sym);
			let param_typs = params.iter().map(|(_, t, _)| t.clone()).collect();
			funcs.insert(
				item.key.clone(),
				FnSig {
					id,
					params: param_typs,
					ret,
				},
			);
		}

		// gather loose top-level statements
		let loose: Vec<Spanned<Expr>>;
		let entry: &[Spanned<Expr>] = match main_body {
			Some(body) => {
				if let Some(first) = loose_refs.first() {
					return Err(Diagnostic::new(
						"top-level statements are not allowed alongside `fn main`",
						first.1.into_range(),
					)
					.with_label("move this inside a function")
					.with_note("`fn main` is the entrypoint, so loose statements have nowhere to run"));
				}
				body
			}
			None => {
				loose = loose_refs.into_iter().cloned().collect();
				&loose
			}
		};

		let types = TypeCtx::new(&structs, &enums, &aliases, &no_type_params, &generics);
		let typ = self.translate(&[], true, None, entry, &funcs, types, None, true, &[])?;
		let entry_id = self.finish_fn("oi_main");

		// drain generic instances queued by calls we've seen
		while let Some((sym, def, subst)) = self.pending.pop() {
			let types = TypeCtx::new(&structs, &enums, &aliases, &subst, &generics);
			let (params, ret) = types.resolve_params_ret(&def.params, &def.ret)?;
			self.translate(
				&params,
				def.params_tuple,
				ret,
				&def.body,
				&funcs,
				types,
				None,
				false,
				&def.captures,
			)?;
			self.finish_fn(&sym);
		}

		let id = self.compile_entry(entry_id, typ, &funcs, types);

		self.module.finalize_definitions().expect("finalize definitions");
		Ok(self.module.get_finalized_function(id))
	}

	fn compile_entry(&mut self, entry: FuncId, typ: Typ, funcs: &HashMap<String, FnSig>, types: TypeCtx) -> FuncId {
		let int = self.module.target_config().pointer_type();
		let mut b = FunctionBuilder::new(&mut self.ctx.func, &mut self.builder_ctx);
		let block = b.create_block();
		b.switch_to_block(block);
		b.seal_block(block);

		let mut trans = Translator {
			int,
			b,
			vars: HashMap::new(),
			params: vec![],
			dollar: None,
			module: &mut self.module,
			funcs,
			structs: types.structs,
			enums: types.enums,
			aliases: types.aliases,
			type_params: types.type_params,
			generics: types.generics,
			generic_fns: &self.generics,
			mono: &mut self.mono,
			pending: &mut self.pending,
			string_idx: &mut self.string_idx,
			atoms: &mut self.atoms,
			ret: None,
			loops: vec![],
			self_type: None,
			is_main: false,
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
		self.module.define_function(id, &mut self.ctx).expect("define function");
		self.module.clear_context(&mut self.ctx);
		id
	}

	#[allow(clippy::too_many_arguments)]
	fn translate(
		&mut self,
		params: &[(String, Typ, bool)],
		params_tuple: bool,
		ret: Option<(Typ, Span)>,
		stmts: &[Spanned<Expr>],
		funcs: &HashMap<String, FnSig>,
		types: TypeCtx,
		self_type: Option<&str>,
		is_main: bool,
		captures: &[(String, Typ, bool)],
	) -> Result<Typ, Diagnostic> {
		let int = self.module.target_config().pointer_type();
		let mut b = FunctionBuilder::new(&mut self.ctx.func, &mut self.builder_ctx);
		// declare param types before the entry block claims them
		for (_, typ, _) in params {
			b.func.signature.params.push(AbiParam::new(cl_type(typ, int)));
		}
		if !captures.is_empty() {
			b.func.signature.params.push(AbiParam::new(int));
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
			params: vec![],
			dollar: None,
			module: &mut self.module,
			funcs,
			structs: types.structs,
			enums: types.enums,
			aliases: types.aliases,
			type_params: types.type_params,
			generics: types.generics,
			generic_fns: &self.generics,
			mono: &mut self.mono,
			pending: &mut self.pending,
			string_idx: &mut self.string_idx,
			atoms: &mut self.atoms,
			ret,
			loops: vec![],
			self_type: self_type.map(str::to_owned),
			is_main,
		};

		let param_vals: Vec<Value> = trans.b.block_params(block).to_vec();
		for ((name, typ, mutable), &val) in params.iter().zip(param_vals.iter()) {
			let cl = trans.b.func.dfg.value_type(val);
			let var = trans.b.declare_var(cl);
			trans.b.def_var(var, val);
			let local = Local::plain(var, typ.clone(), *mutable);
			trans.vars.insert(name.clone(), local.clone());
			trans.params.push(local);
		}
		trans.bind_dollar(params_tuple);

		if !captures.is_empty() {
			let env = param_vals[params.len()];
			for (i, (name, typ, boxed)) in captures.iter().enumerate() {
				let cl = if *boxed { trans.int } else { cl_type(typ, trans.int) };
				let val = trans.b.ins().load(cl, MemFlags::new(), env, ((i + 1) * 8) as i32);
				let var = trans.b.declare_var(cl);
				trans.b.def_var(var, val);
				let local = Local {
					var,
					typ: typ.clone(),
					mutable: *boxed,
					boxed: *boxed,
				};
				trans.vars.insert(name.clone(), local);
			}
		}

		let tail_target = trans.ret.as_ref().map(|(t, _)| t.clone());
		if let Some((val, typ)) = trans.block_tail(stmts, tail_target.as_ref())? {
			let span = stmts.last().map(|s| s.1).or(decl_span).unwrap_or((0..0).into());
			trans.emit_return(val, typ, span)?;
		}
		trans.b.finalize();

		Ok(trans.ret.map(|(t, _)| t).unwrap_or(Typ::unit()))
	}
}
