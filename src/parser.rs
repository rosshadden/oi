use crate::ast::{Expr, ForIter, MatchArm, Param, Pattern, Spanned, TypeExpr};
use crate::lexer::Token;

use chumsky::{
	input::ValueInput,
	pratt::{infix, left, postfix, prefix},
	prelude::*,
};

// The contents of a `[...]` subscript. A single index, or a range to slice.
enum Subscript {
	Index(Spanned<Expr>),
	Slice(Option<Spanned<Expr>>, Option<Spanned<Expr>>),
}

pub fn parser<'token, I>()
-> impl Parser<'token, I, Vec<Spanned<Expr>>, extra::Err<Rich<'token, Token>>>
where
	I: ValueInput<'token, Token = Token, Span = SimpleSpan>,
{
	// `expr` and the statement/block parsers are mutually recursive
	// `if` is an expression, but its branches are statement blocks.
	// Declare `expr` up front so the statement parsers can reference it before it is defined.
	let mut expr = Recursive::declare();

	// bindings
	let bind = just(Token::Mut)
		.or_not()
		.then(select! {
			Token::Ident(name) => name,
		})
		.then_ignore(just(Token::Bind))
		.then(expr.clone())
		.map_with(|((mutable, name), value), ex| {
			(
				Expr::Bind {
					mutable: mutable.is_some(),
					name,
					value: Box::new(value),
				},
				ex.span(),
			)
		});

	// assignment
	let assign = select! { Token::Ident(name) => name }
		.then_ignore(just(Token::Assign))
		.then(expr.clone())
		.map_with(|(name, value), ex| {
			(
				Expr::Assign {
					name,
					value: Box::new(value),
				},
				ex.span(),
			)
		});

	// return statements
	let ret_stmt = just(Token::Return)
		.ignore_then(expr.clone().or_not())
		.map_with(|value, ex| (Expr::Return(value.map(Box::new)), ex.span()));

	// `name[index] = value`
	let index_assign = select! { Token::Ident(name) => name }
		.then(
			expr.clone()
				.delimited_by(just(Token::LBracket), just(Token::RBracket)),
		)
		.then_ignore(just(Token::Assign))
		.then(expr.clone())
		.map_with(|((name, index), value), ex| {
			(
				Expr::IndexAssign {
					name,
					index: Box::new(index),
					value: Box::new(value),
				},
				ex.span(),
			)
		});

	// array appending
	let append = select! { Token::Ident(name) => name }
		.then_ignore(just(Token::LtLt))
		.then(expr.clone())
		.map_with(|(name, value), ex| {
			(
				Expr::Append {
					name,
					value: Box::new(value),
				},
				ex.span(),
			)
		});

	// `name.field = value`
	let field_assign = select! { Token::Ident(name) => name }
		.then_ignore(just(Token::Dot))
		.then(select! { Token::Ident(field) => field })
		.then_ignore(just(Token::Assign))
		.then(expr.clone())
		.map_with(|((name, field), value), ex| {
			(
				Expr::FieldAssign {
					name,
					field,
					value: Box::new(value),
				},
				ex.span(),
			)
		});

	let doc = select! { Token::Doc(text) => text }
		.repeated()
		.at_least(1)
		.collect::<Vec<_>>()
		.map_with(|lines, ex| (Expr::Doc(lines), ex.span()))
		.then_ignore(just(Token::DocBreak).or_not());

	// statements
	let stmt = doc
		.or(ret_stmt)
		.or(bind)
		.or(field_assign)
		.or(assign)
		.or(index_assign)
		.or(append)
		.or(expr.clone());

	// blocks
	let block = stmt
		.clone()
		.repeated()
		.collect::<Vec<_>>()
		.delimited_by(just(Token::LBrace), just(Token::RBrace));

	let definition = {
		let literal = select! {
			Token::Bool(b) => Expr::Bool(b),
			Token::Int(n) => Expr::Int(n),
			Token::Float(s) => Expr::Float(s.parse().unwrap()),
			Token::String(s) => Expr::String(s),
		};

		// variable vs. call vs. struct literal
		let args = expr
			.clone()
			.separated_by(just(Token::Comma))
			.allow_trailing()
			.collect::<Vec<_>>()
			.delimited_by(just(Token::LParen), just(Token::RParen));

		// `(name:)? expr`
		// named or positional field entry
		let struct_field_entry = select! { Token::Ident(name) => name }
			.then_ignore(just(Token::Colon))
			.or_not()
			.then(expr.clone());
		let struct_body = struct_field_entry
			.separated_by(just(Token::Comma).or_not())
			.allow_trailing()
			.collect::<Vec<_>>()
			.delimited_by(just(Token::LBrace), just(Token::RBrace));

		enum VarSuffix {
			Call(Vec<Spanned<Expr>>),
			Lit(Vec<(Option<String>, Spanned<Expr>)>),
		}
		let var_or_call = select! { Token::Ident(name) => name }
			.then(
				args.map(VarSuffix::Call)
					.or(struct_body.map(VarSuffix::Lit))
					.or_not(),
			)
			.map(|(name, suffix)| match suffix {
				Some(VarSuffix::Call(args)) => Expr::Call { name, args },
				Some(VarSuffix::Lit(fields)) => Expr::StructLit { name, fields },
				None => Expr::Ident(name),
			});

		// leaf atoms pair themselves with their span
		let leaf = literal.or(var_or_call).map_with(|e, ex| (e, ex.span()));

		// a lexer error token
		let bad = select! { Token::Error(text) => text }.try_map(|text, span| {
			Err(Rich::custom(span, format!("unexpected character `{text}`")))
		});

		// grouping before tuple rule to avoid making 1ples, which are instead made with `(expr,)`
		let group = expr
			.clone()
			.delimited_by(just(Token::LParen), just(Token::RParen));

		let elem = select! { Token::Ident(name) => name }
			.then_ignore(just(Token::Colon))
			.or_not()
			.then(expr.clone());
		// tuple literal
		let tuple = elem
			.separated_by(just(Token::Comma).or_not())
			.allow_trailing()
			.collect::<Vec<_>>()
			.delimited_by(just(Token::LParen), just(Token::RParen))
			.map_with(|elems, ex| (Expr::Tuple(elems), ex.span()));

		// array literal
		let array = expr
			.clone()
			.separated_by(just(Token::Comma).or_not())
			.allow_trailing()
			.collect::<Vec<_>>()
			.delimited_by(just(Token::LBracket), just(Token::RBracket))
			.map_with(|elems, ex| (Expr::Array(elems), ex.span()));

		let if_expr = recursive(|if_expr| {
			just(Token::If)
				.ignore_then(expr.clone())
				.then(block.clone())
				.then(
					just(Token::Else)
						.ignore_then(if_expr.map(|e| vec![e]).or(block.clone()))
						.or_not(),
				)
				.map_with(|((cond, then), els), ex| {
					(
						Expr::If {
							cond: Box::new(cond),
							then,
							els,
						},
						ex.span(),
					)
				})
		});

		// loops
		// `loop {}`: infinite loop
		// `loop <condition> {}`: while loop
		// `loop <pattern> in <iter> {}`: for loop
		let loop_expr = just(Token::Loop)
			.ignore_then(expr.clone().or_not())
			.then(block.clone())
			.map_with(|(cond, body), ex| {
				(
					Expr::Loop {
						cond: cond.map(Box::new),
						body,
					},
					ex.span(),
				)
			});

		// a for-loop binds/destructures into names
		let pattern = {
			let name = select! { Token::Ident(name) => name };
			let tuple = name
				.separated_by(just(Token::Comma))
				.allow_trailing()
				.collect::<Vec<_>>()
				.delimited_by(just(Token::LParen), just(Token::RParen))
				.map(Pattern::Tuple);
			tuple.or(name.map(Pattern::Name))
		};
		let for_iter = expr
			.clone()
			.then(just(Token::DotDot).ignore_then(expr.clone()).or_not())
			.map(|(start, end)| match end {
				Some(end) => ForIter::Range(Box::new(start), Box::new(end)),
				None => ForIter::Iter(Box::new(start)),
			});
		let for_expr = just(Token::Loop)
			.ignore_then(pattern)
			.then_ignore(just(Token::In))
			.then(for_iter)
			.then(block.clone())
			.map_with(|((pat, iter), body), ex| (Expr::For { pat, iter, body }, ex.span()));
		let break_expr = just(Token::Break).map_with(|_, ex| (Expr::Break, ex.span()));
		let continue_expr = just(Token::Continue).map_with(|_, ex| (Expr::Continue, ex.span()));

		// match expression
		let match_arm = expr
			.clone()
			.separated_by(just(Token::Comma))
			.allow_trailing()
			.at_least(1)
			.collect::<Vec<_>>()
			.then(block.clone())
			.map(|(patterns, body)| MatchArm { patterns, body });
		let match_expr = just(Token::Match)
			.ignore_then(expr.clone())
			.then(
				match_arm
					.repeated()
					.collect::<Vec<_>>()
					.then(just(Token::Else).ignore_then(block.clone()).or_not())
					.delimited_by(just(Token::LBrace), just(Token::RBrace)),
			)
			.map_with(|(subject, (arms, else_body)), ex| {
				(
					Expr::Match {
						subject: Box::new(subject),
						arms,
						else_body,
					},
					ex.span(),
				)
			});

		// atoms
		let atom = leaf
			.or(group)
			.or(tuple)
			.or(array)
			.or(if_expr)
			.or(match_expr)
			.or(for_expr)
			.or(loop_expr)
			.or(break_expr)
			.or(continue_expr)
			.or(bad);

		let field_access = select! {
			Token::Int(n) => vec![n.to_string()],
			Token::Ident(name) => vec![name],
			// split at `.` and fold left, like rustc does
			Token::Float(s) => s.split('.').map(String::from).collect(),
		};

		// array subscripts
		let range = expr
			.clone()
			.or_not()
			.then_ignore(just(Token::DotDot))
			.then(expr.clone().or_not())
			.map(|(start, end)| Subscript::Slice(start, end));
		let subscript = range
			.or(expr.clone().map(Subscript::Index))
			.delimited_by(just(Token::LBracket), just(Token::RBracket));

		atom.pratt((
			// fields
			postfix(
				8,
				just(Token::Dot).ignore_then(field_access),
				|lhs, parts, ex| {
					let mut cur = lhs;
					for field in parts {
						cur = (
							Expr::Field {
								tuple: Box::new(cur),
								field,
							},
							ex.span(),
						);
					}
					cur
				},
			),
			// indexing and slicing
			postfix(8, subscript, |lhs, sub, ex| {
				let collection = Box::new(lhs);
				let e = match sub {
					Subscript::Index(index) => Expr::Index {
						collection,
						index: Box::new(index),
					},
					Subscript::Slice(start, end) => Expr::Slice {
						collection,
						start: start.map(Box::new),
						end: end.map(Box::new),
					},
				};
				(e, ex.span())
			}),
			// unary
			prefix(7, just(Token::Minus), |_, rhs, ex| {
				(Expr::Negative(Box::new(rhs)), ex.span())
			}),
			prefix(7, just(Token::Not), |_, rhs, ex| {
				(Expr::Not(Box::new(rhs)), ex.span())
			}),
			// arithmetic
			infix(left(6), just(Token::Asterisk), |l, _, r, ex| {
				(Expr::Mul(Box::new(l), Box::new(r)), ex.span())
			}),
			infix(left(6), just(Token::Slash), |l, _, r, ex| {
				(Expr::Div(Box::new(l), Box::new(r)), ex.span())
			}),
			infix(left(6), just(Token::Percent), |l, _, r, ex| {
				(Expr::Mod(Box::new(l), Box::new(r)), ex.span())
			}),
			// arithmetic
			infix(left(5), just(Token::Plus), |l, _, r, ex| {
				(Expr::Add(Box::new(l), Box::new(r)), ex.span())
			}),
			infix(left(5), just(Token::Minus), |l, _, r, ex| {
				(Expr::Sub(Box::new(l), Box::new(r)), ex.span())
			}),
			// relational
			infix(left(4), just(Token::Lt), |l, _, r, ex| {
				(Expr::Lt(Box::new(l), Box::new(r)), ex.span())
			}),
			infix(left(4), just(Token::Gt), |l, _, r, ex| {
				(Expr::Gt(Box::new(l), Box::new(r)), ex.span())
			}),
			infix(left(4), just(Token::Le), |l, _, r, ex| {
				(Expr::Le(Box::new(l), Box::new(r)), ex.span())
			}),
			infix(left(4), just(Token::Ge), |l, _, r, ex| {
				(Expr::Ge(Box::new(l), Box::new(r)), ex.span())
			}),
			// equality
			infix(left(3), just(Token::Eq), |l, _, r, ex| {
				(Expr::Eq(Box::new(l), Box::new(r)), ex.span())
			}),
			infix(left(3), just(Token::Ne), |l, _, r, ex| {
				(Expr::Ne(Box::new(l), Box::new(r)), ex.span())
			}),
			// membership
			infix(left(3), just(Token::In), |l, _, r, ex| {
				(Expr::In(Box::new(l), Box::new(r)), ex.span())
			}),
			// logical
			infix(left(2), just(Token::And), |l, _, r, ex| {
				(Expr::And(Box::new(l), Box::new(r)), ex.span())
			}),
			infix(left(1), just(Token::Or), |l, _, r, ex| {
				(Expr::Or(Box::new(l), Box::new(r)), ex.span())
			}),
		))
	};
	expr.define(definition);

	// param type is kept for the compiler to resolve
	let param = select! { Token::Ident(name) => name }
		.then(select! { Token::Ident(typ) => typ })
		.map_with(|(name, typ), ex| Param {
			name,
			typ,
			span: ex.span(),
			default: None,
		});
	let params = param
		.separated_by(just(Token::Comma))
		.allow_trailing()
		.collect::<Vec<_>>()
		.delimited_by(just(Token::LParen), just(Token::RParen));

	// optional return type annotation
	let type_expr = recursive(|te| {
		let name = select! { Token::Ident(t) => TypeExpr::Name(t) };
		let unit = just(Token::LParen)
			.then(just(Token::RParen))
			.to(TypeExpr::Tuple(vec![]));
		let tuple = te
			.clone()
			.separated_by(just(Token::Comma).or_not())
			.allow_trailing()
			.at_least(1)
			.collect::<Vec<_>>()
			.delimited_by(just(Token::LParen), just(Token::RParen))
			.map(TypeExpr::Tuple);
		let array = te
			.delimited_by(just(Token::LBracket), just(Token::RBracket))
			.map(|elem| TypeExpr::Array(Box::new(elem)));
		unit.or(name).or(tuple).or(array)
	});
	let ret = type_expr.map_with(|t, ex| (t, ex.span())).or_not();

	// `fn name(params) ret? { ... }`
	let func = just(Token::Fn)
		.ignore_then(select! { Token::Ident(name) => name })
		.then(params)
		.then(ret)
		.then(block.clone())
		.map_with(|(((name, params), ret), body), ex| {
			(
				Expr::Fn {
					name,
					params,
					ret,
					body,
				},
				ex.span(),
			)
		});

	// `struct Name { name type [= default], ... }`
	let struct_field = select! { Token::Ident(name) => name }
		.then(select! { Token::Ident(typ) => typ })
		.then(just(Token::Assign).ignore_then(expr.clone()).or_not())
		.map_with(|((name, typ), default), ex| Param {
			name,
			typ,
			span: ex.span(),
			default,
		});
	let struct_def = just(Token::Struct)
		.ignore_then(select! { Token::Ident(name) => name })
		.then(
			struct_field
				.separated_by(just(Token::Comma).or_not())
				.allow_trailing()
				.collect::<Vec<_>>()
				.delimited_by(just(Token::LBrace), just(Token::RBrace)),
		)
		.map_with(|(name, fields), ex| (Expr::StructDef { name, fields }, ex.span()));

	struct_def
		.or(func)
		.or(stmt)
		.repeated()
		.collect()
		.then_ignore(end())
}
