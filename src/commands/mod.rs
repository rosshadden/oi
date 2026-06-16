pub mod exec;
pub mod run;

use oi::Reported;

use crate::cli::Command;

/// Route a parsed command to its handler.
pub fn dispatch(command: Command) -> Result<(), Reported> {
	match command {
		Command::Run { file, debug_ast } => run::run(&file, debug_ast),
		Command::Exec { source, debug_ast } => exec::run(source, debug_ast),
	}
}
