use std::path::Path;

use oi::Reported;
use oi::driver::run_source;

/// Read a source file, then compile and run it.
pub fn run(file: &Path, debug_ast: bool) -> Result<(), Reported> {
	let src = std::fs::read_to_string(file).map_err(|e| {
		eprintln!("oi: cannot read {}: {e}", file.display());
		Reported
	})?;
	run_source(&file.display().to_string(), &src, debug_ast)
}
