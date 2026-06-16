use crate::helpers::*;

#[test]
fn int_add() {
	check("2 + 3", "5");
}

#[test]
fn int_sub() {
	check("10 - 4", "6");
}

#[test]
fn int_mul() {
	check("3 * 4", "12");
}

#[test]
fn int_div() {
	check("10 / 3", "3");
}

#[test]
fn float_add() {
	check("1.5 + 2.0", "3.5");
}

#[test]
fn negation() {
	check("-5", "-5");
}

// variables

#[test]
fn variable() {
	check("x := 42\nx", "42");
}

