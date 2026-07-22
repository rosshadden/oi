use crate::helpers::*;
use indoc::indoc;

#[test]
fn infer_from_literal() {
	let src = indoc! {"
		struct Pair[T] { a T, b T }
		p := Pair{ a: 3, b: 4 }
		p.a + p.b
	"};
	check(src, "7");
}

#[test]
fn nested_instantiation() {
	let src = indoc! {"
		struct Box[T] { v T }
		Box{ v: Box{ v: 5 } }.v.v
	"};
	check(src, "5");
}

#[test]
fn type_position_param() {
	let src = indoc! {"
		struct Pair[T] { a T, b T }
		fn sum(p Pair[int]) int { p.a + p.b }
		sum(Pair{ a: 3, b: 4 })
	"};
	check(src, "7");
}

#[test]
fn conflicting_field_types_error() {
	let err = fail(indoc! {r#"
		struct Pair[T] { a T, b T }
		Pair{ a: 3, b: "x" }
	"#});
	assert!(err.contains("bound to both"), "got: {err}");
}

#[test]
fn cannot_infer_error() {
	let err = fail(indoc! {"
		struct Pair[T] { a T, b T }
		Pair{}
	"});
	assert!(err.contains("cannot infer"), "got: {err}");
}

#[test]
fn bare_name_needs_type_arguments() {
	let err = fail(indoc! {"
		struct Pair[T] { a T, b T }
		fn f(p Pair) int { p.a }
		0
	"});
	assert!(err.contains("needs type arguments"), "got: {err}");
}

#[test]
fn generic_fn_round_trip() {
	let src = indoc! {"
		struct Box[T] { v T }
		fn wrap[T](v T) Box[T] { Box{ v: v } }
		wrap(9).v
	"};
	check(src, "9");
}

#[test]
fn concrete_field_type_still_checked() {
	let err = fail(indoc! {r#"
		struct Tagged[T] { v T, id int }
		Tagged{ v: 1.5, id: "x" }
	"#});
	assert!(err.contains("expected int"), "got: {err}");
}

#[test]
fn type_args_on_non_generic_struct_error() {
	let err = fail(indoc! {"
		struct Point { x int, y int }
		fn f(p Point[int]) int { p.x }
		0
	"});
	assert!(err.contains("is not generic"), "got: {err}");
}
