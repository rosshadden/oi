use chumsky::{
	input::{Stream, ValueInput},
	prelude::*,
};
use logos::{Logos, Span};

#[derive(Logos, Clone, PartialEq, Debug)]
#[logos(skip r"[ \t\r\n\f]+")]
enum Token {
	#[regex(r"-?[0-9]+")]
	Int,

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

fn parser<'token, I>() -> impl Parser<'token, I, Expr, extra::Err<Rich<'token, Token>>>
where
	I: ValueInput<'token, Token = Token, Span = SimpleSpan>,
{
	recursive(|expr| {
		let atom = select! { Token::Int => Expr::Int }
			.or(expr.delimited_by(just(Token::LParen), just(Token::RParen)));

		// fold a chain of operators into a left-leaning tree
		// TODO: PEMDAS
		atom.clone().foldl(
			choice((
				just(Token::Plus).to(Expr::Add as fn(_, _) -> _),
				just(Token::Minus).to(Expr::Sub as fn(_, _) -> _),
				just(Token::Asterisk).to(Expr::Mul as fn(_, _) -> _),
				just(Token::Slash).to(Expr::Div as fn(_, _) -> _),
			))
			.then(atom)
			.repeated(),
			|lhs, (op, rhs)| op(Box::new(lhs), Box::new(rhs)),
		)
	})
	.then_ignore(end())
}

fn main() {
	let file = match std::env::args().nth(1) {
		Some(file) => file,
		None => "examples/hello.oi".into(),
	};
	let src = std::fs::read_to_string(file).unwrap();

	let lexed = lex(&src);
	println!("{:?}", lexed);

	let stream = Stream::from_iter(lexed.into_iter().map(|(t, s)| (t, s.into())))
		.map((src.len()..src.len()).into(), |(t, s)| (t, s));
	let ast = parser().parse(stream).into_result().unwrap();
	println!("{ast:#?}");
}
