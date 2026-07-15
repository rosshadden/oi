use std::collections::{HashMap, HashSet};
use std::ops::Range;

use cranelift::codegen;
use cranelift::codegen::ir::immediates::{Ieee16, Ieee128};
use cranelift::codegen::ir::{StackSlotData, StackSlotKind};
use cranelift::prelude::*;
use cranelift_jit::JITModule;
use cranelift_module::{DataDescription, Linkage, Module};

use super::{
	FieldDef, FnSig, Local, LoopFrame, Op, Typ, TypeCtx, VariantInfo, atom_sum_variants, cl_int_for_width, cl_type,
	elem_size, enum_boxed, enum_slots, option_variants, result_variants,
};
use crate::ast::{Expr, MatchArm, Pattern, Span, Spanned, TypeExpr};
use crate::diagnostics::Diagnostic;
use crate::runtime;

mod array;
mod builtin;
mod call;
mod control;
mod core;
mod expr;
mod helpers;
mod op;
mod print;
mod stmt;
mod value;

use self::helpers::*;

pub(super) struct Translator<'a> {
	pub int: types::Type,
	pub b: FunctionBuilder<'a>,
	pub vars: HashMap<String, Local>,
	pub params: Vec<Local>,
	pub dollar: Option<(Value, Typ)>,
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
	pub is_main: bool,
}

// A statement that writes through an existing, mutable binding.
#[derive(Clone, Copy)]
enum Mutation {
	Assign,      // `x = v`
	IndexAssign, // `x[i] = v`
	Append,      // `x << v`
	FieldAssign, // `x.f = v`
}

// A destructured binding.
// `(name, type, offset)`
type Bind = (String, Typ, i32);
