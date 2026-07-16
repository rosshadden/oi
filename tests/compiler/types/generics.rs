use crate::helpers::*;
use indoc::indoc;

#[test]
fn max_int() {
	let src = indoc! {"
		fn max[T](a T, b T) T {
			if a > b { a } else { b }
		}
		max(3, 7)
	"};
	check(src, "7");
}

#[test]
fn max_float_instantiation_is_independent() {
	let src = indoc! {"
		fn max[T](a T, b T) T {
			if a > b { a } else { b }
		}
		mut a := max(3, 7)
		max(3.5, 1.2)
	"};
	check(src, "3.5");
}

#[test]
fn self_recursive_generic() {
	let src = indoc! {"
		fn fact[T](n T) T {
			if n <= 1 { 1 } else { n * fact(n - 1) }
		}
		fact(5)
	"};
	check(src, "120");
}

#[test]
fn mutually_recursive_generics() {
	let src = indoc! {"
		fn is_even[T](n T) bool {
			if n == 0 { true } else { is_odd(n - 1) }
		}
		fn is_odd[T](n T) bool {
			if n == 0 { false } else { is_even(n - 1) }
		}
		is_even(10)
	"};
	check(src, "true");
}

#[test]
fn first_of_array() {
	let src = indoc! {"
		fn first[T](xs []T) ?T {
			if xs.len == 0 { ?T(none) } else { ?T(xs[0]) }
		}
		first([1, 2, 3])
	"};
	check(src, "some");
}

#[test]
fn type_mismatch_across_args() {
	let err = fail("fn max[T](a T, b T) T { if a > b { a } else { b } }\nmax(1, \"a\")");
	assert!(err.contains("bound to both"), "got: {err}");
}

#[test]
fn missing_return_type_errors() {
	let err = fail("fn noret[T](x T) { x }\nnoret(1)");
	assert!(err.contains("needs an explicit return type"), "got: {err}");
}
