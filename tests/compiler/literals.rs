use crate::helpers::*;

#[test]
fn int_literal() {
	check("42", "42");
}

#[test]
fn float_literal() {
	check("3.14", "3.14");
}

#[test]
fn bool_true() {
	check("true", "true");
}

#[test]
fn bool_false() {
	check("false", "false");
}

#[test]
fn string_literal() {
	check(r#""hello""#, "hello");
}
