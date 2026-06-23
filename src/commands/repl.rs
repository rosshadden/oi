use oi::Reported;
use oi::driver::run_source;
use reedline::{DefaultPrompt, DefaultPromptSegment, Reedline, Signal};

pub fn run() -> Result<(), Reported> {
	let mut rl = Reedline::create();
	let prompt = DefaultPrompt::new(
		DefaultPromptSegment::Basic("oi".to_string()),
		DefaultPromptSegment::Empty,
	);

	loop {
		match rl.read_line(&prompt) {
			Ok(Signal::Success(line)) => {
				if line.trim().is_empty() {
					continue;
				}
				let _ = run_source("<repl>", &line, false);
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
