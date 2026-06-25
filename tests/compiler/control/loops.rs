use crate::helpers::*;

// the body repeats until a `return` leaves the function
#[test]
fn counts_to_a_return() {
	let src = indoc! {"
		mut i := 0
		loop {
			i = i + 1
			if i == 3 { return i }
		}
	"};
	check(src, "3");
}

// mutable state carries across iterations
#[test]
fn sums_across_iterations() {
	let src = indoc! {"
		mut sum := 0
		mut i := 0
		loop {
			i = i + 1
			sum = sum + i
			if i == 4 { return sum }
		}
	"};
	check(src, "10");
}

// a `loop` inside a function returns out of it
#[test]
fn loop_in_function() {
	let src = indoc! {"
		fn pow2_over(n int) int {
			mut x := 1
			loop {
				if x > n { return x }
				x = x * 2
			}
		}
		pow2_over(10)
	"};
	check(src, "16");
}

// `break` exits the loop, and the statements after it run
#[test]
fn break_exits() {
	let src = indoc! {"
		mut i := 0
		loop {
			i = i + 1
			if i == 3 { break }
		}
		i
	"};
	check(src, "3");
}

// `continue` skips the rest of the body: sum the evens in 1..=10, breaking past 10
#[test]
fn continue_skips() {
	let src = indoc! {"
		mut sum := 0
		mut i := 0
		loop {
			i = i + 1
			if i > 10 { break }
			if i % 2 == 1 { continue }
			sum = sum + i
		}
		sum
	"};
	check(src, "30");
}

// `break` leaves only the innermost loop
#[test]
fn break_targets_innermost() {
	let src = indoc! {"
		mut outer := 0
		loop {
			outer = outer + 1
			mut inner := 0
			loop {
				inner = inner + 1
				if inner == 2 { break }
			}
			if outer == 3 { break }
		}
		outer
	"};
	check(src, "3");
}

#[test]
fn break_outside_loop() {
	assert!(fail("break").contains("outside of a loop"));
}

#[test]
fn continue_outside_loop() {
	assert!(fail("continue").contains("outside of a loop"));
}

// a condition makes `loop` a `while`: it runs until the condition goes false
#[test]
fn while_counts() {
	let src = indoc! {"
		mut i := 0
		loop i < 5 {
			i = i + 1
		}
		i
	"};
	check(src, "5");
}

// a condition false from the start runs the body zero times
#[test]
fn while_never_enters() {
	let src = indoc! {"
		mut i := 10
		loop i < 5 {
			i = i + 1
		}
		i
	"};
	check(src, "10");
}

// the condition is re-tested every iteration as the body mutates state
#[test]
fn while_sums() {
	let src = indoc! {"
		mut sum := 0
		mut i := 0
		loop i < 5 {
			i = i + 1
			sum = sum + i
		}
		sum
	"};
	check(src, "15");
}

// `break` leaves a `while` early, before the condition would
#[test]
fn while_break() {
	let src = indoc! {"
		mut i := 0
		loop i < 100 {
			i = i + 1
			if i == 3 { break }
		}
		i
	"};
	check(src, "3");
}

// `continue` jumps back to the top, which re-tests the condition: sum the evens in 1..=10
#[test]
fn while_continue() {
	let src = indoc! {"
		mut sum := 0
		mut i := 0
		loop i < 10 {
			i = i + 1
			if i % 2 == 1 { continue }
			sum = sum + i
		}
		sum
	"};
	check(src, "30");
}

// a non-Bool condition is rejected
#[test]
fn while_condition_must_be_bool() {
	assert!(fail("loop 3 { }").contains("must be Bool"));
}

// for loops over a range

// a range loop walks `[start, end)`
// 0..5 -> 0+1+2+3+4
#[test]
fn for_range_sums() {
	let src = indoc! {"
		mut sum := 0
		loop i in 0..5 {
			sum = sum + i
		}
		sum
	"};
	check(src, "10");
}

// the end bound is excluded
#[test]
fn for_range_excludes_end() {
	let src = indoc! {"
		loop i in 0..3 { print(i) }
	"};
	check(src, "0\n1\n2");
}

// an empty range runs the body zero times
#[test]
fn for_range_empty() {
	let src = indoc! {"
		mut sum := 99
		loop i in 3..3 {
			sum = 0
		}
		sum
	"};
	check(src, "99");
}

// the bounds are arbitrary Int expressions
#[test]
fn for_range_variable_bounds() {
	let src = indoc! {"
		lo := 2
		hi := 5
		mut sum := 0
		loop i in lo..hi {
			sum = sum + i
		}
		sum
	"};
	check(src, "9");
}

// `break` leaves a range loop early
#[test]
fn for_range_break() {
	let src = indoc! {"
		mut sum := 0
		loop i in 0..100 {
			if i == 5 { break }
			sum = sum + i
		}
		sum
	"};
	check(src, "10");
}

// `continue` still advances the counter: sum the evens in 0..6
#[test]
fn for_range_continue_advances() {
	let src = indoc! {"
		mut sum := 0
		loop i in 0..6 {
			if i % 2 == 1 { continue }
			sum = sum + i
		}
		sum
	"};
	check(src, "6");
}

// nested range loops each get their own counter
#[test]
fn for_range_nested() {
	let src = indoc! {"
		mut n := 0
		loop i in 0..3 {
			loop j in 0..3 {
				n = n + 1
			}
		}
		n
	"};
	check(src, "9");
}

// a range loop inside a function returns out of it
#[test]
fn for_range_returns() {
	let src = indoc! {"
		fn square_at(n int) int {
			loop i in 0..10 {
				if i == n { return i * i }
			}
			return 0
		}
		square_at(3)
	"};
	check(src, "9");
}

// the loop variable doesn't leak past the loop
#[test]
fn for_var_is_scoped() {
	assert!(fail("loop i in 0..3 { i }\ni").contains("undefined variable"));
}

// for loops over an array

#[test]
fn for_each_sums() {
	let src = indoc! {"
		mut sum := 0
		loop x in [2, 4, 6, 8] {
			sum = sum + x
		}
		sum
	"};
	check(src, "20");
}

#[test]
fn for_each_variable_array() {
	let src = indoc! {"
		a := [10, 20, 30]
		mut sum := 0
		loop x in a {
			sum = sum + x
		}
		sum
	"};
	check(src, "60");
}

#[test]
fn for_each_strings() {
	let src = indoc! {r#"
		loop s in ["a", "b", "c"] { write(s) }
		""
	"#};
	check(src, "abc");
}

// a slice iterates over just its window
#[test]
fn for_each_slice() {
	let src = indoc! {"
		a := [0, 2, 4, 6, 8]
		mut sum := 0
		loop x in a[1..4] {
			sum = sum + x
		}
		sum
	"};
	check(src, "12");
}

// a tuple pattern destructures each element
#[test]
fn for_each_tuple_destructure() {
	let src = indoc! {"
		mut sum := 0
		loop (x, y) in [(0, 0), (1, 2), (3, 4)] {
			sum = sum + x + y
		}
		sum
	"};
	check(src, "10");
}

#[test]
fn for_each_iterable_must_be_array() {
	assert!(fail("loop x in 5 { x }").contains("not iterable"));
}

#[test]
fn for_range_bound_must_be_int() {
	assert!(fail("loop i in 0..true { i }").contains("must be Int"));
}

#[test]
fn for_tuple_pattern_on_non_tuple() {
	assert!(fail("loop (x, y) in [1, 2, 3] { x }").contains("destructure"));
}

#[test]
fn for_tuple_pattern_wrong_field_count() {
	assert!(fail("loop (x, y, z) in [(1, 2)] { x }").contains("fields"));
}
