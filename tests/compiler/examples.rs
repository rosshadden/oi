use std::path::PathBuf;

use crate::support::{oi, stdout_ok};

/// Expected stdout for every file in `examples/`.
const EXPECTED: &[(&str, &str)] = &[
	("boxes", "hi"),
	("dimensions", "(width: 1920, height: 1080)"),
	("errors", "84"),
	("grades", "B"),
	("main", "3.2"),
	("points", "(13, 4)"),
	("ranges", "true"),
	("shapes", "triangle: (3.0, 4.0, 5.0)"),
	("users", "🟢 {u.name}\nWelcome back, {self.name}!"), // FIX: revisit once string interpolation is implemented
];

fn examples_dir() -> PathBuf {
	PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples")
}

#[test]
fn run_examples() {
	for (name, expected) in EXPECTED {
		let path = examples_dir().join(format!("{name}.oi"));
		let out = stdout_ok(oi(&["run", path.to_str().unwrap()], None));
		assert_eq!(&out, expected, "examples/{name}.oi");
	}
}

#[test]
fn all_examples_covered() {
	for entry in std::fs::read_dir(examples_dir()).unwrap() {
		let path = entry.unwrap().path();
		if path.extension().is_some_and(|e| e == "oi") {
			let file = path.file_stem().unwrap().to_str().unwrap();
			assert!(
				EXPECTED.iter().any(|(n, _)| n == &file),
				"examples/{file}.oi has no entry in EXPECTED"
			);
		}
	}
}
