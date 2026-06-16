use crate::ast::{Expr, Spanned};
use crate::lexer::Token;

use chumsky::{
	input::ValueInput,
	pratt::{infix, left, prefix},
	prelude::*,
};

pub fn parser<'token, I>()
-> impl Parser<'token, I, Vec<Spanned<Expr>>, extra::Err<Rich<'token, Token>>>
where
	I: ValueInput<'token, Token = Token, Span = SimpleSpan>,
{
	let expr = recursive(|expr| {
		let literal = select! {
			Token::Bool(b) => Expr::Bool(b),
			Token::Int(n) => Expr::Int(n),
			Token::Float(x) => Expr::Float(x),
			Token::String(s) => Expr::String(s),
		};

		// `name` (variable) vs. `name()` (call)
		let var_or_call = select! { Token::Ident(name) => name }
			.then(
				just(Token::LParen)
					.ignore_then(just(Token::RParen))
					.or_not(),
			)
			.map(|(name, call)| match call {
				Some(_) => Expr::Call(name),
				None => Expr::Ident(name),
			});

		// leaf atoms pair themselves with their span
		let leaf = literal.or(var_or_call).map_with(|e, ex| (e, ex.span()));

		// a lexer error token
		let bad = select! { Token::Error(text) => text }.try_map(|text, span| {
			Err(Rich::custom(span, format!("unexpected character `{text}`")))
		});

		let atom = leaf
			.or(expr.delimited_by(just(Token::LParen), just(Token::RParen)))
			.or(bad);

		atom.pratt((
			prefix(3, just(Token::Minus), |_, rhs, ex| {
				(Expr::Negative(Box::new(rhs)), ex.span())
			}),
			infix(left(2), just(Token::Asterisk), |l, _, r, ex| {
				(Expr::Mul(Box::new(l), Box::new(r)), ex.span())
			}),
			infix(left(2), just(Token::Slash), |l, _, r, ex| {
				(Expr::Div(Box::new(l), Box::new(r)), ex.span())
			}),
			infix(left(1), just(Token::Plus), |l, _, r, ex| {
				(Expr::Add(Box::new(l), Box::new(r)), ex.span())
			}),
			infix(left(1), just(Token::Minus), |l, _, r, ex| {
				(Expr::Sub(Box::new(l), Box::new(r)), ex.span())
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
		.map_with(|((mutable, name), value), ex| {
			(
				Expr::Assign {
					mutable: mutable.is_some(),
					name,
					value: Box::new(value),
				},
				ex.span(),
			)
		});

	// a statement is an assignment or a bare expression
	let stmt = assign.or(expr);

	// `fn name() { ... }`
	let func = just(Token::Fn)
		.ignore_then(select! { Token::Ident(name) => name })
		.then_ignore(just(Token::LParen))
		.then_ignore(just(Token::RParen))
		.then(
			stmt.clone()
				.repeated()
				.collect::<Vec<_>>()
				.delimited_by(just(Token::LBrace), just(Token::RBrace)),
		)
		.map_with(|(name, body), ex| (Expr::Fn { name, body }, ex.span()));

	func.or(stmt).repeated().collect().then_ignore(end())
}
