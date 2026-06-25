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
fn float_sci_e() {
	check("42e1", "420.0");
}

#[test]
fn float_sci_upper_e() {
	check("42E1", "420.0");
}

#[test]
fn float_sci_negative_exp() {
	check("123e-2", "1.23");
}

#[test]
fn float_sci_positive_exp() {
	check("456e+2", "45600.0");
}

#[test]
fn float_sci_decimal_with_exp() {
	check("1.5e2", "150.0");
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
