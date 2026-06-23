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
