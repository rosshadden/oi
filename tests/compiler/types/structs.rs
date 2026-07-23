use crate::helpers::*;
use indoc::indoc;

#[test]
fn field_access() {
	check(
		"struct Point { x int, y int }
		point := Point{ x: 1, y: 2 }
		point.x",
		"1",
	);
	check(
		"struct Point { x int, y int }
		point := Point{ x: 1, y: 2 }
		point.y",
		"2",
	);
}

#[test]
fn zero_value() {
	check(
		"struct Point { x int, y int }
		origin := Point{}
		origin.x",
		"0",
	);
	check(
		"struct User { name string, age int }
		u := User{}
		u.age",
		"0",
	);
}

#[test]
fn positional_literal() {
	check(
		"struct Point { x int, y int }
		p := Point{3, 4}
		p.x",
		"3",
	);
	check(
		"struct Point { x int, y int }
		p := Point{3, 4}
		p.y",
		"4",
	);
}

#[test]
fn field_mutation() {
	check(
		"struct Point { x int, y int }
		mut p := Point{}
		p.x = 5
		p.x",
		"5",
	);
	check(
		"struct Point { x int, y int }
		mut p := Point{ x: 10, y: 20 }
		p.y = 99
		p.y",
		"99",
	);
}

#[test]
fn copy_semantics() {
	check(
		"struct Point { x int, y int }
		a := Point{ x: 1, y: 2 }
		mut b := a
		b.x = 99
		a.x",
		"1",
	);
}

#[test]
fn print_struct() {
	check(
		"struct Point { x int, y int }
		print(Point{ x: 1, y: 2 })",
		"Point{x: 1, y: 2}",
	);
	check(
		"struct Point { x int, y int }
		print(Point{})",
		"Point{x: 0, y: 0}",
	);
}

#[test]
fn mixed_field_types() {
	check(
		r#"struct Foo { n int, s string, f float }
		v := Foo{ n: 42, s: "hi", f: 1.5 }
		v.n"#,
		"42",
	);
	check(
		r#"struct Foo { n int, s string }
		v := Foo{ n: 7, s: "world" }
		v.s"#,
		"world",
	);
}

#[test]
fn fn_return_type_annotation() {
	let src = indoc! {"
		struct Point { x int, y int }
		fn origin() Point { Point{} }
		origin()
	"};
	check(src, "Point{x: 0, y: 0}");
}

#[test]
fn fn_return_type_annotation_named_fields() {
	let src = indoc! {"
		struct Point { x int, y int }
		fn make(a int, b int) Point { Point{ x: a, y: b } }
		make(3, 4)
	"};
	check(src, "Point{x: 3, y: 4}");
}

#[test]
fn fn_return_type_annotation_mismatch() {
	let src = indoc! {"
		struct Point { x int, y int }
		fn bad() Point { 42 }
		bad()
	"};
	assert!(fail(src).contains("wrong return type"));
}

#[test]
fn fn_param_struct_type() {
	let src = indoc! {"
		struct Point { x int, y int }
		fn sum(p Point) int { p.x + p.y }
		sum(Point{ x: 3, y: 4 })
	"};
	check(src, "7");
}

#[test]
fn if_no_else_struct_zero() {
	let src = indoc! {"
		struct Point { x int, y int }
		p := if false { Point{ x: 1, y: 2 } }
		p.x
	"};
	check(src, "0");
}

#[test]
fn immutable_field_assign_error() {
	let err = fail(
		"struct Point { x int, y int }
		p := Point{}
		p.x = 5",
	);
	assert!(err.contains("immutable"), "{err}");
}

#[test]
fn struct_positional_field_access() {
	let src = indoc! {"
		struct Point { x int, y int }
		p := Point{ 2 4 }
		p.1 == p.y
	"};
	check(src, "true");
}

#[test]
fn record_coerces_to_struct() {
	check(
		"struct Point { x int, y int }
		p Point := { x: 2, y: 1 }
		p.x + p.y",
		"3",
	);
	check(
		"struct Point { x int, y int }
		x := 5
		y := 7
		p Point := { x, y }
		p.y",
		"7",
	);
}

#[test]
fn record_as_call_arg() {
	let src = indoc! {"
		struct Point { x int, y int }
		fn sum(p Point) int { p.x + p.y }
		sum({ x: 3, y: 4 })
	"};
	check(src, "7");
}

#[test]
fn record_in_return_position() {
	let src = indoc! {"
		struct Point { x int, y int }
		fn make() Point { { x: 1, y: 2 } }
		make()
	"};
	check(src, "Point{x: 1, y: 2}");
}

#[test]
fn empty_record_defaults_struct() {
	check(
		"struct User { age int, swag int = 5 }
		u User := {}
		u.swag",
		"5",
	);
}

#[test]
fn record_unknown_field_error() {
	let err = fail(
		"struct Point { x int, y int }
		p Point := { z: 1 }",
	);
	assert!(err.contains("no field `z`"), "{err}");
}

#[test]
fn record_non_ident_key_error() {
	let err = fail(
		r#"struct Point { x int, y int }
		p Point := { "x": 1 }"#,
	);
	assert!(err.contains("named by idents"), "{err}");
}

#[test]
fn default_field_value() {
	// empty literal uses the default
	check(
		"struct User { age int, name string, swag int = 5 }
		u := User{}
		u.swag",
		"5",
	);
	// partial named literal
	check(
		"struct User { age int, swag int = 5 }
		u := User{ age: 30 }
		u.swag",
		"5",
	);
	// explicit value overrides the default
	check(
		"struct User { age int, swag int = 5 }
		u := User{ swag: 99 }
		u.swag",
		"99",
	);
	// non-defaulted fields still zero-init
	check(
		"struct User { age int, swag int = 5 }
		u := User{}
		u.age",
		"0",
	);
}

#[test]
fn named_call_args() {
	check(
		"struct Options { foo int, bar bool }
		fn f(o Options) { print(o.foo) }
		f(bar: true, foo: 4)",
		"4",
	);
}

#[test]
fn named_method_args() {
	check(
		"struct Options { foo int, bar bool }
		struct User {}
		impl User {
			fn with_options(self, opt Options) { print(opt.bar) }
		}
		user := User{}
		user.with_options(bar: true, foo: 4)",
		"true",
	);
}

#[test]
fn mixed_positional_and_named_args() {
	check(
		"struct Options { foo int }
		fn g(x int, o Options) { print(x + o.foo) }
		g(1, foo: 2)",
		"3",
	);
}

#[test]
fn named_before_positional_error() {
	let err = fail(
		"struct Options { foo int }
		fn g(x int, o Options) {}
		g(foo: 1, 2)",
	);
	assert!(err.contains("positional args go before named args"), "{err}");
}
