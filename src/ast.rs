use chumsky::span::SimpleSpan;

// A value paired with the span it came from.
pub type Span = SimpleSpan;
pub type Spanned<T> = (T, Span);

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum Expr {
	// literals
	Bool(bool),
	Int(i64),
	Float(f64),
	String(String),
	Atom(String),
	Ident(String),
	Dollar,
	None,

	// `[mods] name [type] := value`: declares a new binding
	Bind {
		mutable: bool,
		name: String,
		typ: Option<Spanned<TypeExpr>>,
		value: Option<Box<Spanned<Expr>>>,
	},

	// `[]T{}` or `[N]T{}`
	ArrayInit(Spanned<TypeExpr>),

	// `?T(value)` or `?T(none)`
	OptionInit {
		inner: Spanned<TypeExpr>,
		arg: Box<Spanned<Expr>>,
	},

	// `!T(value)` or `!T(error)`
	ResultInit {
		inner: Spanned<TypeExpr>,
		arg: Box<Spanned<Expr>>,
	},

	// `name = value`: assigns to an existing mutable binding
	Assign {
		name: String,
		value: Box<Spanned<Expr>>,
	},

	Fn {
		name: String,
		params: Vec<Param>,
		params_tuple: bool,
		ret: Option<Spanned<TypeExpr>>,
		body: Vec<Spanned<Expr>>,
	},

	Call {
		name: String,
		args: Vec<Spanned<Expr>>,
	},

	MethodCall {
		recv: Box<Spanned<Expr>>,
		method: String,
		args: Vec<Spanned<Expr>>,
	},

	Return(Option<Box<Spanned<Expr>>>),

	// control flow
	If {
		cond: Box<Spanned<Expr>>,
		then: Vec<Spanned<Expr>>,
		els: Option<Vec<Spanned<Expr>>>,
	},

	Loop {
		cond: Option<Box<Spanned<Expr>>>,
		body: Vec<Spanned<Expr>>,
	},

	// `loop <pat> in <iter> {}`
	For {
		pat: Pattern,
		iter: Box<Spanned<Expr>>,
		body: Vec<Spanned<Expr>>,
	},

	Break,
	Continue,

	// structures

	// tuples
	Tuple(Vec<(Option<String>, Spanned<Expr>)>),
	Field {
		tuple: Box<Spanned<Expr>>,
		field: String,
	},

	// arrays
	Array(Vec<Spanned<Expr>>),
	// `collection[index]`
	// TODO: handle negative indices
	Index {
		collection: Box<Spanned<Expr>>,
		index: Box<Spanned<Expr>>,
	},
	// `collection[start?..end?]`
	Slice {
		collection: Box<Spanned<Expr>>,
		start: Option<Box<Spanned<Expr>>>,
		end: Option<Box<Spanned<Expr>>>,
	},
	// `name[index] = value`
	IndexAssign {
		name: String,
		index: Box<Spanned<Expr>>,
		value: Box<Spanned<Expr>>,
	},
	// `name << value`
	Append {
		name: String,
		value: Box<Spanned<Expr>>,
	},

	// `match subject { pattern, ... { body } ... else { body } }`
	Match {
		subject: Box<Spanned<Expr>>,
		arms: Vec<MatchArm>,
		else_body: Option<Vec<Spanned<Expr>>>,
	},

	// `value or { body }`
	OrElse {
		value: Box<Spanned<Expr>>,
		body: Vec<Spanned<Expr>>,
	},

	// `value?`
	PropagateNone(Box<Spanned<Expr>>),
	// `value!`
	PropagateErr(Box<Spanned<Expr>>),

	// structs
	// `struct Name {}`
	StructDef {
		name: String,
		fields: Vec<Param>,
	},
	// `Name {}`
	StructLit {
		name: String,
		fields: Vec<(Option<String>, Spanned<Expr>)>,
	},
	// `name.field = value`
	FieldAssign {
		name: String,
		field: String,
		value: Box<Spanned<Expr>>,
	},

	Impl {
		typ: String,
		methods: Vec<Spanned<Expr>>,
	},

	// `type Name = TypeExpr`
	TypeAlias {
		name: String,
		typ: TypeExpr,
	},

	// `start..end`, `start..`, `..end`
	Range {
		start: Option<Box<Spanned<Expr>>>,
		end: Option<Box<Spanned<Expr>>>,
	},

	// `enum Name {}`
	EnumDef {
		name: String,
		variants: Vec<EnumVariant>,
	},
	// `.variant` or `.variant(args)`
	EnumShorthand {
		variant: String,
		args: Vec<Spanned<Expr>>,
	},

	// operators

	// unary
	Negative(Box<Spanned<Expr>>),

	// arithmetic
	Add(Box<Spanned<Expr>>, Box<Spanned<Expr>>),
	Sub(Box<Spanned<Expr>>, Box<Spanned<Expr>>),
	Mul(Box<Spanned<Expr>>, Box<Spanned<Expr>>),
	Div(Box<Spanned<Expr>>, Box<Spanned<Expr>>),
	Mod(Box<Spanned<Expr>>, Box<Spanned<Expr>>),

	// comparison
	Eq(Box<Spanned<Expr>>, Box<Spanned<Expr>>),
	Ne(Box<Spanned<Expr>>, Box<Spanned<Expr>>),
	Lt(Box<Spanned<Expr>>, Box<Spanned<Expr>>),
	Gt(Box<Spanned<Expr>>, Box<Spanned<Expr>>),
	Le(Box<Spanned<Expr>>, Box<Spanned<Expr>>),
	Ge(Box<Spanned<Expr>>, Box<Spanned<Expr>>),

	// logical
	And(Box<Spanned<Expr>>, Box<Spanned<Expr>>),
	Or(Box<Spanned<Expr>>, Box<Spanned<Expr>>),
	Not(Box<Spanned<Expr>>),

	// membership
	In(Box<Spanned<Expr>>, Box<Spanned<Expr>>),

	// meta
	Doc(Vec<String>),
}

// Type annotation.
#[derive(Debug, Clone)]
pub enum TypeExpr {
	Name(String),
	Tuple(Vec<TypeExpr>),
	Array(Box<TypeExpr>),
	FixedArray(Box<TypeExpr>, usize),
	Fn(Vec<TypeExpr>, Box<TypeExpr>),
	Option(Box<TypeExpr>),
	Result(Box<TypeExpr>),
	AtomSum(Vec<String>),
}

#[derive(Debug, Clone)]
// One arm of a `match` expression.
// `patterns` are compared to the subject (OR'd together).
// `binding @` names the subject value for the arm body.
// `body` runs when any pattern matches.
pub struct MatchArm {
	pub binding: Option<String>,
	pub patterns: Vec<Spanned<Expr>>,
	pub body: Vec<Spanned<Expr>>,
}

// Enum variant.
#[derive(Debug, Clone)]
pub struct EnumVariant {
	pub name: String,
	pub disc: Option<i64>,
	pub payload: Vec<Spanned<TypeExpr>>,
}

// A `loop` binding pattern (name or destruction).
#[derive(Debug, Clone)]
pub enum Pattern {
	Name(String),
	Tuple(Vec<String>),
}

// A function parameter or struct field declaration.
#[derive(Debug, Clone)]
pub struct Param {
	pub name: String,
	pub typ: TypeExpr,
	pub span: Span,
	pub default: Option<Spanned<Expr>>,
	pub mutable: bool,
}
