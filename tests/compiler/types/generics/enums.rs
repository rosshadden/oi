use crate::helpers::*;

#[test]
fn shorthand_round_trip() {
	let src = indoc! {"
		enum Opt[T] { nope, some(T) }
		fn get() Opt[int] { .some(5) }
		match get() {
			.some(n) { n }
			.nope { -1 }
		}
	"};
	check(src, "5");
}

#[test]
fn nope_arm() {
	let src = indoc! {"
		enum Opt[T] { nope, some(T) }
		fn get() Opt[int] { .nope }
		match get() {
			.some(n) { n }
			.nope { -1 }
		}
	"};
	check(src, "-1");
}

#[test]
fn generic_fn_round_trip() {
	let src = indoc! {"
		enum Opt[T] { nope, some(T) }
		fn wrap[T](v T) Opt[T] { .some(v) }
		match wrap(9) {
			.some(n) { n }
			.nope { -1 }
		}
	"};
	check(src, "9");
}

#[test]
fn two_instances_coexist() {
	let src = indoc! {r#"
		enum Opt[T] { nope, some(T) }
		fn geti() Opt[int] { .some(1) }
		fn gets() Opt[string] { .some("hi") }
		match geti() { .some(n) { print(n) } .nope {} }
		match gets() { .some(s) { print(s) } .nope {} }
	"#};
	check(src, "1\nhi");
}

#[test]
fn bare_name_needs_type_arguments() {
	let err = fail(indoc! {"
		enum Opt[T] { nope, some(T) }
		fn f(o Opt) int { 0 }
		0
	"});
	assert!(err.contains("needs type arguments"), "got: {err}");
}

#[test]
fn wrong_arity() {
	let err = fail(indoc! {"
		enum Opt[T] { nope, some(T) }
		fn f() Opt[int, string] { .nope }
		0
	"});
	assert!(err.contains("expects 1 type argument(s), got 2"), "got: {err}");
}

#[test]
fn recursive_payload() {
	let src = indoc! {"
		enum Tree[T] { leaf(T), node(Tree[T]) }
		fn f() Tree[int] { .node(.leaf(5)) }
		match f() {
			.leaf(v) { v }
			.node(inner) { match inner { .leaf(v) { v } .node(x) { -1 } } }
		}
	"};
	check(src, "5");
}
