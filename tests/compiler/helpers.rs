use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) use indoc::indoc;

static ID: AtomicUsize = AtomicUsize::new(0);

/// Run provided Oi source.
pub(crate) fn run(src: &str) -> String {
	let n = ID.fetch_add(1, Ordering::Relaxed);
	let path = std::env::temp_dir().join(format!("oi_test_{n}.oi"));
	std::fs::write(&path, src).unwrap();
	let out = exec(&path);
	std::fs::remove_file(&path).ok();
	out
}

/// Run provided Oi file.
#[allow(dead_code)]
pub(crate) fn run_file(name: &str) -> String {
	let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
		.join("tests/cases")
		.join(name);
	exec(&path)
}

/// Run compiler.
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

/// Check that provide Oi source produces the expected result.
pub(crate) fn check(src: &str, expected: &str) {
	assert_eq!(run(src), expected, "\nsrc:\n{src}");
}
