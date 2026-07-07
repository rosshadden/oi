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
	check("enum Color { red green blue }\nc := Color.green\nc", "green");
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
	check("enum Color { red green blue }\nColor.red == Color.red", "true");
}

#[test]
fn eq_different() {
	check("enum Color { red green blue }\nColor.red == Color.blue", "false");
}

#[test]
fn ne() {
	check("enum Color { red green blue }\nColor.red != Color.blue", "true");
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
	check("enum Color { red green blue }\nc := Color.red\nc == .red", "true");
	check("enum Color { red green blue }\nc := Color.red\nc != .blue", "true");
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
fn payload_construct() {
	check(
		"enum Shape { point triangle(f64, f64, f64) }\nShape.triangle(3.0, 4.0, 5.0)",
		"triangle",
	);
}

#[test]
fn payloadless_variant_of_boxed_enum() {
	check("enum Opt { none some(int) }\nOpt.none", "none");
	check("enum Opt { none some(int) }\no Opt := .none\no", "none");
}

#[test]
fn payload_enum_default_is_first() {
	check("enum Opt { none some(int) }\nmut o Opt\no", "none");
}

#[test]
fn payload_int_cast_gives_tag() {
	check("enum Opt { none some(int) }\nint(Opt.some(1))", "1");
}

#[test]
fn payload_field_type_mismatch() {
	let err = fail("enum Opt { none some(int) }\nOpt.some(3.0)");
	assert!(err.contains("expected int, got float"), "got: {err}");
}

#[test]
fn payload_wrong_arity() {
	let err = fail("enum Opt { none some(int) }\nOpt.some()");
	assert!(err.contains("takes 1 field(s), got 0"), "got: {err}");
}

#[test]
fn payload_match_binds_fields() {
	check(
		indoc! {r#"
			enum Opt { none some(int) }
			o := Opt.some(7)
			match o {
				.some(n) { n }
				.none { -1 }
			}
		"#},
		"7",
	);
}

#[test]
fn payload_match_fieldless_arm() {
	check(
		indoc! {r#"
			enum Opt { none some(int) }
			o Opt := .none
			match o {
				.some(n) { n }
				.none { -1 }
			}
		"#},
		"-1",
	);
}

#[test]
fn payload_match_multiple_fields() {
	check(
		indoc! {r#"
			enum Shape { rect(int, int) tri(int, int, int) }
			s := Shape.rect(3, 4)
			match s {
				.rect(w, h) { w * h }
				.tri(a, b, c) { a + b + c }
			}
		"#},
		"12",
	);
}

#[test]
fn shorthand_payload_construct() {
	check(
		"enum Opt { none some(int) }\no Opt := .some(5)\nmatch o { .some(n) { n } .none { 0 } }",
		"5",
	);
}

#[test]
fn payload_eq() {
	check("enum Opt { none some(int) }\nOpt.some(1) == Opt.some(1)", "true");
	check("enum Opt { none some(int) }\nOpt.some(1) == Opt.some(2)", "false");
	check("enum Opt { none some(int) }\nOpt.none == Opt.some(1)", "false");
	check("enum Opt { none some(int) }\nOpt.none != Opt.some(1)", "true");
}

#[test]
fn payload_eq_string_field() {
	check("enum Msg { quit say(str) }\nMsg.say(\"hi\") == Msg.say(\"hi\")", "true");
	check("enum Msg { quit say(str) }\nMsg.say(\"hi\") == Msg.say(\"bye\")", "false");
}

#[test]
fn payload_ordering_rejected() {
	let err = fail("enum Opt { none some(int) }\nOpt.some(1) < Opt.some(2)");
	assert!(err.contains("only `==`/`!=`"), "got: {err}");
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
	check("enum Color { red green blue }\nc := Color.red\nc == :red", "true");
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

#[test]
fn cast_to_int() {
	check("enum Color { red green blue }\nint(Color.blue)", "2");
}

#[test]
fn cast_to_int_explicit_disc() {
	check("enum Status { ok = 200, err = 500 }\nint(Status.err)", "500");
}

#[test]
fn compare_via_int() {
	check("enum Color { red green blue }\nint(Color.green) == 1", "true");
}

#[test]
fn str_method() {
	check("enum Color { red green blue }\nColor.blue.str()", "blue");
}

#[test]
fn str_method_concat() {
	check(
		r#"enum Color { red green blue }
		"the color is " + Color.green.str()"#,
		"the color is green",
	);
}

#[test]
fn no_such_method() {
	let err = fail("enum Color { red green blue }\nColor.red.hex()");
	assert!(err.contains("has no method `hex`"), "got: {err}");
}
