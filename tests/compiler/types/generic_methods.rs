use crate::helpers::*;

#[test]
fn basic_dispatch() {
	let src = indoc! {"
		struct Box[T] { v T }
		impl Box[T] { fn get(self) T { self.v } }
		Box{ v: 7 }.get()
	"};
	check(src, "7");
}

#[test]
fn two_instances_coexist() {
	let src = indoc! {r#"
		struct Box[T] { v T }
		impl Box[T] { fn get(self) T { self.v } }
		print(Box{ v: 1 }.get())
		print(Box{ v: "hi" }.get())
	"#};
	check(src, "1\nhi");
}

#[test]
fn method_own_type_param() {
	let src = indoc! {r#"
		struct Box[T] { v T }
		impl Box[T] { fn swap[U](self, u U) U { u } }
		Box{ v: 1 }.swap("hi")
	"#};
	check(src, "hi");
}

#[test]
fn self_return() {
	let src = indoc! {"
		struct Box[T] { v T }
		impl Box[T] { fn same(self) Self { self } }
		Box{ v: 3 }.same().v
	"};
	check(src, "3");
}

#[test]
fn concrete_impl_own_type_param() {
	let src = indoc! {"
		struct Point { x int, y int }
		impl Point { fn id[U](self, u U) U { u } }
		Point{1, 2}.id(5)
	"};
	check(src, "5");
}

#[test]
fn unknown_method_error() {
	let err = fail(indoc! {"
		struct Box[T] { v T }
		impl Box[T] { fn get(self) T { self.v } }
		Box{ v: 1 }.nope()
	"});
	assert!(err.contains("no such method"), "got: {err}");
}

#[test]
fn field_through_self() {
	let src = indoc! {"
		struct Box[T] { v T }
		impl Box[T] { fn double(self) T { self.v + self.v } }
		Box{ v: 7 }.double()
	"};
	check(src, "14");
}
