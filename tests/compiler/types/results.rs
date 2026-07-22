use crate::helpers::*;
use indoc::indoc;

#[test]
fn construct_ok() {
	check("!int(42)", "ok");
}

#[test]
fn construct_err() {
	check(r#"!int(error("oops"))"#, "err");
}

#[test]
fn zero_value_is_ok() {
	check("mut r !int\nr", "ok");
}

#[test]
fn int_cast_gives_tag() {
	check("int(!int(42))", "0");
	check(r#"int(!int(error("oops")))"#, "1");
}

#[test]
fn eq_same_ok() {
	check("!int(42) == !int(42)", "true");
}

#[test]
fn eq_different_ok() {
	check("!int(42) == !int(7)", "false");
}

#[test]
fn eq_ok_vs_err() {
	check(r#"!int(42) == !int(error("oops"))"#, "false");
	check(r#"!int(42) != !int(error("oops"))"#, "true");
}

#[test]
fn field_type_mismatch() {
	let err = fail("!int(3.0)");
	assert!(err.contains("expected int or Error, got float"), "got: {err}");
}

#[test]
fn ordering_rejected() {
	let err = fail("!int(1) < !int(2)");
	assert!(err.contains("only `==`&`!=`"), "got: {err}");
}

#[test]
fn match_binds_ok() {
	check(
		indoc! {r#"
			r := !int(42)
			match r {
				.ok(n) => n,
				.err(e) => -1,
			}
		"#},
		"42",
	);
}

#[test]
fn match_err_arm() {
	check(
		indoc! {r#"
			r := !int(error("oops"))
			match r {
				.ok(n) => n,
				.err(e) => -1,
			}
		"#},
		"-1",
	);
}

#[test]
fn match_non_exhaustive_errors() {
	let err = fail("r := !int(42)\nmatch r {\n\t.ok(n) => n,\n}");
	assert!(err.contains("non-exhaustive match, missing: err"), "got: {err}");
}

#[test]
fn struct_field_type() {
	check(
		"struct Box { val !int }
		b := Box{ val: !int(42) }
		b.val",
		"ok",
	);
}

#[test]
fn fn_param_type() {
	let src = indoc! {"
		fn unwrap_or(r !int, fallback int) int {
			match r {
				.ok(n) => n,
				.err(e) => fallback,
			}
		}
		unwrap_or(!int(42), 0)
	"};
	check(src, "42");
}

#[test]
fn bare_value_return_wraps_ok() {
	let src = indoc! {"
		fn find(x int) !int {
			return x
		}
		find(5)
	"};
	check(src, "ok");
}

#[test]
fn bare_error_return_wraps_err() {
	let src = indoc! {r#"
		fn find(x int) !int {
			return error("not found")
		}
		find(5)
	"#};
	check(src, "err");
}

#[test]
fn error_message() {
	check(r#"error("oops").message()"#, "oops");
}

#[test]
fn error_message_via_dollar() {
	let src = indoc! {r#"
		!int(error("boom")) or {
			print($.message())
			0
		}
	"#};
	check(src, "boom\n0");
}

#[test]
fn error_unknown_method() {
	let err = fail(r#"error("oops").code()"#);
	assert!(err.contains("`Error` has no method `code`"), "got: {err}");
}

#[test]
fn long_form_matches_shorthand() {
	let src = indoc! {r#"
		fn load(path string) Result[int, Error] {
			if path == "ok" { return 42 }
			return error("missing")
		}
		fn double(path string) Result[int, Error] {
			v := load(path)?
			v * 2
		}
		double("ok") or { -1 }
	"#};
	check(src, "84");
	let src = indoc! {r#"
		fn load(path string) Result[int, Error] {
			if path == "ok" { return 42 }
			return error("missing")
		}
		fn double(path string) Result[int, Error] {
			v := load(path)?
			v * 2
		}
		double("nope") or {
			print($)
			0
		}
	"#};
	check(src, "missing\n0");
}

#[test]
fn long_form_nested() {
	let src = indoc! {r#"
		fn load() Result[[]int, Error] {
			return [1, 2, 3]
		}
		load() or { [-1] }
	"#};
	check(src, "[1, 2, 3]");
}

#[test]
fn long_form_rejects_custom_error() {
	let err = fail("fn load() Result[int, MyError] { 42 }\nload()");
	assert!(err.contains("custom error types aren't supported yet"), "got: {err}");
}
