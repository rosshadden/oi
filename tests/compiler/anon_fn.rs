use crate::helpers::*;

#[test]
fn call_via_var() {
	let src = indoc! {"
		mul := fn [] (x int, y int) int { x * y }
		mul(6, 7)
	"};
	check(src, "42");
}

#[test]
fn call_via_var_no_args() {
	let src = indoc! {"
		answer := fn [] () int { 42 }
		answer()
	"};
	check(src, "42");
}

#[test]
fn call_via_var_passed_to_fn() {
	let src = indoc! {"
		fn apply(f fn(int) int, x int) int { f(x) }
		double := fn [] (n int) int { n * 2 }
		apply(double, 21)
	"};
	check(src, "42");
}

#[test]
fn wrong_arg_count() {
	let src = indoc! {"
		add := fn [] (x int, y int) int { x + y }
		add(1)
	"};
	assert!(fail(src).contains("expects 2 argument"));
}

#[test]
fn wrong_arg_type() {
	let src = indoc! {"
		add := fn [] (x int, y int) int { x + y }
		add(1, 2.0)
	"};
	assert!(fail(src).contains("wrong argument type"));
}

#[test]
fn not_callable() {
	let src = indoc! {"
		x := 5
		x()
	"};
	assert!(fail(src).contains("not callable"));
}
