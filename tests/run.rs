use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use indoc::indoc;

static ID: AtomicUsize = AtomicUsize::new(0);

fn run(src: &str) -> String {
	let n = ID.fetch_add(1, Ordering::Relaxed);
	let path = std::env::temp_dir().join(format!("oi_test_{n}.oi"));
	std::fs::write(&path, src).unwrap();
	let out = exec(&path);
	std::fs::remove_file(&path).ok();
	out
}

#[allow(dead_code)]
fn run_file(name: &str) -> String {
	let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
		.join("tests/cases")
		.join(name);
	exec(&path)
}

fn exec(path: &Path) -> String {
	let out = Command::new(env!("CARGO_BIN_EXE_oi"))
		.arg(path)
		.output()
		.unwrap();
	assert!(
		out.status.success(),
		"compiler failed:\n{}",
		String::from_utf8_lossy(&out.stderr)
	);
	let s = String::from_utf8(out.stdout).unwrap();
	s.strip_suffix('\n').unwrap_or(&s).to_string()
}

fn check(src: &str, expected: &str) {
	assert_eq!(run(src), expected, "\nsrc:\n{src}");
}

// literals

#[test]
fn int_literal() {
	check("42", "42");
}

#[test]
fn float_literal() {
	check("3.14", "3.14");
}

#[test]
fn bool_true() {
	check("true", "true");
}

#[test]
fn bool_false() {
	check("false", "false");
}

#[test]
fn string_literal() {
	check(r#""hello""#, "hello");
}

// arithmetic

#[test]
fn int_add() {
	check("2 + 3", "5");
}

#[test]
fn int_sub() {
	check("10 - 4", "6");
}

#[test]
fn int_mul() {
	check("3 * 4", "12");
}

#[test]
fn int_div() {
	check("10 / 3", "3");
}

#[test]
fn float_add() {
	check("1.5 + 2.0", "3.5");
}

#[test]
fn negation() {
	check("-5", "-5");
}

// strings

#[test]
fn string_concat() {
	check(r#""foo" + "bar""#, "foobar");
}

// variables

#[test]
fn variable() {
	check("x := 42\nx", "42");
}

// functions

#[test]
fn fn_call() {
	let src = indoc! {"
		fn double() { 21 * 2 }
		double()
	"};
	check(src, "42");
}

#[test]
fn multi_fn() {
	let src = indoc! {"
		fn base() {
			6
		}

		fn triple() {
			base() + base() + base()
		}

		triple()
	"};
	check(src, "18");
}

#[test]
fn fn_vars() {
	let src = indoc! {"
		fn area() {
			width := 12
			height := 5
			width * height
		}

		area()
	"};
	check(src, "60");
}

// statements

#[test]
fn stmts() {
	let src = indoc! {"
		x := 3
		y := x * x
		z := y + x
		z
	"};
	check(src, "12");
}
