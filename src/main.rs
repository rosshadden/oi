mod ast;
mod compiler;
mod lexer;
mod parser;
mod runtime;

use crate::lexer::*;
use crate::parser::*;

use chumsky::{input::Stream, prelude::*};
use logos::{Logos, Span};

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

	let mut compiler = compiler::Compiler::default();
	let code = compiler.compile(&ast).unwrap();
	let f = unsafe { std::mem::transmute::<*const u8, fn()>(code) };
	f();
}
