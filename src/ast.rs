use chumsky::span::SimpleSpan;

// A value paired with the span it came from.
pub type Span = SimpleSpan;
pub type Spanned<T> = (T, Span);

#[allow(dead_code)]
#[derive(Debug)]
pub enum Expr {
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

	// unary operators
	Negative(Box<Spanned<Expr>>),

	// binary operators
	Add(Box<Spanned<Expr>>, Box<Spanned<Expr>>),
	Sub(Box<Spanned<Expr>>, Box<Spanned<Expr>>),
	Mul(Box<Spanned<Expr>>, Box<Spanned<Expr>>),
	Div(Box<Spanned<Expr>>, Box<Spanned<Expr>>),
}

// A function parameter.
// `typ` is the declared type name.
#[derive(Debug)]
pub struct Param {
	pub name: String,
	pub typ: String,
	pub span: Span,
}
