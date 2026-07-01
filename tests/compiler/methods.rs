use crate::helpers::*;

#[test]
fn instance_method() {
	let src = indoc! {"
		struct Point { x int, y int }
		impl Point {
			fn sum(self) int { self.x + self.y }
		}
		p := Point{3, 4}
		p.sum()
	"};
	check(src, "7");
}

#[test]
fn method_with_args() {
	let src = indoc! {"
		struct Point { x int, y int }
		impl Point {
			fn scaled(self, k int) int { (self.x + self.y) * k }
		}
		Point{3, 4}.scaled(10)
	"};
	check(src, "70");
}

#[test]
fn method_on_literal() {
	let src = indoc! {"
		struct P { x int, y int }
		impl P { fn sum(self) int { self.x + self.y } }
		P{3, 4}.sum()
	"};
	check(src, "7");
}

#[test]
fn method_returns_struct_field() {
	let src = indoc! {"
		struct User { name string, age int }
		impl User {
			fn can_register(self) bool { self.age > 16 }
		}
		User{name: \"ada\", age: 36}.can_register()
	"};
	check(src, "true");
}

#[test]
fn static_method() {
	let src = indoc! {"
		struct Point { x int, y int }
		impl Point {
			fn origin() Point { Point{0, 0} }
			fn sum(self) int { self.x + self.y }
		}
		Point.origin().sum()
	"};
	check(src, "0");
}

#[test]
fn static_method_with_args() {
	let src = indoc! {"
		struct Point { x int, y int }
		impl Point { fn make(a int, b int) Point { Point{a, b} } }
		Point.make(3, 4).x
	"};
	check(src, "3");
}

#[test]
fn no_such_method() {
	let err = fail(indoc! {"
		struct P { x int }
		p := P{1}
		p.nope()
	"});
	assert!(err.contains("no method `nope`"), "{err}");
}

#[test]
fn methods_only_on_structs() {
	let err = fail("(5).double()");
	assert!(err.contains("no methods"), "{err}");
}

#[test]
fn wrong_arg_count() {
	let err = fail(indoc! {"
		struct P { x int }
		impl P { fn add(self, k int) int { self.x + k } }
		P{1}.add()
	"});
	assert!(err.contains("expects 1 argument"), "{err}");
}
