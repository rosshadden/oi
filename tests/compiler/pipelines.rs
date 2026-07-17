use crate::helpers::*;

#[test]
fn threads_through_bare_fn_step() {
	let src = indoc! {"
		fn double(x int) int { x * 2 }
		3 |> double
	"};
	check(src, "6");
}

#[test]
fn threads_through_multiple_steps() {
	let src = indoc! {"
		fn double(x int) int { x * 2 }
		fn inc(x int) int { x + 1 }
		3 |> double |> inc |> double
	"};
	check(src, "14");
}

#[test]
fn dollar_expression_step() {
	check("3 |> $ + 1", "4");
}

#[test]
fn pipe_is_loosest() {
	let src = indoc! {"
		fn double(x int) int { x * 2 }
		1 + 1 |> double
	"};
	check(src, "4");
}

#[test]
fn if_expression_step() {
	check(r#"5 |> if $ > 3 { "big" } else { "small" }"#, "big");
}

#[test]
fn question_step_unwraps_some() {
	let src = indoc! {"
		fn find(id int) ?int {
			if id == 7 { return 42 }
			return none
		}
		fn display(id int) {
			v := id |> find?
			v + 1
		}
		display(7) or { -1 }
	"};
	check(src, "43");
}

#[test]
fn question_step_propagates_none() {
	let src = indoc! {"
		fn find(id int) ?int {
			if id == 7 { return 42 }
			return none
		}
		fn display(id int) {
			v := id |> find?
			v + 1
		}
		display(1) or { -1 }
	"};
	check(src, "-1");
}

#[test]
fn bang_step_unwraps_ok() {
	let src = indoc! {r#"
		fn load(path string) !int {
			if path == "ok" { return 42 }
			return error("missing")
		}
		fn double(path string) !int {
			v := path |> load!
			v * 2
		}
		double("ok") or { -1 }
	"#};
	check(src, "84");
}

#[test]
fn bang_step_propagates_error() {
	let src = indoc! {r#"
		fn load(path string) !int {
			if path == "ok" { return 42 }
			return error("missing")
		}
		fn double(path string) !int {
			v := path |> load!
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
fn or_tail_after_chain() {
	let src = indoc! {"
		fn find(id int) ?int {
			if id == 7 { return 42 }
			return none
		}
		7 |> find or { -1 }
	"};
	check(src, "42");
}

#[test]
fn or_tail_after_chain_fallback() {
	let src = indoc! {"
		fn find(id int) ?int {
			if id == 7 { return 42 }
			return none
		}
		1 |> find or { -1 }
	"};
	check(src, "-1");
}
