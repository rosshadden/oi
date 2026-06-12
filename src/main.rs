use chumsky::{
	input::{Stream, ValueInput},
	pratt::{infix, left, prefix},
	prelude::*,
};
use logos::{Logos, Span};

#[derive(Logos, Clone, PartialEq, Debug)]
#[logos(skip r"[ \t\r\n\f]+")]
enum Token {
	// primitive literals
	#[regex(r"[0-9]+", |lex| lex.slice().parse().ok())]
	Int(i32),
	#[regex(r"[0-9]+\.[0-9]+", |lex| lex.slice().parse().ok())]
	Float(f64),
	#[regex(r#""[^"]*""#, |lex| { let s = lex.slice(); s[1..s.len() - 1].to_string() })]
	String(String),

	// identifiers
	#[token("mut")]
	Mut,
	#[regex(r"[A-Za-z_][A-Za-z0-9_]*", |lex| lex.slice().to_string())]
	Ident(String),
	#[token(":=")]
	Assign,

	// binary operators
	#[token("+")]
	Plus,
	#[token("-")]
	Minus,
	#[token("*")]
	Asterisk,
	#[token("/")]
	Slash,

	#[token("(")]
	LParen,
	#[token(")")]
	RParen,
}

impl std::fmt::Display for Token {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		match self {
			Token::Int(n) => write!(f, "{n}"),
			Token::Float(x) => write!(f, "{x}"),
			Token::String(s) => write!(f, "\"{s}\""),
			Token::Mut => write!(f, "mut"),
			Token::Ident(name) => write!(f, "{name}"),
			Token::Assign => write!(f, ":="),
			Token::Plus => write!(f, "+"),
			Token::Minus => write!(f, "-"),
			Token::Asterisk => write!(f, "*"),
			Token::Slash => write!(f, "/"),
			Token::LParen => write!(f, "("),
			Token::RParen => write!(f, ")"),
			_ => Ok(()),
		}
	}
}

#[allow(dead_code)]
#[derive(Debug)]
enum Expr {
	Int(i32),
	Float(f64),
	String(String),
	Ident(String),

	Assign {
		mutable: bool,
		name: String,
		value: Box<Expr>,
	},

	// unary operators
	Negative(Box<Expr>),

	// binary operators
	Add(Box<Expr>, Box<Expr>),
	Sub(Box<Expr>, Box<Expr>),
	Mul(Box<Expr>, Box<Expr>),
	Div(Box<Expr>, Box<Expr>),
}

fn lex(src: &str) -> Vec<(Token, Span)> {
	let lexer = Token::lexer(src);
	let mut tokens = vec![];
	for (token, span) in lexer.spanned() {
		match token {
			Ok(t) => tokens.push((t, span)),
			Err(()) => panic!("{:?}", span),
		}
	}
	tokens
}

fn parser<'token, I>() -> impl Parser<'token, I, Vec<Expr>, extra::Err<Rich<'token, Token>>>
where
	I: ValueInput<'token, Token = Token, Span = SimpleSpan>,
{
	let expr = recursive(|expr| {
		let atom = select! {
			Token::Int(n) => Expr::Int(n),
			Token::Float(x) => Expr::Float(x),
			Token::String(s) => Expr::String(s),
			Token::Ident(name) => Expr::Ident(name),
		}
		.or(expr.delimited_by(just(Token::LParen), just(Token::RParen)));

		atom.pratt((
			prefix(3, just(Token::Minus), |_, rhs, _| {
				Expr::Negative(Box::new(rhs))
			}),
			infix(left(2), just(Token::Asterisk), |l, _, r, _| {
				Expr::Mul(Box::new(l), Box::new(r))
			}),
			infix(left(2), just(Token::Slash), |l, _, r, _| {
				Expr::Div(Box::new(l), Box::new(r))
			}),
			infix(left(1), just(Token::Plus), |l, _, r, _| {
				Expr::Add(Box::new(l), Box::new(r))
			}),
			infix(left(1), just(Token::Minus), |l, _, r, _| {
				Expr::Sub(Box::new(l), Box::new(r))
			}),
		))
	});

	let assign = just(Token::Mut)
		.or_not()
		.then(select! {
			Token::Ident(name) => name,
		})
		.then_ignore(just(Token::Assign))
		.then(expr.clone())
		.map(|((mutable, name), value)| Expr::Assign {
			mutable: mutable.is_some(),
			name,
			value: Box::new(value),
		});

	assign.or(expr).repeated().collect().then_ignore(end())
}

fn main() {
	let file = match std::env::args().nth(1) {
		Some(file) => file,
		None => "examples/hello.oi".into(),
	};
	let src = std::fs::read_to_string(file).unwrap();

	let lexed = lex(&src);
	let stream = Stream::from_iter(lexed.into_iter().map(|(t, s)| (t, s.into())))
		.map((src.len()..src.len()).into(), |(t, s)| (t, s));
	let ast = parser().parse(stream).into_result().unwrap();
	println!("{ast:#?}");
}
