use crate::helpers::*;

// `()` at top-level yields unit, which prints nothing (same as an empty program)
#[test]
fn unit_literal_top_level() {
	check("()", "");
}

#[test]
fn unit_equality() {
	check("() == ()", "true");
}

#[test]
fn unit_inequality() {
	check("() != ()", "false");
}

// Binding unit and then naming it yields unit → prints nothing
#[test]
fn unit_in_binding() {
	check("x := ()\nx", "");
}

// Empty fn returns unit; calling it at top level produces no output
#[test]
fn empty_fn_returns_unit() {
	let src = indoc! {"
		fn nada() {}
		nada()
	"};
	check(src, "");
}

// Body ending in `()` also returns unit
#[test]
fn fn_explicit_unit_return() {
	let src = indoc! {"
		fn nope() {
			()
		}
		nope()
	"};
	check(src, "");
}

#[test]
fn fn_return_unit_keyword() {
	let src = indoc! {"
		fn no_way() {
			return ()
		}
		no_way()
	"};
	check(src, "");
}

#[test]
fn fn_bare_return_is_unit() {
	let src = indoc! {"
		fn nuh_uh() {
			return
		}
		nuh_uh()
	"};
	check(src, "");
}

#[test]
fn fn_explicit_unit_ret_annotation() {
	let src = indoc! {"
		fn zilch() () {}
		zilch()
	"};
	check(src, "");
}

// All these unit-returning forms are equal to each other and to the literal `()`
#[test]
fn unit_fns_compare_equal() {
	let src = indoc! {"
		fn nada() {}
		fn zilch() () {}
		assert(nada() == zilch())
	"};
	check(src, "true");
}

#[test]
fn unit_fns_equal_unit_literal() {
	let src = indoc! {"
		fn nada() {}
		assert(nada() == ())
	"};
	check(src, "true");
}

#[test]
fn empty_main_prints_nothing() {
	let src = indoc! {"
		fn main() {}
	"};
	check(src, "");
}

// Unit is a printable value when passed to `print`
#[test]
fn print_unit() {
	let src = indoc! {"
		print(())
		42
	"};
	check(src, "()\n42");
}

// Unit in a tuple renders as `()`
#[test]
fn unit_in_tuple() {
	check("((), ())", "((), ())");
}
