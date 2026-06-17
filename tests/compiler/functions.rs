use crate::helpers::*;

#[test]
fn fn_call() {
	let src = indoc! {"
		fn double() { 21 * 2 }
		double()
	"};
	check(src, "42");
}

#[test]
fn multi_fn() {
	let src = indoc! {"
		fn base() {
			6
		}

		fn triple() {
			base() + base() + base()
		}

		triple()
	"};
	check(src, "18");
}

#[test]
fn fn_vars() {
	let src = indoc! {"
		fn area() {
			width := 12
			height := 5
			width * height
		}

		area()
	"};
	check(src, "60");
}

#[test]
fn fn_args() {
	let src = indoc! {"
		fn add(x int, y int) {
			x + y
		}
		add(3, 4)
	"};
	check(src, "7");
}

#[test]
fn fn_arg_passthrough() {
	let src = indoc! {"
		fn identity(x int) { x }
		identity(99)
	"};
	check(src, "99");
}

#[test]
fn fn_args_nested() {
	let src = indoc! {"
		fn add(x int, y int) { x + y }
		fn add3(a int, b int, c int) { add(add(a, b), c) }
		add3(1, 2, 3)
	"};
	check(src, "6");
}

#[test]
fn fn_arg_float() {
	let src = indoc! {"
		fn scale(x f64) { x * 2.0 }
		scale(2.5)
	"};
	check(src, "5.0");
}

#[test]
fn fn_arg_trailing_comma() {
	let src = indoc! {"
		fn add(x int, y int,) { x + y }
		add(40, 2,)
	"};
	check(src, "42");
}

#[test]
fn fn_arg_wrong_type() {
	let src = indoc! {"
		fn i(x int) { x }
		i(2.4)
	"};
	assert!(fail(src).contains("wrong argument type"));
}

#[test]
fn fn_return_type() {
	let src = indoc! {"
		fn add(x int, y int) int {
			x + y
		}
		add(3, 4)
	"};
	check(src, "7");
}

#[test]
fn fn_return_type_float() {
	let src = indoc! {"
		fn scale(x f64) f64 { x * 2.0 }
		scale(2.5)
	"};
	check(src, "5.0");
}
