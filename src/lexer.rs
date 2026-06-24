use std::fmt;

use chumsky::span::SimpleSpan;
use logos::Logos;

pub enum Operator {}

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
	#[regex(r"[0-9][0-9_]*\.[0-9][0-9_]*", |lex| Some(lex.slice().replace('_', "")))]
	Float(String),
	#[regex(r#""[^"]*""#, |lex| { let s = lex.slice(); s[1..s.len() - 1].to_string() })]
	String(String),

	// keywords
	#[token("fn")]
	Fn,
	#[token("struct")]
	Struct,
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
	#[regex(r"## ([^\r\n]+)", |lex| {
		let s = lex.slice();
		s.get(3..).unwrap_or("").to_owned()
	}, allow_greedy = true)]
	Doc(String),
	#[regex(r"#.*", logos::skip, allow_greedy = true)]
	Comment,
}

impl fmt::Display for Token {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match self {
			Token::Error(s) => write!(f, "{s}"),
			Token::Comment => write!(f, "comment"),
			Token::Doc(_) => write!(f, "doc-comment"),
			Token::Bool(b) => write!(f, "{b}"),
			Token::Int(n) => write!(f, "{n}"),
			Token::Float(s) => write!(f, "{s}"),
			Token::String(s) => write!(f, "\"{s}\""),
			Token::Fn => write!(f, "fn"),
			Token::Struct => write!(f, "struct"),
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
pub fn lex(src: &str) -> Vec<(Token, SimpleSpan)> {
	Token::lexer(src)
		.spanned()
		.map(|(token, span)| match token {
			Ok(token) => (token, span.into()),
			Err(()) => (Token::Error(src[span.clone()].to_string()), span.into()),
		})
		.collect()
}
