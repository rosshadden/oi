use crate::helpers::*;

#[test]
fn dollar_scalar() {
	let src = indoc! {"
		fn f(x int) int { assert(x == $); $ }
		f(7)
	"};
	check(src, "7");
}

#[test]
fn dollar_scalar_has_no_fields() {
	let err = fail("fn f(x int) int { $.0 } f(9)");
	assert!(err.contains("cannot access a field of int"), "{err}");
}

#[test]
fn dollar_one_tuple() {
	let src = indoc! {"
		fn f(x int,) int { assert(x == $.0); $.0 }
		f(5)
	"};
	check(src, "5");
}

#[test]
fn dollar_two_tuple() {
	let src = indoc! {"
		fn f(x int, y int) int { assert(x == $.0); assert(y == $.1); $.0 + $.1 }
		f(3, 4)
	"};
	check(src, "7");
}

#[test]
fn dollar_tuple_prints() {
	check("fn f(x int, y int) { $ } f(3, 4)", "(3, 4)");
}

#[test]
fn dollar_unit() {
	check("fn f() bool { $ == () } f()", "true");
}

#[test]
fn dollar_index_out_of_range() {
	let err = fail("fn f(x int, y int) int { $.5 } f(1, 2)");
	assert!(err.contains("out of range"), "{err}");
}
