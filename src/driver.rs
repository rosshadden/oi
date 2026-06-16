use chumsky::{input::Stream, prelude::*};

use crate::Reported;
use crate::compiler::Compiler;
use crate::diagnostics::Diagnostic;
use crate::lexer::lex;
use crate::parser::parser;

/// Compile and run a program from its source text.
///
/// `name` labels the source in diagnostics (a file path, or `<exec>` / `<stdin>`).
/// On failure the diagnostic is rendered to stderr.
pub fn run_source(name: &str, src: &str, debug_ast: bool) -> Result<(), Reported> {
	// lex
	let tokens = lex(src);
	let stream = Stream::from_iter(tokens).map((src.len()..src.len()).into(), |(t, s)| (t, s));
	// parse
	let ast = match parser().parse(stream).into_result() {
		Ok(ast) => ast,
		Err(errors) => {
			for e in &errors {
				Diagnostic::from_rich(e).report(name, src);
			}
			return Err(Reported);
		}
	};

	if debug_ast {
		eprintln!("{ast:#?}");
	}

	// compile
	let mut compiler = Compiler::default();
	let code = match compiler.compile(&ast) {
		Ok(code) => code,
		Err(error) => {
			error.report(name, src);
			return Err(Reported);
		}
	};

	// run
	let f = unsafe { std::mem::transmute::<*const u8, fn()>(code) };
	f();
	Ok(())
}
