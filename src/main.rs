use logos::Logos;
use chumsky::prelude::*;

#[derive(Logos, Debug)]
#[logos(skip r"[ \t\r\n\f]+")]
enum Token{
	#[regex(r"-?[0-9]+")]
	Int,

	// binary operators
	#[token("+")]
	Add,
	#[token("-")]
	Minus,
}

#[derive(Debug)]
enum Expr {
	Int,

	// unary operators
	Negative(Box<Expr>),

	// binary operators
	Add(Box<Expr>, Box<Expr>),
	Sub(Box<Expr>, Box<Expr>),
	Mul(Box<Expr>, Box<Expr>),
	Div(Box<Expr>, Box<Expr>),
}

fn lex(src: &str) -> Vec<Token> {
	let lexer = Token::lexer(src);
	let mut tokens = vec![];
	tokens
}

fn parse(tokens: Vec<Token>) {}

fn main() {
	let file = match std::env::args().nth(1) {
		Some(file) => file,
		None => "examples/hello.oi".into(),
	};
	let src = std::fs::read_to_string(file).unwrap();

	let lexed = lex(&src);
	println!("{}", src);

	let ast = parse(lexed);

	ast
}
