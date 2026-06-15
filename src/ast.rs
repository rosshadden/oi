#[allow(dead_code)]
#[derive(Debug)]
pub enum Expr {
	Bool(bool),
	Int(i32),
	Float(f64),
	String(String),
	Ident(String),

	Assign {
		mutable: bool,
		name: String,
		value: Box<Expr>,
	},

	Fn {
		name: String,
		body: Vec<Expr>,
	},

	// unary operators
	Negative(Box<Expr>),

	// binary operators
	Add(Box<Expr>, Box<Expr>),
	Sub(Box<Expr>, Box<Expr>),
	Mul(Box<Expr>, Box<Expr>),
	Div(Box<Expr>, Box<Expr>),
}
