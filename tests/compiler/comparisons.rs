use crate::helpers::*;

#[test]
fn int_eq() {
	check("2 == 2", "true");
}

#[test]
fn int_eq_false() {
	check("2 == 3", "false");
}

#[test]
fn int_ne() {
	check("2 != 3", "true");
}

#[test]
fn int_lt() {
	check("2 < 3", "true");
}

#[test]
fn int_gt() {
	check("2 > 3", "false");
}

#[test]
fn int_le() {
	check("3 <= 3", "true");
}

#[test]
fn int_ge() {
	check("2 >= 3", "false");
}

#[test]
fn float_cmp() {
	check("1.5 < 2.0", "true");
}

#[test]
fn bool_eq() {
	check("true == true", "true");
}

#[test]
fn bool_ne() {
	check("true != false", "true");
}

#[test]
fn compares_variable() {
	let src = indoc! {"
		x := 5
		x > 3
	"};
	check(src, "true");
}

#[test]
fn looser_than_arithmetic() {
	// parses as (1 + 2) < (2 + 2)
	check("1 + 2 < 2 + 2", "true");
}

#[test]
fn equality_looser_than_relational() {
	// parses as (1 < 2) == (3 < 4) -> true == true
	check("1 < 2 == 3 < 4", "true");
}

#[test]
fn mismatched_types() {
	assert!(fail("1 < 2.0").contains("cannot compare"));
}
