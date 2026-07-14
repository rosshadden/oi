use crate::helpers::*;

#[test]
fn question_unwraps_some() {
	let src = indoc! {"
		fn find(id int) ?int {
			if id == 7 { return 42 }
			return none
		}
		fn display(id int) ?int {
			v := find(id)?
			v + 1
		}
		display(7) or { -1 }
	"};
	check(src, "43");
}

#[test]
fn question_propagates_none() {
	let src = indoc! {"
		fn find(id int) ?int {
			if id == 7 { return 42 }
			return none
		}
		fn display(id int) ?int {
			v := find(id)?
			v + 1
		}
		display(1) or { -1 }
	"};
	check(src, "-1");
}

#[test]
fn bang_unwraps_ok() {
	let src = indoc! {r#"
		fn load(path string) !int {
			if path == "ok" { return 42 }
			return error("missing")
		}
		fn double(path string) !int {
			v := load(path)!
			v * 2
		}
		double("ok") or { -1 }
	"#};
	check(src, "84");
}

#[test]
fn bang_propagates_error() {
	let src = indoc! {r#"
		fn load(path string) !int {
			if path == "ok" { return 42 }
			return error("missing")
		}
		fn double(path string) !int {
			v := load(path)!
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
fn question_infers_enclosing_return_type() {
	let src = indoc! {"
		fn find(id int) ?int {
			if id == 7 { return 42 }
			return none
		}
		fn display(id int) {
			v := find(id)?
			v + 1
		}
		display(7) or { -1 }
	"};
	check(src, "43");
}

#[test]
fn requires_option_or_result() {
	let err = fail("fn f() int { 42? }\nf()");
	assert!(err.contains("`?` needs a `?T` value"), "got: {err}");
}

#[test]
fn option_panics_in_main() {
	let src = indoc! {"
		fn find(id int) ?int {
			if id == 7 { return 42 }
			return none
		}
		find(1)?
	"};
	let err = fail(src);
	assert!(err.contains("panic: unwrapped `none`"), "got: {err}");
}

#[test]
fn result_panics_in_main() {
	let src = indoc! {r#"
		fn load(path string) !int {
			if path == "ok" { return 42 }
			return error("missing")
		}
		load("nope")!
	"#};
	let err = fail(src);
	assert!(err.contains("panic: missing"), "got: {err}");
}

#[test]
fn requires_matching_enclosing_return() {
	let src = indoc! {"
		fn find(id int) ?int {
			if id == 7 { return 42 }
			return none
		}
		fn display(id int) int {
			find(id)?
		}
		display(7)
	"};
	let err = fail(src);
	assert!(err.contains("needs an enclosing fn returning `?T`"), "got: {err}");
}
