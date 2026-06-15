use logos::Logos;

pub enum Operator {}

#[derive(Logos, Clone, PartialEq, Debug)]
#[logos(skip r"[ \t\r\n\f]+")]
pub enum Token {
	#[regex(r"#.*", logos::skip, allow_greedy = true)]
	Comment,

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
	#[token("{")]
	LBrace,
	#[token("}")]
	RBrace,
}
