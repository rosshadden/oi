use crate::helpers::*;

#[test]
fn tuple_literal() {
	check("(1, 2, 3)", "(1, 2, 3)");
}

#[test]
fn tuple_mixed_types() {
	// strings are quoted when printed inside a tuple
	check(r#"(true, 2, "lol")"#, r#"(true, 2, "lol")"#);
}

#[test]
fn tuple_named() {
	check("(a: 1, b: 2)", "(a: 1, b: 2)");
}

#[test]
fn tuple_partially_named() {
	check("(1, b: 2)", "(1, b: 2)");
}

#[test]
fn tuple_trailing_comma() {
	check("(1, 2,)", "(1, 2)");
}

#[test]
fn one_tuple_needs_comma() {
	// `(e)` is grouping, a 1-tuple needs the trailing comma
	check("(1)", "1");
	check("(1,)", "(1)");
}

#[test]
fn no_comma_ints() {
	// spec: `only_nums := (2 3 4)`
	check("(2 3 4)", "(2, 3, 4)");
}

#[test]
fn no_comma_mixed_literals() {
	// spec: `other_literals := ("lisp, innit?" true [2 4 5])`
	check(
		r#"("lisp, innit?" true [2, 4, 5])"#,
		r#"("lisp, innit?", true, [2, 4, 5])"#,
	);
}

#[test]
fn no_comma_nested_array_no_comma() {
	// nested array also uses comma-free syntax
	check(
		r#"("lisp, innit?" true [2 4 5])"#,
		r#"("lisp, innit?", true, [2, 4, 5])"#,
	);
}

#[test]
fn nested_tuple() {
	check("(1, (2, 3))", "(1, (2, 3))");
}

#[test]
fn field_by_index() {
	check("t := (10, 20)\nt.1", "20");
}

#[test]
fn field_by_name() {
	check("t := (a: 1, b: 2)\nt.b", "2");
}

#[test]
fn named_and_positional_agree() {
	check("t := (a: 1, b: 2); assert(t.a == t.0)", "true");
}

#[test]
fn field_float_load() {
	check("t := (1.5, 2.5)\nt.0", "1.5");
}

#[test]
fn field_arithmetic() {
	check("t := (3, 4)\nt.0 * t.1", "12");
}

#[test]
fn tuple_in_var_prints() {
	check(r#"t := (1, "two", 3.0); t"#, r#"(1, "two", 3.0)"#);
}

#[test]
fn index_out_of_range() {
	assert!(fail("t := (1, 2)\nt.5").contains("out of range"));
}

#[test]
fn unknown_named_field() {
	assert!(fail("t := (a: 1)\nt.z").contains("no field `z`"));
}

#[test]
fn field_of_non_tuple() {
	assert!(fail("x := 5\nx.0").contains("cannot access a field"));
}

#[test]
fn fn_returns_tuple() {
	let src = indoc! {"
		fn pair() { (1, 2) }
		pair()
	"};
	check(src, "(1, 2)");
}

#[test]
fn fn_returns_tuple_field() {
	let src = indoc! {"
		fn pair() { (10, 20) }
		t := pair()
		t.1
	"};
	check(src, "20");
}

#[test]
fn fn_return_type_annotation_tuple() {
	let src = indoc! {"
		fn pair() (int, int) { (3, 4) }
		pair()
	"};
	check(src, "(3, 4)");
}

#[test]
fn fn_return_type_mismatch_tuple() {
	let src = indoc! {"
		fn bad() (int, int) { 42 }
		bad()
	"};
	assert!(fail(src).contains("wrong return type"));
}

#[test]
fn fn_tuple_return_composing() {
	let src = indoc! {"
		fn swap(x int, y int) (int, int) { (y, x) }
		t := swap(1, 2)
		t.0
	"};
	check(src, "2");
}

#[test]
fn if_no_else_tuple_zero() {
	let src = indoc! {"
		t := if false { (1, 2) }
		t
	"};
	check(src, "(0, 0)");
}
