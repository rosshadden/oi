use std::process::Command;

use oi::Reported;

/// Generate and run static site.
pub fn run() -> Result<(), Reported> {
	match Command::new("zola")
		.args(["--root", "www", "serve", "--base-url", "localhost"])
		.spawn()
	{
		Ok(mut child) => {
			let _ = child.wait().expect("Command wasn't running.");
			Ok(())
		}
		Err(e) => {
			eprintln!("Failed to execute command: {}", e);
			Err(Reported)
		}
	}
}
