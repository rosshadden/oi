use crate::helpers::*;

#[test]
fn undefined_variable() {
	assert!(fail("foo").contains("undefined variable"));
}

#[test]
fn undefined_function() {
	assert!(fail("bar()").contains("undefined function"));
}

#[test]
fn type_mismatch() {
	assert!(fail(r#"1 + "x""#).contains("cannot Add"));
}

#[test]
fn unexpected_token() {
	// `+` with no RHS runs into end of input
	assert!(fail("2 +").contains("expected"));
}

#[test]
fn invalid_token() {
	// a stray char becomes `Token::Error`, surfaced by the parser with its text
	assert!(fail("@").contains("unexpected character `@`"));
}

#[test]
fn top_level_stmt_with_main() {
	let src = indoc! {"
		fn main() {
			1
		}
		2
	"};
	assert!(fail(src).contains("top-level statements"));
}
