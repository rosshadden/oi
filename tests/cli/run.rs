use crate::support::oi;

#[test]
fn missing_file_errors() {
	let out = oi(&["run", "definitely-missing.oi"], None);
	assert!(!out.status.success());
	let stderr = String::from_utf8_lossy(&out.stderr);
	assert!(stderr.contains("cannot read"), "stderr was:\n{stderr}");
}

#[test]
fn default_file_runs() {
	// With no path, `run` falls back to examples/main.oi, resolved from the
	// package root (where cargo runs integration tests).
	let out = oi(&["run"], None);
	assert!(
		out.status.success(),
		"stderr:\n{}",
		String::from_utf8_lossy(&out.stderr)
	);
	assert!(!String::from_utf8(out.stdout).unwrap().trim().is_empty());
}
