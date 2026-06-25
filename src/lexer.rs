use std::fmt;

use chumsky::span::SimpleSpan;
use logos::{Lexer, Logos};

pub enum Operator {}

fn lex_block_comment(lex: &mut Lexer<Token>) {
	let src = lex.remainder().as_bytes();
	let mut depth = 1usize;
	let mut i = 0;
	while i < src.len() {
		match (src[i], src.get(i + 1).copied()) {
			(b'#', Some(b'{')) => {
				depth += 1;
				i += 2;
			}
			(b'}', Some(b'#')) => {
				i += 2;
				depth -= 1;
				if depth == 0 {
					break;
				}
			}
			_ => i += 1,
		}
	}
	lex.bump(i);
}

#[derive(Logos, Clone, PartialEq, Debug)]
#[logos(skip r"[ \t\r\n\f]+")]
pub enum Token {
	// an unrecognized lexeme, kept as a token so lexing never fails and the parser reports it
	Error(String),

	// literals
	#[regex(r"(true|false)", |lex| lex.slice().parse().ok())]
	Bool(bool),
	#[regex(r"[0-9][0-9_]*", |lex| lex.slice().replace('_', "").parse().ok())]
	#[regex(r"0[xX][0-9a-fA-F][0-9a-fA-F_]*", |lex| i64::from_str_radix(&lex.slice()[2..].replace('_', ""), 16).ok())]
	#[regex(r"0[bB][01][01_]*", |lex| i64::from_str_radix(&lex.slice()[2..].replace('_', ""), 2).ok())]
	#[regex(r"0[oO][0-7][0-7_]*", |lex| i64::from_str_radix(&lex.slice()[2..].replace('_', ""), 8).ok())]
	Int(i64),
	#[regex(r"[0-9][0-9_]*\.[0-9][0-9_]*([eE][+\-]?[0-9]+)?", |lex| Some(lex.slice().replace('_', "")))]
	#[regex(r"[0-9][0-9_]*[eE][+\-]?[0-9]+", |lex| Some(lex.slice().replace('_', "")))]
	Float(String),
	#[regex(r#""[^"]*""#, |lex| { let s = lex.slice(); s[1..s.len() - 1].to_string() })]
	String(String),
	#[regex(r":[A-Za-z0-9_]+", |lex| lex.slice()[1..].to_string())]
	Atom(String),

	// keywords
	#[token("fn")]
	Fn,
	#[token("struct")]
	Struct,
	#[token("type")]
	Type,
	#[token("return")]
	Return,
	#[token("match")]
	Match,

	// control flow
	#[token("if")]
	If,
	#[token("else")]
	Else,
	#[token("loop")]
	Loop,
	#[token("break")]
	Break,
	#[token("continue")]
	Continue,
	#[token("in")]
	In,

	// identifiers
	#[token("mut")]
	Mut,
	#[regex(r"[A-Za-z_][A-Za-z0-9_]*", |lex| lex.slice().to_string())]
	Ident(String),
	#[token(":=")]
	Bind,
	#[token("=")]
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
	#[token("%")]
	Percent,

	// comparison operators
	#[token("==")]
	Eq,
	#[token("!=")]
	Ne,
	#[token("<<")]
	LtLt,
	#[token("<")]
	Lt,
	#[token(">")]
	Gt,
	#[token("<=")]
	Le,
	#[token(">=")]
	Ge,

	// logical operators
	#[token("&&")]
	And,
	#[token("||")]
	Or,
	#[token("!")]
	Not,

	// grouping
	#[token("(")]
	LParen,
	#[token(")")]
	RParen,
	#[token("{")]
	LBrace,
	#[token("}")]
	RBrace,
	#[token("[")]
	LBracket,
	#[token("]")]
	RBracket,

	// delimiters
	#[token("..")]
	DotDot,
	#[token(".")]
	Dot,
	#[token(":")]
	Colon,
	#[token(",")]
	Comma,
	#[token(";", logos::skip)]
	Semicolon,

	// comments
	#[token("#{", lex_block_comment)]
	BlockComment,
	#[regex(r"#([^{#\r\n][^\r\n]*)?", logos::skip)]
	Comment,
	#[regex(r"##( [^\r\n]+)?", |lex| {
		let s = lex.slice();
		s.get(3..).unwrap_or("").to_owned()
	}, allow_greedy = true)]
	Doc(String),
	DocBreak,
}

impl fmt::Display for Token {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match self {
			Token::Error(s) => write!(f, "{s}"),
			Token::BlockComment | Token::Comment | Token::DocBreak => write!(f, "comment"),
			Token::Doc(_) => write!(f, "doc"),
			Token::Bool(b) => write!(f, "{b}"),
			Token::Int(n) => write!(f, "{n}"),
			Token::Float(s) => write!(f, "{s}"),
			Token::String(s) => write!(f, "\"{s}\""),
			Token::Fn => write!(f, "fn"),
			Token::Struct => write!(f, "struct"),
			Token::Type => write!(f, "type"),
			Token::Return => write!(f, "return"),
			Token::Match => write!(f, "match"),
			Token::If => write!(f, "if"),
			Token::Else => write!(f, "else"),
			Token::Loop => write!(f, "loop"),
			Token::Break => write!(f, "break"),
			Token::Continue => write!(f, "continue"),
			Token::In => write!(f, "in"),
			Token::Mut => write!(f, "mut"),
			Token::Ident(name) => write!(f, "{name}"),
			Token::Bind => write!(f, ":="),
			Token::Assign => write!(f, "="),
			Token::Atom(name) => write!(f, ":{name}"),
			Token::DotDot => write!(f, ".."),
			Token::Dot => write!(f, "."),
			Token::Colon => write!(f, ":"),
			Token::Plus => write!(f, "+"),
			Token::Minus => write!(f, "-"),
			Token::Asterisk => write!(f, "*"),
			Token::Slash => write!(f, "/"),
			Token::Percent => write!(f, "%"),
			Token::Eq => write!(f, "=="),
			Token::Ne => write!(f, "!="),
			Token::LtLt => write!(f, "<<"),
			Token::Lt => write!(f, "<"),
			Token::Gt => write!(f, ">"),
			Token::Le => write!(f, "<="),
			Token::Ge => write!(f, ">="),
			Token::And => write!(f, "&&"),
			Token::Or => write!(f, "||"),
			Token::Not => write!(f, "!"),
			Token::LParen => write!(f, "("),
			Token::RParen => write!(f, ")"),
			Token::LBrace => write!(f, "{{"),
			Token::RBrace => write!(f, "}}"),
			Token::LBracket => write!(f, "["),
			Token::RBracket => write!(f, "]"),
			Token::Comma => write!(f, ","),
			Token::Semicolon => write!(f, ";"),
		}
	}
}

// Lex `src`.
// Convert errors into tokens so parsing stays recoverable.
// Insert `DocBreak` between consecutive `Doc` tokens separated by at least one newline.
pub fn lex(src: &str) -> Vec<(Token, SimpleSpan)> {
	let raw: Vec<(Token, SimpleSpan)> = Token::lexer(src)
		.spanned()
		.filter_map(|(token, span)| match token {
			Ok(Token::BlockComment) => None,
			Ok(token) => Some((token, span.into())),
			Err(()) => Some((Token::Error(src[span.clone()].to_string()), span.into())),
		})
		.collect();

	let mut out = Vec::with_capacity(raw.len() + 4);
	for i in 0..raw.len() {
		let (tok, span) = &raw[i];
		if i > 0 {
			if let (Token::Doc(_), Token::Doc(_)) = (&raw[i - 1].0, tok) {
				let gap = &src[raw[i - 1].1.end..span.start];
				if gap.bytes().filter(|&b| b == b'\n').count() > 1 {
					out.push((Token::DocBreak, (raw[i - 1].1.end..span.start).into()));
				}
			}
		}
		out.push((tok.clone(), *span));
	}
	out
}
