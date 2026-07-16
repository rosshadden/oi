use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

#[test]
fn missing_zola_errors() {
	let out = Command::new(env!("CARGO_BIN_EXE_oi"))
		.args(["serve"])
		.env("PATH", "/nonexistent")
		.output()
		.unwrap();
	assert!(!out.status.success());
	assert!(String::from_utf8_lossy(&out.stderr).contains("Failed to execute command"));
}

#[test]
fn serve_spawns_and_stays_alive() {
	let mut child = Command::new(env!("CARGO_BIN_EXE_oi"))
		.args(["serve"])
		.stdout(Stdio::null())
		.stderr(Stdio::null())
		.process_group(0) // own group, so we can kill zola alongside it below
		.spawn()
		.unwrap();
	thread::sleep(Duration::from_millis(200));
	assert!(child.try_wait().unwrap().is_none(), "serve exited early");

	Command::new("kill")
		.args(["-KILL", "--", &format!("-{}", child.id())])
		.status()
		.unwrap();
	child.wait().unwrap();
}
