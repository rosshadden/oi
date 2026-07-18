use crate::ast::{BinOp, Capture, EnumVariant, Expr, MatchArm, Param, Pattern, Spanned, TypeExpr};
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

// field/tuple/method access
enum Access {
	Fields(Vec<String>),
	Method(String, Vec<Spanned<Expr>>),
}

fn pipe_step((e, span): Spanned<Expr>) -> Spanned<Expr> {
	match e {
		Expr::Ident(name) => (
			Expr::Call {
				name,
				type_args: vec![],
				args: vec![(Expr::Dollar, span)],
			},
			span,
		),
		Expr::PropagateNone(inner) => (Expr::PropagateNone(Box::new(pipe_step(*inner))), span),
		Expr::PropagateErr(inner) => (Expr::PropagateErr(Box::new(pipe_step(*inner))), span),
		e => (e, span),
	}
}

pub fn parser<'token, I>() -> impl Parser<'token, I, Vec<Spanned<Expr>>, extra::Err<Rich<'token, Token>>>
where
	I: ValueInput<'token, Token = Token, Span = SimpleSpan>,
{
	// `expr` and the statement/block parsers are mutually recursive
	// `if` is an expression, but its branches are statement blocks.
	// Declare `expr` up front so the statement parsers can reference it before it is defined.
	let mut expr = Recursive::declare();

	let ident = || select! { Token::Ident(name) => name };

	fn paren<'token, I, O, P>(p: P) -> impl Parser<'token, I, O, extra::Err<Rich<'token, Token>>> + Clone
	where
		I: ValueInput<'token, Token = Token, Span = SimpleSpan>,
		P: Parser<'token, I, O, extra::Err<Rich<'token, Token>>> + Clone,
	{
		p.delimited_by(just(Token::LParen), just(Token::RParen))
	}
	fn brace<'token, I, O, P>(p: P) -> impl Parser<'token, I, O, extra::Err<Rich<'token, Token>>> + Clone
	where
		I: ValueInput<'token, Token = Token, Span = SimpleSpan>,
		P: Parser<'token, I, O, extra::Err<Rich<'token, Token>>> + Clone,
	{
		p.delimited_by(just(Token::LBrace), just(Token::RBrace))
	}
	fn bracket<'token, I, O, P>(p: P) -> impl Parser<'token, I, O, extra::Err<Rich<'token, Token>>> + Clone
	where
		I: ValueInput<'token, Token = Token, Span = SimpleSpan>,
		P: Parser<'token, I, O, extra::Err<Rich<'token, Token>>> + Clone,
	{
		p.delimited_by(just(Token::LBracket), just(Token::RBracket))
	}
	fn list<'token, I, O, P>(p: P) -> impl Parser<'token, I, Vec<O>, extra::Err<Rich<'token, Token>>> + Clone
	where
		I: ValueInput<'token, Token = Token, Span = SimpleSpan>,
		P: Parser<'token, I, O, extra::Err<Rich<'token, Token>>> + Clone,
	{
		p.separated_by(just(Token::Comma)).collect::<Vec<_>>()
	}
	fn loose_list<'token, I, O, P>(p: P) -> impl Parser<'token, I, Vec<O>, extra::Err<Rich<'token, Token>>> + Clone
	where
		I: ValueInput<'token, Token = Token, Span = SimpleSpan>,
		P: Parser<'token, I, O, extra::Err<Rich<'token, Token>>> + Clone,
	{
		p.separated_by(just(Token::Comma).or_not()).allow_trailing().collect::<Vec<_>>()
	}

	// type annotations
	let type_expr = recursive(|te| {
		let name = ident().map(TypeExpr::Name);
		let unit = just(Token::LParen).then(just(Token::RParen)).to(TypeExpr::Tuple(vec![]));
		let tuple = paren(
			te.clone()
				.separated_by(just(Token::Comma).or_not())
				.allow_trailing()
				.at_least(1)
				.collect::<Vec<_>>(),
		)
		.map(TypeExpr::Tuple);
		// arrays
		let array = just(Token::LBracket)
			.ignore_then(select! { Token::Int(n) => n }.or_not())
			.then_ignore(just(Token::RBracket))
			.then(te.clone())
			.map(|(n, elem)| match n {
				Some(n) => TypeExpr::FixedArray(Box::new(elem), n as usize),
				None => TypeExpr::Array(Box::new(elem)),
			});
		let fn_type = just(Token::Fn)
			.ignore_then(paren(loose_list(te.clone())))
			.then(te.clone())
			.map(|(params, ret)| TypeExpr::Fn(params, Box::new(ret)));
		// options
		let option = just(Token::Question)
			.ignore_then(te.clone())
			.map(|t| TypeExpr::Option(Box::new(t)));
		// results
		let result = just(Token::Not).ignore_then(te.clone()).map(|t| TypeExpr::Result(Box::new(t)));
		// atom(s)
		let atom = select! { Token::Atom(a) => TypeExpr::AtomSum(vec![a]) };
		// maps
		let map_type = just(Token::Ident("Map".to_string()))
			.ignore_then(bracket(te.clone().then_ignore(just(Token::Comma)).then(te.clone())))
			.map(|(k, v)| TypeExpr::Map(Box::new(k), Box::new(v)));
		unit.or(fn_type)
			.or(option)
			.or(result)
			.or(atom)
			.or(map_type)
			.or(name)
			.or(tuple)
			.or(array)
	});

	// param type is kept for the compiler to resolve
	// NOTE: a bare `self` receiver gets the type `Self`
	let param = just(Token::Mut)
		.or_not()
		.then(ident())
		.then(type_expr.clone().or_not())
		.map_with(|((mutable, name), typ), ex| Param {
			typ: typ.unwrap_or(TypeExpr::Name("Self".into())),
			name,
			span: ex.span(),
			default: None,
			mutable: mutable.is_some(),
		});
	// NOTE: a trailing comma forces a tuple even for one param
	let params = paren(
		param
			.separated_by(just(Token::Comma))
			.collect::<Vec<_>>()
			.then(just(Token::Comma).or_not()),
	)
	.map(|(params, trailing)| {
		let tuple = params.len() != 1 || trailing.is_some();
		(params, tuple)
	});

	// optional return type annotation
	let ret = type_expr.clone().map_with(|t, ex| (t, ex.span())).or_not();

	// generics
	let type_params = bracket(list(ident())).or_not().map(Option::unwrap_or_default);

	// bindings
	let annot = type_expr.clone().map_with(|t, ex| (t, ex.span()));
	let bind = just(Token::Mut)
		.or_not()
		.then(ident())
		.then(annot.clone().or_not())
		.then(just(Token::Bind).ignore_then(expr.clone()).or_not())
		.try_map(|(((mutable, name), typ), value), span| {
			if value.is_none() && (typ.is_none() || mutable.is_none()) {
				return Err(Rich::custom(span, "expected `:=` value, or `mut name type`"));
			}
			Ok(Expr::Bind {
				mutable: mutable.is_some(),
				name,
				typ,
				value: value.map(Box::new),
			})
		})
		.map_with(|e, ex| (e, ex.span()));

	// assignment
	let assign = ident()
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

	// index assignment
	let index_assign = ident()
		.then(bracket(expr.clone()))
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
	let append = ident()
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

	// map deletion
	let map_delete = ident()
		.then_ignore(just(Token::Dot))
		.then_ignore(just(Token::Ident("delete".to_string())))
		.then(bracket(expr.clone()))
		.map_with(|(name, key), ex| {
			(
				Expr::MapDelete {
					name,
					key: Box::new(key),
				},
				ex.span(),
			)
		});

	// field assignment
	let field_assign = ident()
		.then_ignore(just(Token::Dot))
		.then(ident())
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
		.or(map_delete)
		.or(append)
		.or(expr.clone());

	// blocks
	let block = brace(stmt.clone().repeated().collect::<Vec<_>>());

	let definition = {
		let literal = select! {
			Token::Bool(b) => Expr::Bool(b),
			Token::Int(n) => Expr::Int(n),
			Token::Float(s) => Expr::Float(s.parse().unwrap()),
			Token::String(s) => Expr::String(s),
			Token::Atom(name) => Expr::Atom(name),
			Token::Dollar => Expr::Dollar,
			Token::None => Expr::None,
		};

		// variable vs. call vs. struct literal
		let args = paren(
			expr.clone()
				.separated_by(just(Token::Comma))
				.allow_trailing()
				.collect::<Vec<_>>(),
		);

		// named or positional field entry
		let struct_field_entry = ident().then_ignore(just(Token::Colon)).or_not().then(expr.clone());
		let struct_body = brace(loose_list(struct_field_entry.clone()));

		// pull out struct literals separately (they have title case names) from vars/calls/whatever below
		let struct_lit = ident()
			.filter(|name| name.starts_with(char::is_uppercase))
			.then(struct_body)
			.map(|(name, fields)| Expr::StructLit { name, fields });

		// explicit generic types
		let call_type_args = bracket(
			type_expr
				.clone()
				.map_with(|t, ex| (t, ex.span()))
				.separated_by(just(Token::Comma))
				.at_least(1)
				.collect::<Vec<_>>(),
		);

		let var_or_call = ident()
			.then(call_type_args.or_not().then(args.clone()).or_not())
			.map(|(name, call)| match call {
				Some((type_args, args)) => Expr::Call {
					name,
					type_args: type_args.unwrap_or_default(),
					args,
				},
				None => Expr::Ident(name),
			});

		// leaf atoms pair themselves with their span
		let leaf = literal.or(struct_lit).or(var_or_call).map_with(|e, ex| (e, ex.span()));

		// enum shorthand
		let enum_shorthand = just(Token::Dot)
			.ignore_then(select! { Token::Ident(v) => v, Token::None => "none".to_string() })
			.then(args.clone().or_not())
			.map_with(|(variant, args), ex| {
				let args = args.unwrap_or_default();
				(Expr::EnumShorthand { variant, args }, ex.span())
			});

		// a lexer error token
		let bad = select! { Token::Error(text) => text }
			.try_map(|text, span| Err(Rich::custom(span, format!("unexpected character `{text}`"))));

		// grouping before tuple rule to avoid making 1ples, which are instead made with `(expr,)`
		let group = paren(expr.clone());

		// tuple literal
		let tuple = paren(loose_list(struct_field_entry)).map_with(|elems, ex| (Expr::Tuple(elems), ex.span()));

		// map literal
		let type_init = type_expr
			.clone()
			.filter(|t| matches!(t, TypeExpr::Array(_) | TypeExpr::FixedArray(..) | TypeExpr::Map(..)))
			.then_ignore(just(Token::LBrace))
			.then_ignore(just(Token::RBrace))
			.map_with(|te, ex| (Expr::TypeInit((te, ex.span())), ex.span()));

		let option_init = just(Token::Question)
			.ignore_then(type_expr.clone())
			.then(paren(expr.clone()))
			.map_with(|(elem, arg), ex| {
				(
					Expr::OptionInit {
						inner: (elem, ex.span()),
						arg: Box::new(arg),
					},
					ex.span(),
				)
			});

		// result literal
		let result_shape = type_expr.clone().then(paren(expr.clone()));
		let result_init = just(Token::Not).ignore_then(result_shape.clone()).map_with(|(elem, arg), ex| {
			(
				Expr::ResultInit {
					inner: (elem, ex.span()),
					arg: Box::new(arg),
				},
				ex.span(),
			)
		});

		// array literal
		let array = bracket(loose_list(expr.clone())).map_with(|elems, ex| (Expr::Array(elems), ex.span()));

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
			let name = ident();
			let tuple =
				paren(name.separated_by(just(Token::Comma)).allow_trailing().collect::<Vec<_>>()).map(Pattern::Tuple);
			tuple.or(name.map(Pattern::Name))
		};
		let for_expr = just(Token::Loop)
			.ignore_then(pattern)
			.then_ignore(just(Token::In))
			.then(expr.clone().map(Box::new))
			.then(block.clone())
			.map_with(|((pat, iter), body), ex| (Expr::For { pat, iter, body }, ex.span()));
		let break_expr = just(Token::Break).map_with(|_, ex| (Expr::Break, ex.span()));
		let continue_expr = just(Token::Continue).map_with(|_, ex| (Expr::Continue, ex.span()));

		// match expression
		let binding = ident().then_ignore(just(Token::At)).or_not();
		let match_arm = binding
			.then(
				expr.clone()
					.separated_by(just(Token::Comma))
					.allow_trailing()
					.at_least(1)
					.collect::<Vec<_>>(),
			)
			.then(block.clone())
			.map(|((binding, patterns), body)| MatchArm {
				binding,
				patterns,
				body,
			});
		let match_expr = just(Token::Match)
			.ignore_then(expr.clone())
			.then(brace(
				match_arm
					.repeated()
					.collect::<Vec<_>>()
					.then(just(Token::Else).ignore_then(block.clone()).or_not()),
			))
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

		// anonymous functions
		let capture = ident();
		let capture = just(Token::Move)
			.ignore_then(capture)
			.map(Capture::Move)
			.or(just(Token::Mut).ignore_then(capture).map(Capture::Mut))
			.or(capture.map(Capture::ReadOnly));
		let captures = bracket(capture.separated_by(just(Token::Comma)).allow_trailing().collect::<Vec<_>>());
		let anon_fn = just(Token::Fn)
			.ignore_then(captures.or_not())
			.then(params.clone().or_not())
			.then(ret.clone())
			.then(block.clone())
			.map_with(|(((captures, params), ret), body), ex| {
				let (params, tuple) = params.unwrap_or((vec![], true));
				(
					Expr::AnonFn {
						captures,
						params,
						params_tuple: tuple,
						ret,
						body,
					},
					ex.span(),
				)
			});

		// atoms
		let atom = type_init
			.or(leaf)
			.or(enum_shorthand)
			.or(group)
			.or(tuple)
			.or(option_init)
			.or(result_init)
			.or(array)
			.or(if_expr)
			.or(match_expr)
			.or(for_expr)
			.or(loop_expr)
			.or(break_expr)
			.or(continue_expr)
			.or(anon_fn)
			.or(bad);

		// field/tuple/method access
		let access = choice((
			select! { Token::Int(n) => Access::Fields(vec![n.to_string()]) },
			// NOTE: chained tuple access like `x.0.1` lexes `0.1` as a float, hence the split
			select! { Token::Float(s) => Access::Fields(s.split('.').map(String::from).collect()) },
			ident().then(args.clone().or_not()).map(|(name, call)| match call {
				Some(args) => Access::Method(name, args),
				None => Access::Fields(vec![name]),
			}),
		));

		// array subscripts
		let no_start_range = just(Token::DotDot)
			.ignore_then(expr.clone().or_not())
			.map(|end| Subscript::Slice(None, end));
		let with_start = expr
			.clone()
			.then(just(Token::DotDot).ignore_then(expr.clone().or_not()).or_not())
			.map(|(e, extra)| match (e, extra) {
				// closed range
				((Expr::Range { start, end }, _), None) => Subscript::Slice(start.map(|s| *s), end.map(|e| *e)),
				// open range
				(e, Some(end)) => Subscript::Slice(Some(e), end),
				// numeric index
				(e, None) => Subscript::Index(e),
			});
		let subscript = bracket(no_start_range.or(with_start));

		// infix operator builder
		let binop = |prec, tok: Token, op: BinOp| {
			infix(left(prec), just(tok), move |l, _, r, ex| {
				(Expr::Binary(op, Box::new(l), Box::new(r)), ex.span())
			})
		};

		let core = atom.pratt((
			// field/tuple/method access
			postfix(9, just(Token::Dot).ignore_then(access), |lhs, acc, ex| match acc {
				Access::Fields(parts) => parts.into_iter().fold(lhs, |cur, field| {
					(
						Expr::Field {
							tuple: Box::new(cur),
							field,
						},
						ex.span(),
					)
				}),
				Access::Method(method, args) => (
					Expr::MethodCall {
						recv: Box::new(lhs),
						method,
						args,
					},
					ex.span(),
				),
			}),
			// indexing and slicing
			postfix(9, subscript, |lhs, sub, ex| {
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
			// propagators
			postfix(9, just(Token::Question), |lhs, _, ex| {
				(Expr::PropagateNone(Box::new(lhs)), ex.span())
			}),
			postfix(9, just(Token::Not), |lhs, _, ex| {
				(Expr::PropagateErr(Box::new(lhs)), ex.span())
			}),
			// unary
			prefix(8, just(Token::Minus), |_, rhs, ex| {
				(Expr::Negative(Box::new(rhs)), ex.span())
			}),
			prefix(
				8,
				just(Token::Not).then_ignore(result_shape.clone().not()),
				|_, rhs, ex| (Expr::Not(Box::new(rhs)), ex.span()),
			),
			// arithmetic
			binop(7, Token::Asterisk, BinOp::Mul),
			binop(7, Token::Slash, BinOp::Div),
			binop(7, Token::Percent, BinOp::Mod),
			binop(6, Token::Plus, BinOp::Add),
			binop(6, Token::Minus, BinOp::Sub),
			// relational
			binop(5, Token::Lt, BinOp::Lt),
			binop(5, Token::Gt, BinOp::Gt),
			binop(5, Token::Le, BinOp::Le),
			binop(5, Token::Ge, BinOp::Ge),
			// equality | membership
			binop(4, Token::Eq, BinOp::Eq),
			binop(4, Token::Ne, BinOp::Ne),
			binop(4, Token::In, BinOp::In),
			// logical
			binop(3, Token::AndAnd, BinOp::And),
			binop(2, Token::OrOr, BinOp::Or),
			// ranges
			infix(left(1), just(Token::DotDot), |l, _, r, ex| {
				(
					Expr::Range {
						start: Some(Box::new(l)),
						end: Some(Box::new(r)),
					},
					ex.span(),
				)
			}),
			// pipelines
			infix(left(0), just(Token::Pipeline), |l, _, r, ex| {
				(
					Expr::Pipe {
						value: Box::new(l),
						step: Box::new(pipe_step(r)),
					},
					ex.span(),
				)
			}),
		));

		// or blocks
		let or_tail = just(Token::Or).ignore_then(block.clone().or(core.clone().map(|e| vec![pipe_step(e)])));
		core.then(or_tail.or_not()).map_with(|(value, body), ex| match body {
			Some(body) => (
				Expr::OrElse {
					value: Box::new(value),
					body,
				},
				ex.span(),
			),
			None => value,
		})
	};
	expr.define(definition);

	// fn defs
	let func = just(Token::Fn)
		.ignore_then(ident())
		.then(type_params)
		.then(params)
		.then(ret)
		.then(block.clone())
		.map_with(|((((name, type_params), (params, tuple)), ret), body), ex| {
			(
				Expr::Fn {
					name,
					type_params,
					params,
					params_tuple: tuple,
					ret,
					body,
				},
				ex.span(),
			)
		});

	// struct defs
	let struct_field = ident()
		.then(type_expr.clone())
		.then(just(Token::Assign).ignore_then(expr.clone()).or_not())
		.map_with(|((name, typ), default), ex| Param {
			name,
			typ,
			span: ex.span(),
			default,
			mutable: false,
		});
	let struct_def = just(Token::Struct)
		.ignore_then(ident())
		.then(brace(loose_list(struct_field)))
		.map_with(|(name, fields), ex| (Expr::StructDef { name, fields }, ex.span()));

	// enum defs
	let disc = just(Token::Assign)
		.ignore_then(just(Token::Minus).or_not())
		.then(select! { Token::Int(n) => n })
		.map(|(neg, n)| if neg.is_some() { -n } else { n });
	let payload = paren(annot.separated_by(just(Token::Comma)).allow_trailing().collect::<Vec<_>>());
	let variant = ident()
		.then(payload.or_not())
		.then(disc.or_not())
		.map(|((name, payload), disc)| EnumVariant {
			name,
			disc,
			payload: payload.unwrap_or_default(),
		});
	let enum_def = just(Token::Enum)
		.ignore_then(ident())
		.then(brace(loose_list(variant)))
		.try_map_with(|(name, variants), ex| {
			let mut next = 0;
			let mut seen = Vec::new();
			for v in &variants {
				let d = v.disc.unwrap_or(next);
				if seen.contains(&d) {
					let msg = format!("discriminant value `{d}` assigned more than once");
					return Err(Rich::custom(ex.span(), msg));
				}
				seen.push(d);
				next = d + 1;
			}
			Ok((Expr::EnumDef { name, variants }, ex.span()))
		});

	// type aliases
	let atom_sum = select! { Token::Atom(a) => a }
		.separated_by(just(Token::Pipe))
		.at_least(1)
		.collect::<Vec<_>>()
		.map(TypeExpr::AtomSum);
	let type_alias = just(Token::Type)
		.ignore_then(ident())
		.then_ignore(just(Token::Assign))
		.then(atom_sum.or(type_expr))
		.map_with(|(name, typ), ex| (Expr::TypeAlias { name, typ }, ex.span()));

	let impl_block = just(Token::Impl)
		.ignore_then(ident())
		.then(brace(func.clone().repeated().collect::<Vec<_>>()))
		.map_with(|(typ, methods), ex| (Expr::Impl { typ, methods }, ex.span()));

	struct_def
		.or(enum_def)
		.or(type_alias)
		.or(func)
		.or(impl_block)
		.or(stmt)
		.repeated()
		.collect()
		.then_ignore(end())
}
