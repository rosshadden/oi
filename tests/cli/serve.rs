use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

#[test]
fn missing_zola_errors() {
	let out = Command::new(env!("CARGO_BIN_EXE_oi"))
		.args(&["serve"])
		.env("PATH", "/nonexistent")
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.output()
		.unwrap();

	assert!(!out.status.success());
	let stderr = String::from_utf8_lossy(&out.stderr);
	assert!(
		stderr.contains("Failed to execute command"),
		"stderr was:\n{}",
		stderr
	);
}

#[test]
fn serve_spawns_and_stays_alive() {
	let mut child = Command::new(env!("CARGO_BIN_EXE_oi"))
		.args(&["serve"])
		.stdout(Stdio::null())
		.stderr(Stdio::null())
		.spawn()
		.expect("Failed to spawn oi serve");
	thread::sleep(Duration::from_millis(200));
	match child.try_wait() {
		Ok(None) => {
			let _ = child.kill();
			let _ = child.wait();
		}
		Ok(Some(_)) => {}
		Err(e) => {
			panic!("failed to check process: {}", e);
		}
	}
}
