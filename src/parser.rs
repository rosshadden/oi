use crate::ast::Expr;
use crate::lexer::Token;

use chumsky::{
	input::ValueInput,
	pratt::{infix, left, prefix},
	prelude::*,
};

pub fn parser<'token, I>() -> impl Parser<'token, I, Vec<Expr>, extra::Err<Rich<'token, Token>>>
where
	I: ValueInput<'token, Token = Token, Span = SimpleSpan>,
{
	let expr = recursive(|expr| {
		let atom = select! {
			Token::Bool(b) => Expr::Bool(b),
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
