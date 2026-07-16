use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// The Oi CLI.
#[derive(Parser)]
#[command(name = "oi", version, about)]
pub struct Cli {
	#[command(subcommand)]
	pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
	/// Compile and run an Oi program.
	Run {
		/// Path to the .oi source file.
		#[arg(default_value = "main.oi")]
		file: PathBuf,

		/// Dump the parsed AST to stderr.
		#[arg(long)]
		debug_ast: bool,
	},

	/// Compile and run an Oi script.
	Exec {
		/// Source to run. If omitted, read from stdin.
		#[arg(allow_hyphen_values = true)]
		source: Option<String>,
	},

	/// Start an interactive Oi REPL.
	Repl,
}
