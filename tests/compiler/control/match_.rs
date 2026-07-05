use crate::helpers::*;

#[test]
fn match_string() {
	let src = indoc! {r#"
		os := "linux"
		match os {
			"darwin" { "macOS" }
			"linux" { "Linux" }
			else { "other" }
		}
	"#};
	check(src, "Linux");
}

#[test]
fn match_int() {
	let src = indoc! {"
		x := 2
		match x {
			1 { \"one\" }
			2 { \"two\" }
			else { \"other\" }
		}
	"};
	check(src, "two");
}

#[test]
fn match_else_taken() {
	let src = indoc! {r#"
		x := 99
		match x {
			1 { "one" }
			else { "other" }
		}
	"#};
	check(src, "other");
}

#[test]
fn match_no_else_hit() {
	let src = indoc! {r#"
		match "linux" {
			"linux" { "penguin" }
		}
	"#};
	check(src, "penguin");
}

// no else and no match yields the zero value of the result type
#[test]
fn match_no_else_miss_int() {
	check("match 5 { 1 { 10 } }", "0");
}

#[test]
fn match_no_else_miss_string() {
	check(r#"match "x" { "y" { "yes" } }"#, "");
}

#[test]
fn match_as_binding() {
	let src = indoc! {r#"
		label := match 2 {
			1 { "one" }
			2 { "two" }
			else { "other" }
		}
		label
	"#};
	check(src, "two");
}

#[test]
fn match_int_expr_value() {
	let src = indoc! {"
		1 + match 3 {
			1 { 10 }
			3 { 30 }
			else { 0 }
		}
	"};
	check(src, "31");
}

// comma-separated patterns within one arm are OR'd
#[test]
fn match_or_patterns() {
	let src = indoc! {r#"
		os := "macos"
		match os {
			"darwin", "macos" { "Apple" }
			"linux" { "Linux" }
			else { "other" }
		}
	"#};
	check(src, "Apple");
}

#[test]
fn match_or_patterns_second() {
	let src = indoc! {r#"
		os := "darwin"
		match os {
			"darwin", "macos" { "Apple" }
			else { "other" }
		}
	"#};
	check(src, "Apple");
}

#[test]
fn match_bool() {
	check("match true { true { 1 } false { 2 } }", "1");
}

// `match true { cond { } }` works as an if-else chain
#[test]
fn match_true_as_if_chain() {
	let src = indoc! {"
		x := 7
		match true {
			x < 5 { \"small\" }
			x < 10 { \"medium\" }
			else { \"large\" }
		}
	"};
	check(src, "medium");
}

#[test]
fn match_wildcard() {
	check(r#"match 5 { 1 { "one" } _ { "other" } }"#, "other");
	let src = indoc! {r#"
		enum Color { red green blue }
		match Color.blue {
			.red { 1 }
			_ { 9 }
		}
	"#};
	check(src, "9");
}

#[test]
fn match_range() {
	let src = indoc! {r#"
		age := 18
		match age {
			0..18 { "minor" }
			18..65 { "adult" }
			_ { "senior" }
		}
	"#};
	check(src, "adult");
}

#[test]
fn match_binding() {
	check(r#"match 15 { n @ 0..18 { n } _ { 0 } }"#, "15");
}

#[test]
fn match_range_needs_int_subject() {
	assert!(fail(r#"match "s" { 0..5 { 1 } _ { 2 } }"#).contains("integer subject"));
}

#[test]
fn match_tuple_destructure() {
	check("p := (3, 4)\nmatch p { (x, y) { x + y } }", "7");
}

#[test]
fn match_tuple_arity_mismatch() {
	assert!(fail("match (1, 2) { (a, b, c) { a } }").contains("tuple pattern has 3"));
}

#[test]
fn match_struct_destructure() {
	let src = indoc! {r#"
		struct Point { x int, y int }
		p := Point{ x: 3, y: 4 }
		match p {
			Point{ y: b, x } { x + b }
		}
	"#};
	check(src, "7");
}

#[test]
fn match_struct_unknown_field() {
	let src = indoc! {r#"
		struct Point { x int, y int }
		match Point{ x: 1, y: 2 } {
			Point { z } { z }
		}
	"#};
	assert!(fail(src).contains("no field `z`"));
}

#[test]
fn match_array_destructure() {
	let src = indoc! {r#"
		a := [3, 4]
		match a {
			[x, y] { x + y }
		}
	"#};
	check(src, "7");
}

#[test]
fn match_array_length_guard() {
	let src = indoc! {r#"
		a := [1, 2, 3]
		match a {
			[x, y] { 0 }
			_ { 99 }
		}
	"#};
	check(src, "99");
}

#[test]
fn match_enum_non_exhaustive() {
	let src = indoc! {r#"
		enum Color { red green blue }
		c := Color.red
		match c {
			.red { 1 }
			.green { 2 }
		}
	"#};
	assert!(fail(src).contains("non-exhaustive match, missing: blue"));
}

// arms must yield the same type
#[test]
fn match_mismatched_arm_types() {
	assert!(fail(r#"match 1 { 1 { "str" } else { 2 } }"#).contains("mismatched types"));
}

// pattern type must match subject type
#[test]
fn match_pattern_type_mismatch() {
	assert!(fail(r#"match 1 { "str" { 1 } }"#).contains("type mismatch"));
}
