use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Output, Stdio};

/// Spawn `oi` with `args`, optionally piping `stdin`, and return the raw process output.
pub fn oi(args: &[&str], stdin: Option<&str>) -> Output {
	run(None, args, stdin)
}

/// Spawn `oi` in `dir` instead of the current working directory.
#[allow(dead_code)]
pub fn oi_in(dir: &Path, args: &[&str], stdin: Option<&str>) -> Output {
	run(Some(dir), args, stdin)
}

fn run(dir: Option<&Path>, args: &[&str], stdin: Option<&str>) -> Output {
	let mut cmd = Command::new(env!("CARGO_BIN_EXE_oi"));
	cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
	if let Some(dir) = dir {
		cmd.current_dir(dir);
	}
	if stdin.is_some() {
		cmd.stdin(Stdio::piped());
	}
	let mut child = cmd.spawn().unwrap();
	if let Some(input) = stdin {
		child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
	}
	child.wait_with_output().unwrap()
}

/// Strip a single trailing newline.
pub fn trim_trailing_newline(bytes: &[u8]) -> String {
	let s = String::from_utf8(bytes.to_vec()).unwrap();
	s.strip_suffix('\n').unwrap_or(&s).to_string()
}

/// Assert the process exited successfully and return its trimmed stdout.
pub fn stdout_ok(out: Output) -> String {
	assert!(
		out.status.success(),
		"oi failed:\n{}",
		String::from_utf8_lossy(&out.stderr)
	);
	trim_trailing_newline(&out.stdout)
}
