use crate::helpers::*;

#[test]
fn assert_true() {
	check("assert(true)", "true");
}

#[test]
fn assert_condition() {
	check("assert(1 == 1)", "true");
}

#[test]
fn assert_as_statement() {
	check("assert(2 > 1)\n42", "42");
}

#[test]
fn assert_false_aborts() {
	let err = fail("assert(false)");
	assert!(err.contains("assertion failed"), "{err}");
}

#[test]
fn assert_false_with_message() {
	let err = fail(r#"assert(false, "bad value")"#);
	assert!(err.contains("bad value"), "{err}");
}

#[test]
fn assert_wrong_arg_count() {
	assert!(fail("assert()").contains("1 or 2 arguments"));
	assert!(fail(r#"assert(true, "a", "b")"#).contains("1 or 2 arguments"));
}

#[test]
fn assert_non_bool_condition() {
	assert!(fail("assert(1)").contains("must be Bool"));
}

#[test]
fn assert_non_str_message() {
	assert!(fail("assert(false, 42)").contains("must be Str"));
}
