use crate::helpers::*;

#[test]
fn variable() {
	check("x := 42\nx", "42");
}

#[test]
fn assign() {
	check("mut x := 1\nx = 2\nx", "2");
}

#[test]
fn assign_from_self() {
	check("mut x := 10\nx = x + 5\nx", "15");
}

#[test]
fn assign_string() {
	check(r#"mut s := "old"; s = "new"; s"#, "new");
}

#[test]
fn declare_zero_int() {
	check("mut n int\nn", "0");
}

#[test]
fn declare_zero_string() {
	check("mut s string\ns", "");
}

#[test]
fn declare_zero_then_assign() {
	check("mut n int\nn = 7\nn", "7");
}

#[test]
fn declare_zero_struct() {
	check(
		"struct Point { x int, y int }
		mut p Point
		p.x = 5
		p.x",
		"5",
	);
}

#[test]
fn annotated_binding() {
	check("a int := 2\na", "2");
	check(r#"b string := "hi"; b"#, "hi");
}

#[test]
fn annotation_type_mismatch() {
	assert!(fail(r#"x int := "hi""#).contains("expected int, got str"));
}

#[test]
fn annotation_pins_width() {
	// the literal fits an i32, but the annotation widens it to i64
	check("mut big i64 := 50_000\nbig", "50000");
}

#[test]
fn annotation_coerces_float() {
	check("f f32 := 1.5\nf", "1.5");
	check("x f64 := 5\nx", "5.0");
}

#[test]
fn annotation_out_of_range() {
	assert!(fail("mut x i8 := 9999\nx").contains("out of range for i8"));
}
