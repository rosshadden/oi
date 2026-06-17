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
fn wrong_arg_count() {
	let src = indoc! {"
		fn add(x int, y int) { x + y }
		add(1)
	"};
	assert!(fail(src).contains("expects 2 argument"));
}

#[test]
fn wrong_arg_type() {
	let src = indoc! {r#"
		fn double(x int) { x + x }
		double("nope")
	"#};
	assert!(fail(src).contains("expected Int argument"));
}

#[test]
fn wrong_return_type() {
	let src = indoc! {r#"
		fn bad() int { "nope" }
		bad()
	"#};
	assert!(fail(src).contains("expected Int return value"));
}

#[test]
fn unknown_return_type() {
	let src = indoc! {"
		fn bad() blob { 1 }
		bad()
	"};
	assert!(fail(src).contains("unknown type `blob`"));
}

#[test]
fn return_keyword_wrong_type() {
	let src = indoc! {"
		fn bad() int { return 2.0 }
		bad()
	"};
	assert!(fail(src).contains("expected Int return value"));
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
