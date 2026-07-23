use crate::support::{oi, stdout_ok};

fn exec_arg(src: &str) -> String {
	stdout_ok(oi(&["exec", src], None))
}

fn exec_stdin(src: &str) -> String {
	stdout_ok(oi(&["exec"], Some(src)))
}

#[test]
fn arg_arithmetic() {
	assert_eq!(exec_arg("2 + 3 * 4"), "14");
}

#[test]
fn arg_string_concat() {
	assert_eq!(exec_arg(r#""a" + "b""#), "ab");
}

#[test]
fn arg_leading_hyphen() {
	// `allow_hyphen_values` lets source starting with `-` through as the arg.
	assert_eq!(exec_arg("-5 + 8"), "3");
}

#[test]
fn stdin_arithmetic() {
	assert_eq!(exec_stdin("1 + 2"), "3");
}

#[test]
fn stdin_and_arg_concatenate() {
	assert_eq!(stdout_ok(oi(&["exec", "x + 1"], Some("x := 41"))), "42");
}

#[test]
fn error_names_exec_source() {
	let out = oi(&["exec", "2 +"], None);
	assert!(!out.status.success());
	let stderr = String::from_utf8_lossy(&out.stderr);
	assert!(stderr.contains("<exec>"), "stderr was:\n{stderr}");
}
