use chumsky::span::SimpleSpan;

// A value paired with the span it came from.
pub type Span = SimpleSpan;
pub type Spanned<T> = (T, Span);

#[allow(dead_code)]
#[derive(Debug)]
pub enum Expr {
	// literals
	Bool(bool),
	Int(i32),
	Float(f64),
	String(String),
	Ident(String),

	// `[mods] name := value`: declares a new binding
	Bind {
		mutable: bool,
		name: String,
		value: Box<Spanned<Expr>>,
	},

	// `name = value`: assigns to an existing mutable binding
	Assign {
		name: String,
		value: Box<Spanned<Expr>>,
	},

	Fn {
		name: String,
		params: Vec<Param>,
		ret: Option<Spanned<String>>,
		body: Vec<Spanned<Expr>>,
	},

	Call {
		name: String,
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

	Break,
	Continue,

	// structures
	Tuple(Vec<(Option<String>, Spanned<Expr>)>),
	Field {
		tuple: Box<Spanned<Expr>>,
		field: String,
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
}

// A function parameter.
// `typ` is the declared type name.
#[derive(Debug)]
pub struct Param {
	pub name: String,
	pub typ: String,
	pub span: Span,
}
