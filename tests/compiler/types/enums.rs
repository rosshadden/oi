use crate::helpers::*;

#[test]
fn qualified_access() {
	check("enum Color { red green blue }\nColor.red", "red");
	check("enum Color { red green blue }\nColor.blue", "blue");
}

#[test]
fn oneliner() {
	check("enum Fruit { apple orange grape }\nFruit.orange", "orange");
}

#[test]
fn bind() {
	check(
		"enum Color { red green blue }\nc := Color.green\nc",
		"green",
	);
}

#[test]
fn reassign() {
	check(
		"enum Color { red green blue }\nmut c := Color.red\nc = Color.blue\nc",
		"blue",
	);
}

#[test]
fn first_variant_is_default() {
	check("enum Color { red green blue }\nmut c Color\nc", "red");
}

#[test]
fn eq_same() {
	check(
		"enum Color { red green blue }\nColor.red == Color.red",
		"true",
	);
}

#[test]
fn eq_different() {
	check(
		"enum Color { red green blue }\nColor.red == Color.blue",
		"false",
	);
}

#[test]
fn ne() {
	check(
		"enum Color { red green blue }\nColor.red != Color.blue",
		"true",
	);
}

#[test]
fn returned_from_fn() {
	check(
		"enum Color { red green blue }\nfn fav() Color { Color.blue }\nfav()",
		"blue",
	);
}

#[test]
fn struct_field() {
	check(
		"enum Stat { health mana stamina }
		struct User { s Stat }
		u := User{ s: Stat.mana }
		u.s",
		"mana",
	);
}

#[test]
fn in_match() {
	check(
		indoc! {r#"
			enum Color { red green blue }
			c := Color.green
			match c {
				Color.red { "r" }
				Color.green { "g" }
				else { "?" }
			}
		"#},
		"g",
	);
}

#[test]
fn unknown_variant() {
	let err = fail("enum Color { red green blue }\nColor.purple");
	assert!(err.contains("no variant `purple`"), "got: {err}");
}

#[test]
fn shorthand_in_assignment() {
	check(
		"enum Color { red green blue }\nmut c := Color.green\nc = .red\nc",
		"red",
	);
}

#[test]
fn shorthand_in_annotated_binding() {
	check("enum Color { red green blue }\nc Color := .blue\nc", "blue");
}

#[test]
fn shorthand_in_comparison() {
	check(
		"enum Color { red green blue }\nc := Color.red\nc == .red",
		"true",
	);
	check(
		"enum Color { red green blue }\nc := Color.red\nc != .blue",
		"true",
	);
}

#[test]
fn shorthand_in_match() {
	check(
		indoc! {r#"
			enum Color { red green blue }
			c := Color.green
			match c {
				.red { "r" }
				.green { "g" }
				else { "?" }
			}
		"#},
		"g",
	);
}

#[test]
fn shorthand_in_struct_field() {
	check(
		"enum Stat { health mana stamina }
		struct User { s Stat }
		u := User{ s: .mana }
		u.s",
		"mana",
	);
	check(
		"enum Stat { health mana stamina }
		struct User { s Stat }
		u := User{ .stamina }
		u.s",
		"stamina",
	);
}

#[test]
fn shorthand_unknown_variant() {
	let err = fail("enum Color { red green blue }\nc := Color.red\nc == .purple");
	assert!(err.contains("no variant `purple`"), "got: {err}");
}

#[test]
fn shorthand_without_context_errors() {
	let err = fail("enum Color { red green blue }\n.red");
	assert!(err.contains("cannot infer the enum type"), "got: {err}");
}

#[test]
fn duplicate_disc_rejected() {
	let err = fail("enum E { a = 2, b, c = 2 }");
	assert!(err.contains("discriminant value `2`"), "got: {err}");
}

#[test]
fn auto_increment_from_explicit() {
	let err = fail("enum E { a = 5, b, c = 6 }");
	assert!(err.contains("discriminant value `6`"), "got: {err}");
}

#[test]
fn negative_disc() {
	let err = fail("enum E { a = -2, b, c = -1 }");
	assert!(err.contains("discriminant value `-1`"), "got: {err}");
}

#[test]
fn explicit_disc_default_is_first() {
	check("enum E { a = 5, b c }\nmut x E\nx", "a");
}

#[test]
fn atom_coerces_in_annotated_binding() {
	check("enum Color { red green blue }\nc Color := :blue\nc", "blue");
}

#[test]
fn atom_coerces_in_assignment() {
	check(
		"enum Color { red green blue }\nmut c := Color.green\nc = :red\nc",
		"red",
	);
}

#[test]
fn atom_coerces_in_comparison() {
	check(
		"enum Color { red green blue }\nc := Color.red\nc == :red",
		"true",
	);
	check("enum Color { red green blue }\nColor.blue == :blue", "true");
}

#[test]
fn atom_coerces_in_struct_field() {
	check(
		indoc! {"
			enum Stat { health mana stamina }
			struct User { s Stat }
			u := User{ s: :mana }
			u.s
		"},
		"mana",
	);
}

#[test]
fn atom_unknown_variant() {
	let err = fail("enum Color { red green blue }\nc Color := :purple");
	assert!(err.contains("no variant `purple`"), "got: {err}");
}
