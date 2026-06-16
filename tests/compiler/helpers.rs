use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) use indoc::indoc;

use crate::support::{oi, stdout_ok};

static ID: AtomicUsize = AtomicUsize::new(0);

/// Run `oi run <path>` and return trimmed stdout.
fn exec(path: &Path) -> String {
	let p = path.to_string_lossy();
	stdout_ok(oi(&["run", p.as_ref()], None))
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
	let p = path.to_string_lossy();
	let out = oi(&["run", p.as_ref()], None);
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
