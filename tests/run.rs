use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static ID: AtomicUsize = AtomicUsize::new(0);

// Run an inline Oi snippet and return its stdout.
fn run(src: &str) -> String {
	let n = ID.fetch_add(1, Ordering::Relaxed);
	let path = std::env::temp_dir().join(format!("oi_test_{n}.oi"));
	std::fs::write(&path, src).unwrap();
	let out = exec(&path);
	std::fs::remove_file(&path).ok();
	out
}

// Run a file from tests/cases/ and return its stdout.
fn run_file(name: &str) -> String {
	let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
		.join("tests/cases")
		.join(name);
	exec(&path)
}

// Test runner.
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
	String::from_utf8(out.stdout).unwrap()
}

// tests

#[test]
fn int_literal() {
	assert_eq!(run("42"), "42\n");
}

#[test]
fn float_literal() {
	assert_eq!(run("3.14"), "3.14\n");
}

#[test]
fn bool_literal() {
	assert_eq!(run("true"), "true\n");
	assert_eq!(run("false"), "false\n");
}

#[test]
fn string_literal() {
	assert_eq!(run(r#""hello""#), "hello\n");
}

#[test]
fn int_arithmetic() {
	assert_eq!(run("2 + 3"), "5\n");
	assert_eq!(run("10 - 4"), "6\n");
	assert_eq!(run("3 * 4"), "12\n");
	assert_eq!(run("10 / 3"), "3\n");
}

#[test]
fn float_arithmetic() {
	assert_eq!(run("1.5 + 2.0"), "3.5\n");
}

#[test]
fn negation() {
	assert_eq!(run("-5"), "-5\n");
}

#[test]
fn string_concat() {
	assert_eq!(run(r#""foo" + "bar""#), "foobar\n");
}

#[test]
fn variable() {
	assert_eq!(run("x := 42\nx"), "42\n");
}

#[test]
fn fn_call() {
	assert_eq!(run("fn double() { 21 * 2 }\ndouble()"), "42\n");
}

#[test]
fn multi_fn() {
	assert_eq!(
		run(r#"
fn base() {
	6
}

fn triple() {
	base() + base() + base()
}

triple()
		"#),
		"18\n"
	);
}

#[test]
fn fn_vars() {
	assert_eq!(
		run(r#"
fn area() {
	width := 12
	height := 5
	width * height
}

area()
		"#),
		"60\n"
	);
}

#[test]
fn stmts() {
	assert_eq!(
		run("x := 3\ny := x * x\nz := y + x\nz"),
		"12\n"
	);
}
