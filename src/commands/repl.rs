use oi::Reported;
use oi::driver::run_source;
use reedline::{DefaultPrompt, DefaultPromptSegment, Reedline, Signal};

pub fn run() -> Result<(), Reported> {
	let mut rl = Reedline::create().use_bracketed_paste(true);
	let mut session = String::new();
	let prompt = DefaultPrompt::new(
		DefaultPromptSegment::Basic("oi".to_string()),
		DefaultPromptSegment::Empty,
	);

	// TODO: add version and whatever else REPLs usually have in the greeting
	eprintln!("Oi! Type :help for help.");

	loop {
		match rl.read_line(&prompt) {
			Ok(Signal::Success(line)) => {
				match line.trim() {
					"" => continue,
					":h" | ":help" => {
						// TODO: print version too
						indoc::eprintdoc! {"
							The Oi REPL.

							Runs code you input as if it were running a script.
							The context persists, but it's just by concatenating all your input together,
							so if you run into any issues `:clear` it away.

							Commands:
								:h, :help: help
								:q, :quit: quit
								:c, :clear: clear session context
						"};
						continue;
					}
					":q" | ":quit" => {
						eprintln!("goodbye");
						break;
					}
					":c" | ":clear" => {
						eprintln!("session cleared");
						session.clear();
						continue;
					}
					_ => {}
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
