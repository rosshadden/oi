use crate::support::{oi, oi_in};

#[test]
fn missing_file_errors() {
	let out = oi(&["run", "definitely-missing.oi"], None);
	assert!(!out.status.success());
	let stderr = String::from_utf8_lossy(&out.stderr);
	assert!(stderr.contains("cannot read"), "stderr was:\n{stderr}");
}

#[test]
fn default_file_is_main_oi_in_cwd() {
	// with no path, `run` runs ./main.oi in the current directory
	let dir = std::env::temp_dir().join(format!("oi_run_default_{}", std::process::id()));
	std::fs::create_dir_all(&dir).unwrap();
	std::fs::write(dir.join("main.oi"), "1 + 2").unwrap();
	let out = oi_in(&dir, &["run"], None);
	std::fs::remove_dir_all(&dir).ok();
	assert!(
		out.status.success(),
		"stderr:\n{}",
		String::from_utf8_lossy(&out.stderr)
	);
	assert_eq!(String::from_utf8(out.stdout).unwrap().trim(), "3");
}

#[test]
fn debug_ast_goes_to_stderr() {
	// --debug-ast dumps the AST to stderr
	let plain = oi(&["run", "examples/main.oi"], None);
	assert!(plain.status.success());
	assert!(
		plain.stderr.is_empty(),
		"unexpected stderr:\n{}",
		String::from_utf8_lossy(&plain.stderr)
	);

	let dumped = oi(&["run", "examples/main.oi", "--debug-ast"], None);
	assert!(dumped.status.success());
	assert!(!dumped.stderr.is_empty(), "expected the AST dump on stderr");
	assert_eq!(dumped.stdout, plain.stdout);
}
