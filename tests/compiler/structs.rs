use super::helpers::*;

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
		"Point{x: 1, y: 2}\n0",
	);
	check(
		"struct Point { x int, y int }
		print(Point{})",
		"Point{x: 0, y: 0}\n0",
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
fn immutable_field_assign_error() {
	let err = fail(
		"struct Point { x int, y int }
		p := Point{}
		p.x = 5",
	);
	assert!(err.contains("immutable"), "{err}");
}
