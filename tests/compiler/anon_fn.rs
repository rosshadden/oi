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

#[test]
fn capture_read_only() {
	let src = indoc! {"
		factor := 3
		triple := fn [factor] (x int) int { x * factor }
		triple(4)
	"};
	check(src, "12");
}

#[test]
fn capture_multiple() {
	let src = indoc! {"
		a := 10
		b := 32
		add := fn [a, b] () int { a + b }
		add()
	"};
	check(src, "42");
}

#[test]
fn capture_move() {
	let src = indoc! {"
		factor := 3
		triple := fn [move factor] (x int) int { x * factor }
		triple(4)
	"};
	check(src, "12");
}

#[test]
fn capture_undefined() {
	let src = indoc! {"
		f := fn [missing] () int { 0 }
		f()
	"};
	assert!(fail(src).contains("undefined variable"));
}

#[test]
fn capture_mut_writes_visible_outside() {
	let src = indoc! {"
		mut counter := 0
		inc := fn [mut counter] () int { counter = counter + 1; counter }
		inc()
		inc()
		counter
	"};
	check(src, "2");
}

#[test]
fn capture_mut_sees_outer_writes() {
	let src = indoc! {"
		mut x := 1
		bump := fn [mut x] () int { x = x + 10; x }
		x = 5
		bump()
	"};
	check(src, "15");
}

#[test]
fn capture_mut_requires_mut_binding() {
	let src = indoc! {"
		x := 3
		f := fn [mut x] () int { x }
		f()
	"};
	assert!(fail(src).contains("cannot capture `x` as `mut`"));
}

#[test]
fn capturing_closure_rejected_as_plain_fn_param() {
	let src = indoc! {"
		fn apply(f fn(int) int, x int) int { f(x) }
		factor := 2
		scale := fn [factor] (n int) int { n * factor }
		apply(scale, 21)
	"};
	assert!(fail(src).contains("wrong argument type"));
}

#[test]
fn implicit_capture_read_only() {
	let src = indoc! {"
		n := 10
		scale := fn (x int) int { x * n }
		scale(5)
	"};
	check(src, "50");
}

#[test]
fn implicit_capture_multiple() {
	let src = indoc! {"
		a := 10
		b := 32
		add := fn () int { a + b }
		add()
	"};
	check(src, "42");
}

#[test]
fn implicit_capture_ignores_shadowed_inner_binding() {
	let src = indoc! {"
		n := 10
		f := fn () int { n := 5; n }
		f() + n
	"};
	check(src, "15");
}

#[test]
fn implicit_capture_ignores_param_shadowing_outer() {
	let src = indoc! {"
		n := 10
		f := fn (n int) int { n * 2 }
		f(4) + n
	"};
	check(src, "18");
}

#[test]
fn implicit_capture_ignores_for_loop_pattern() {
	let src = indoc! {"
		nums := [1, 2, 3]
		mut total := 0
		f := fn () int {
			mut sum := 0
			loop n in nums { sum = sum + n }
			sum
		}
		total = f()
		total
	"};
	check(src, "6");
}

#[test]
fn implicit_capture_ignores_match_bound_name() {
	let src = indoc! {r#"
		r := !int(7)
		f := fn () int {
			match r {
				.ok(n) => n * 2,
				.err(e) => -1,
			}
		}
		f()
	"#};
	check(src, "14");
}
