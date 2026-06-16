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
