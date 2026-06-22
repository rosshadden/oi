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
