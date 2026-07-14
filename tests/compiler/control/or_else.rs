use crate::helpers::*;

#[test]
fn fallback_on_none() {
	check("?int(none) or { -1 }", "-1");
}

#[test]
fn fallback_on_err() {
	check(r#"!int(error("oops")) or { -1 }"#, "-1");
}

#[test]
fn unwraps_some() {
	check("?int(42) or { -1 }", "42");
}

#[test]
fn unwraps_ok() {
	check("!int(42) or { -1 }", "42");
}

#[test]
fn skips_fallback_body_when_ok() {
	let src = indoc! {r#"
		!int(42) or {
			print("ran")
			9
		}
	"#};
	check(src, "42");
}

#[test]
fn dollar_is_error_message() {
	let src = indoc! {r#"
		!int(error("boom")) or {
			print($)
			9
		}
	"#};
	check(src, "boom\n9");
}

#[test]
fn as_binding() {
	check("x := ?int(none) or { 99 }\nx", "99");
}

#[test]
fn fallback_can_diverge() {
	let src = indoc! {"
		fn unwrap_or_bail(o ?int) int {
			v := o or { return -1 }
			v
		}
		unwrap_or_bail(?int(none))
	"};
	check(src, "-1");

	let src = indoc! {"
		fn unwrap_or_bail(o ?int) int {
			v := o or { return -1 }
			v
		}
		unwrap_or_bail(?int(42))
	"};
	check(src, "42");
}

#[test]
fn type_mismatch_errors() {
	let err = fail(r#"?int(42) or { "wrong" }"#);
	assert!(
		err.contains("or` branches have mismatched types: int and str"),
		"got: {err}"
	);
}

#[test]
fn requires_option_or_result() {
	let err = fail("42 or { 0 }");
	assert!(err.contains("needs a `?T`/`!T` value"), "got: {err}");
}
