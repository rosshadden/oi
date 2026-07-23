use std::io::{IsTerminal as _, Read as _};

use oi::Reported;
use oi::driver::run_source;

/// Compile and run source from the argument, piped stdin, or both concatenated.
pub fn run(source: Option<String>) -> Result<(), Reported> {
	let stdin = std::io::stdin();
	let mut src = String::new();
	if source.is_none() || !stdin.is_terminal() {
		stdin.lock().read_to_string(&mut src).map_err(|e| {
			eprintln!("oi: cannot read stdin: {e}");
			Reported
		})?;
	}
	let name = if source.is_some() { "<exec>" } else { "<stdin>" };
	if let Some(arg) = source {
		if !src.is_empty() && !src.ends_with('\n') {
			src.push('\n');
		}
		src.push_str(&arg);
	}
	run_source(name, &src, false)
}
