use std::io::IsTerminal;
use std::ops::Range;

use ariadne::{Color, Config, IndexType, Label, Report, ReportKind, Source};
use chumsky::error::{Rich, RichReason};

use crate::lexer::Token;

// A user-facing error rendered with ariadne.
pub struct Diagnostic {
	message: String,
	span: Range<usize>,
	label: Option<String>,
	note: Option<String>,
}

impl Diagnostic {
	pub fn new(message: impl Into<String>, span: Range<usize>) -> Self {
		Self {
			message: message.into(),
			span,
			label: None,
			note: None,
		}
	}

	pub fn with_label(mut self, label: impl Into<String>) -> Self {
		self.label = Some(label.into());
		self
	}

	pub fn with_note(mut self, note: impl Into<String>) -> Self {
		self.note = Some(note.into());
		self
	}

	// Build a diagnostic from a chumsky parse error.
	pub fn from_rich(err: &Rich<'_, Token>) -> Self {
		let label = match err.reason() {
			RichReason::Custom(_) => "here",
			RichReason::ExpectedFound { found: None, .. } => "unexpected end of input",
			RichReason::ExpectedFound { .. } => "unexpected token",
		};
		Self::new(err.reason().to_string(), err.span().into_range()).with_label(label)
	}

	// Render span to stderr.
	pub fn report(&self, filename: &str, src: &str) {
		let id = filename.to_string();
		let color = std::io::stderr().is_terminal();
		let config = Config::default()
			.with_color(color)
			.with_index_type(IndexType::Byte);

		let mut builder = Report::build(ReportKind::Error, (id.clone(), self.span.clone()))
			.with_config(config)
			.with_message(&self.message)
			.with_label(
				Label::new((id.clone(), self.span.clone()))
					.with_message(self.label.as_deref().unwrap_or("here"))
					.with_color(Color::Red),
			);
		if let Some(note) = &self.note {
			builder = builder.with_note(note);
		}
		builder.finish().eprint((id, Source::from(src))).unwrap();
	}
}
