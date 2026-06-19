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
