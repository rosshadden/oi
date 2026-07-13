use crate::helpers::*;
use indoc::indoc;

#[test]
fn construct_some() {
	check("?int(42)", "some");
}

#[test]
fn construct_none() {
	check("?int(none)", "none");
}

#[test]
fn zero_value_is_none() {
	check("mut o ?int\no", "none");
}

#[test]
fn bare_none_without_context_errors() {
	let err = fail("none");
	assert!(err.contains("cannot infer the type"), "got: {err}");
}

#[test]
fn int_cast_gives_tag() {
	check("int(?int(42))", "1");
	check("int(?int(none))", "0");
}

#[test]
fn eq_same_some() {
	check("?int(42) == ?int(42)", "true");
}

#[test]
fn eq_different_some() {
	check("?int(42) == ?int(7)", "false");
}

#[test]
fn eq_none_vs_some() {
	check("?int(none) == ?int(42)", "false");
	check("?int(none) != ?int(42)", "true");
}

#[test]
fn field_type_mismatch() {
	let err = fail("?int(3.0)");
	assert!(err.contains("expected int, got float"), "got: {err}");
}

#[test]
fn ordering_rejected() {
	let err = fail("?int(1) < ?int(2)");
	assert!(err.contains("only `==`&`!=`"), "got: {err}");
}

#[test]
fn match_binds_some() {
	check(
		indoc! {r#"
			o := ?int(42)
			match o {
				.some(n) { n }
				.none { -1 }
			}
		"#},
		"42",
	);
}

#[test]
fn match_none_arm() {
	check(
		indoc! {r#"
			o := ?int(none)
			match o {
				.some(n) { n }
				.none { -1 }
			}
		"#},
		"-1",
	);
}

#[test]
fn match_non_exhaustive_errors() {
	let err = fail("o := ?int(42)\nmatch o {\n\t.some(n) { n }\n}");
	assert!(err.contains("non-exhaustive match, missing: none"), "got: {err}");
}
