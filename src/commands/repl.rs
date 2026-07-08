use std::process::Command;

use oi::Reported;
use reedline::{DefaultPrompt, DefaultPromptSegment, Reedline, Signal};

pub fn run() -> Result<(), Reported> {
	let mut rl = Reedline::create();
	let prompt = DefaultPrompt::new(
		DefaultPromptSegment::Basic("oi".to_string()),
		DefaultPromptSegment::Empty,
	);
	let mut session = String::new();

	loop {
		match rl.read_line(&prompt) {
			Ok(Signal::Success(line)) => {
				if line.trim().is_empty() {
					continue;
				}
				let candidate = format!("{session}{line}\n");
				if eval(&candidate) {
					session = candidate;
				}
			}
			Ok(Signal::CtrlC) => continue,
			Ok(Signal::CtrlD) => break,
			Ok(_) => {}
			Err(e) => {
				eprintln!("oi: {e}");
				break;
			}
		}
	}

	Ok(())
}

// Run `src` as a fresh process.
fn eval(src: &str) -> bool {
	Command::new(std::env::current_exe().expect("current executable path"))
		.args(["exec", src])
		.status()
		.expect("spawn `oi exec`")
		.success()
}
