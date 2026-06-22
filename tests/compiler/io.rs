use crate::helpers::*;

#[test]
fn print_string() {
	check(r#"print("hello")"#, "hello\n0");
}

#[test]
fn print_int() {
	check("print(42)", "42\n0");
}

#[test]
fn print_bool() {
	check("print(true)", "true\n0");
}

#[test]
fn print_multiple() {
	check(r#"print("a", "b", "c")"#, "a b c\n0");
}

#[test]
fn print_mixed_types() {
	check(r#"print("n =", 7)"#, "n = 7\n0");
}

#[test]
fn print_as_statement() {
	check(
		r#"print("hello")
42"#,
		"hello\n42",
	);
}

#[test]
fn print_no_args() {
	assert!(fail("print()").contains("at least 1 argument"));
}

#[test]
fn write_no_newline() {
	check(r#"write("hi")
42"#, "hi42");
}

#[test]
fn write_multiple() {
	check(r#"write("a", "b")
42"#, "a b42");
}

#[test]
fn write_no_args() {
	assert!(fail("write()").contains("at least 1 argument"));
}

#[test]
fn eprint_goes_to_stderr() {
	let (stdout, stderr) = run_streams(r#"eprint("err")
42"#);
	assert_eq!(stdout, "42");
	assert_eq!(stderr, "err");
}

#[test]
fn ewrite_goes_to_stderr() {
	let (stdout, stderr) = run_streams(r#"ewrite("err")
42"#);
	assert_eq!(stdout, "42");
	assert_eq!(stderr, "err");
}

#[test]
fn eprint_no_args() {
	assert!(fail("eprint()").contains("at least 1 argument"));
}

#[test]
fn ewrite_no_args() {
	assert!(fail("ewrite()").contains("at least 1 argument"));
}
