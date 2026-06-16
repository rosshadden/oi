use crate::helpers::{exec_arg, exec_stdin};

#[test]
fn exec_arg_arithmetic() {
	assert_eq!(exec_arg("2 + 3 * 4"), "14");
}

#[test]
fn exec_arg_string_concat() {
	assert_eq!(exec_arg(r#""a" + "b""#), "ab");
}

#[test]
fn exec_stdin_arithmetic() {
	assert_eq!(exec_stdin("1 + 2"), "3");
}
