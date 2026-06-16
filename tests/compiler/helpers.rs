use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) use indoc::indoc;

static ID: AtomicUsize = AtomicUsize::new(0);

/// Run compiler.
fn exec(path: &Path) -> String {
	let out = Command::new(env!("CARGO_BIN_EXE_oi"))
		.arg("run")
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

/// Run provided source.
pub(crate) fn run(src: &str) -> String {
	let n = ID.fetch_add(1, Ordering::Relaxed);
	let path = std::env::temp_dir().join(format!("oi_test_{n}.oi"));
	std::fs::write(&path, src).unwrap();
	let out = exec(&path);
	std::fs::remove_file(&path).ok();
	out
}

/// Run provided file.
#[allow(dead_code)]
pub(crate) fn run_file(name: &str) -> String {
	let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
		.join("tests/cases")
		.join(name);
	exec(&path)
}

/// Run provided source expecting a compilation error.
pub(crate) fn fail(src: &str) -> String {
	let n = ID.fetch_add(1, Ordering::Relaxed);
	let path = std::env::temp_dir().join(format!("oi_test_{n}.oi"));
	std::fs::write(&path, src).unwrap();
	let out = Command::new(env!("CARGO_BIN_EXE_oi"))
		.arg("run")
		.arg(&path)
		.output()
		.unwrap();
	std::fs::remove_file(&path).ok();
	assert!(
		!out.status.success(),
		"expected failure but compiler succeeded\nsrc:\n{src}\nstdout:\n{}",
		String::from_utf8_lossy(&out.stdout)
	);
	String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Run provided source expecting a given result.
pub(crate) fn check(src: &str, expected: &str) {
	assert_eq!(run(src), expected, "\nsrc:\n{src}");
}

/// Run source via `oi exec <src>`.
pub(crate) fn exec_arg(src: &str) -> String {
	let out = Command::new(env!("CARGO_BIN_EXE_oi"))
		.arg("exec")
		.arg(src)
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

/// Run source piped to `oi exec` via stdin.
pub(crate) fn exec_stdin(src: &str) -> String {
	let mut child = Command::new(env!("CARGO_BIN_EXE_oi"))
		.arg("exec")
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();
	child
		.stdin
		.take()
		.unwrap()
		.write_all(src.as_bytes())
		.unwrap();
	let out = child.wait_with_output().unwrap();
	assert!(
		out.status.success(),
		"compiler failed:\n{}",
		String::from_utf8_lossy(&out.stderr)
	);
	let s = String::from_utf8(out.stdout).unwrap();
	s.strip_suffix('\n').unwrap_or(&s).to_string()
}
