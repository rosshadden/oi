use crate::helpers::*;

#[test]
fn ternary_true() {
	check(r#"if 2 > 1 { "yes" } else { "no" }"#, "yes");
}

#[test]
fn ternary_false() {
	check(r#"if 1 > 2 { "yes" } else { "no" }"#, "no");
}

#[test]
fn else_if_first() {
	let src = indoc! {r#"
		i := 0
		if i == 0 { "zero" } else if i == 1 { "one" } else { "idk" }
	"#};
	check(src, "zero");
}

#[test]
fn else_if_middle() {
	let src = indoc! {r#"
		i := 1
		if i == 0 { "zero" } else if i == 1 { "one" } else { "idk" }
	"#};
	check(src, "one");
}

#[test]
fn else_if_last() {
	let src = indoc! {r#"
		i := 2
		if i == 0 { "zero" } else if i == 1 { "one" } else { "idk" }
	"#};
	check(src, "idk");
}

#[test]
fn no_else_true() {
	check("if true { 42 }", "42");
}

// no else yields the zero value of the if's type
#[test]
fn no_else_false_int() {
	check("if false { 42 }", "0");
}

#[test]
fn no_else_false_string() {
	check(r#"if false { "idk" }"#, "");
}

// an `if` is an expression usable anywhere a value is
#[test]
fn if_as_binding() {
	let src = indoc! {"
		x := if true { 10 } else { 20 }
		x
	"};
	check(src, "10");
}

#[test]
fn if_in_arithmetic() {
	check("1 + if true { 2 } else { 3 }", "3");
}

#[test]
fn nested_if() {
	let src = indoc! {"
		x := if true { if false { 1 } else { 2 } } else { 3 }
		x
	"};
	check(src, "2");
}

#[test]
fn float_branches() {
	check("if 1.5 < 2.0 { 1.5 } else { 2.5 }", "1.5");
}

#[test]
fn bool_branches() {
	check("if false { true } else { false }", "false");
}

// a binding declared inside a branch is local to it
#[test]
fn branch_binding_is_local() {
	let src = indoc! {"
		mut x := 1
		if true {
			y := 5
			x = y
		}
		x
	"};
	check(src, "5");
}

#[test]
fn branch_binding_does_not_leak() {
	let src = indoc! {"
		if true { y := 5 }
		y
	"};
	assert!(fail(src).contains("undefined variable"));
}

// `return` inside a branch propagates out of the function
#[test]
fn guard_return_taken() {
	let src = indoc! {"
		fn abs(x int) int {
			if x < 0 { return -x }
			x
		}
		abs(-5)
	"};
	check(src, "5");
}

#[test]
fn guard_return_not_taken() {
	let src = indoc! {"
		fn abs(x int) int {
			if x < 0 { return -x }
			x
		}
		abs(3)
	"};
	check(src, "3");
}

// one branch returns, the other yields a value
#[test]
fn return_in_one_branch() {
	let src = indoc! {"
		fn pick(x int) int {
			if x > 0 { return 1 } else { 99 }
		}
		pick(5)
	"};
	check(src, "1");
}

#[test]
fn condition_must_be_bool() {
	assert!(fail("if 1 { 2 } else { 3 }").contains("must be Bool"));
}

#[test]
fn mismatched_branches() {
	assert!(fail(r#"if true { 1 } else { "x" }"#).contains("mismatched types"));
}
