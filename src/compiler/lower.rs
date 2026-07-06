use std::collections::{HashMap, HashSet};
use std::ops::Range;

use cranelift::codegen;
use cranelift::codegen::ir::immediates::{Ieee16, Ieee128};
use cranelift::codegen::ir::{StackSlotData, StackSlotKind};
use cranelift::prelude::*;
use cranelift_jit::JITModule;
use cranelift_module::{DataDescription, Linkage, Module};

use super::{
	FieldDef, FnSig, Local, LoopFrame, Op, Typ, TypeCtx, VariantInfo, cl_int_for_width, cl_type, elem_size, enum_boxed,
	enum_slots,
};
use crate::ast::{Expr, MatchArm, Pattern, Span, Spanned, TypeExpr};
use crate::diagnostics::Diagnostic;
use crate::runtime;

mod array;
mod builtin;
mod call;
mod control;
mod expr;
mod op;
mod print;
mod stmt;
mod value;

pub(super) struct Translator<'a> {
	pub int: types::Type,
	pub b: FunctionBuilder<'a>,
	pub vars: HashMap<String, Local>,
	pub module: &'a mut JITModule,
	pub funcs: &'a HashMap<String, FnSig>,
	pub structs: &'a HashMap<String, Vec<FieldDef>>,
	pub enums: &'a HashMap<String, Vec<VariantInfo>>,
	pub aliases: &'a HashMap<String, TypeExpr>,
	pub string_idx: &'a mut usize,
	pub atoms: &'a mut HashSet<String>,
	pub ret: Option<(Typ, Span)>,
	pub loops: Vec<LoopFrame>,
	pub self_type: Option<String>,
}

// A statement that writes through an existing, mutable binding.
#[derive(Clone, Copy)]
enum Mutation {
	Assign,      // `x = v`
	IndexAssign, // `x[i] = v`
	Append,      // `x << v`
	FieldAssign, // `x.f = v`
}

impl<'a> Translator<'a> {
	// The named types in scope, bundled for resolving type annotations.
	fn types(&self) -> TypeCtx<'a> {
		TypeCtx {
			structs: self.structs,
			enums: self.enums,
			aliases: self.aliases,
		}
	}

	// Look up the binding that a mutation targets.
	fn mutable_local(&self, name: &str, span: Range<usize>, op: Mutation) -> Result<Local, Diagnostic> {
		// how the mutation reads in errors
		// (verb, verb when immutable, noun for the `mut` hint, suggest `:=`?)
		let (verb, immutable_verb, allow, suggest_declare) = match op {
			Mutation::Assign => ("assign to", "assign to", "assignment", true),
			Mutation::IndexAssign => ("assign to", "assign to element of", "assignment", true),
			Mutation::Append => ("append to", "append to", "append", false),
			Mutation::FieldAssign => ("assign field of", "assign field of", "field assignment", false),
		};
		let local = self.vars.get(name).cloned().ok_or_else(|| {
			let d = Diagnostic::new(format!("cannot {verb} undefined variable `{name}`"), span.clone())
				.with_label("not found in scope");
			if suggest_declare {
				d.with_note(format!("declare it first with `{name} := ...`"))
			} else {
				d
			}
		})?;
		if !local.mutable {
			return Err(
				Diagnostic::new(format!("cannot {immutable_verb} immutable `{name}`"), span)
					.with_label("declared without `mut`")
					.with_note(format!("use `mut {name} := ...` to allow {allow}")),
			);
		}
		Ok(local)
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
			_ => Err(Diagnostic::new("patterns must bind names", e.1.into_range()).with_label("not a name")),
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
					Diagnostic::new("struct patterns must bind names", e.1.into_range()).with_label("not a name")
				);
			};
			let field = fname.as_deref().unwrap_or(local);
			let idx = fdefs.iter().position(|f| f.name == field).ok_or_else(|| {
				Diagnostic::new(format!("struct `{sname}` has no field `{field}`"), e.1.into_range())
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

// The width of `i{N}` and `i{N}` casts.
fn int_cast_width(prefix: char, name: &str) -> Option<u16> {
	name.strip_prefix(prefix)
		.and_then(|w| w.parse::<u16>().ok())
		.filter(|&w| w > 0 && w <= 64)
}

fn uint_max(width: u16) -> i64 {
	if width >= 64 {
		u64::MAX as i64
	} else {
		((1u64 << width) - 1) as i64
	}
}

fn int_min(width: u16) -> i64 {
	if width >= 64 { i64::MIN } else { -(1i64 << (width - 1)) }
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
