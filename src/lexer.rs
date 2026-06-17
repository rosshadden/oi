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
	#[regex(r"[0-9]+", |lex| lex.slice().parse().ok())]
	Int(i32),
	#[regex(r"[0-9]+\.[0-9]+", |lex| lex.slice().parse().ok())]
	Float(f64),
	#[regex(r#""[^"]*""#, |lex| { let s = lex.slice(); s[1..s.len() - 1].to_string() })]
	String(String),

	// keywords
	#[token("fn")]
	Fn,
	#[token("return")]
	Return,

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

	// delimiters
	#[token("(")]
	LParen,
	#[token(")")]
	RParen,
	#[token("{")]
	LBrace,
	#[token("}")]
	RBrace,
	#[token(",")]
	Comma,
	// statement separator (for joining lines)
	#[token(";", logos::skip)]
	Semicolon,

	// comments
	#[regex(r"#.*", logos::skip, allow_greedy = true)]
	Comment,
}

impl fmt::Display for Token {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match self {
			Token::Error(s) => write!(f, "{s}"),
			Token::Comment => write!(f, "comment"),
			Token::Bool(b) => write!(f, "{b}"),
			Token::Int(n) => write!(f, "{n}"),
			Token::Float(x) => write!(f, "{x}"),
			Token::String(s) => write!(f, "\"{s}\""),
			Token::Fn => write!(f, "fn"),
			Token::Return => write!(f, "return"),
			Token::Mut => write!(f, "mut"),
			Token::Ident(name) => write!(f, "{name}"),
			Token::Bind => write!(f, ":="),
			Token::Assign => write!(f, "="),
			Token::Plus => write!(f, "+"),
			Token::Minus => write!(f, "-"),
			Token::Asterisk => write!(f, "*"),
			Token::Slash => write!(f, "/"),
			Token::LParen => write!(f, "("),
			Token::RParen => write!(f, ")"),
			Token::LBrace => write!(f, "{{"),
			Token::RBrace => write!(f, "}}"),
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
