use oi::Reported;
use oi::driver::run_source;
use reedline::{DefaultPrompt, DefaultPromptSegment, Reedline, Signal};

pub fn run() -> Result<(), Reported> {
	let mut rl = Reedline::create();
	let mut session = String::new();
	let prompt = DefaultPrompt::new(
		DefaultPromptSegment::Basic("oi".to_string()),
		DefaultPromptSegment::Empty,
	);

	loop {
		match rl.read_line(&prompt) {
			Ok(Signal::Success(line)) => {
				match line.trim() {
					s if s.is_empty() => { continue },
					"/q" | "/quit" => {
						println!("goodbye");
						break
					},
					"/c" | "/clear" => {
						session.clear();
						continue
					},
					_ => {},
				}

				let candidate = format!("{session}{line}\n");
				if run_source("<repl>", &candidate, false).is_ok() {
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
