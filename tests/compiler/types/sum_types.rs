use crate::helpers::*;

#[test]
fn tail_return() {
	check(
		indoc! {"
			type Status = :ok | :err
			fn f() Status { :err }
			f()
		"},
		"err",
	);
}

#[test]
fn bind_and_print() {
	check(
		indoc! {"
			type Status = :ok | :err
			x Status := :ok
			x
		"},
		"ok",
	);
}

#[test]
fn zero_value_is_first_variant() {
	check(
		indoc! {"
			type Status = :ok | :err
			mut x Status
			x
		"},
		"ok",
	);
}

#[test]
fn eq() {
	check(
		indoc! {"
			type Status = :ok | :err
			a Status := :ok
			b Status := :ok
			a == b
		"},
		"true",
	);
	check(
		indoc! {"
			type Status = :ok | :err
			a Status := :ok
			b Status := :err
			a == b
		"},
		"false",
	);
	check(
		indoc! {"
			type Status = :ok | :err
			a Status := :ok
			b Status := :err
			a != b
		"},
		"true",
	);
}

#[test]
fn matching() {
	check(
		indoc! {r#"
			type Status = :ok | :err
			x Status := :err
			match x {
				:ok { "good" }
				:err { "bad" }
			}
		"#},
		"bad",
	);
	check(
		indoc! {r#"
			type Status = :ok | :err
			x Status := :err
			match x {
				:ok { "good" }
				_ { "fallback" }
			}
		"#},
		"fallback",
	);
	let err = fail(indoc! {r#"
			type Status = :ok | :err
			x Status := :err
			match x {
				:ok { "good" }
			}
		"#});
	assert!(err.contains("non-exhaustive match, missing: err"), "got: {err}");
}

#[test]
fn unknown_atom_errors() {
	let err = fail(indoc! {"
		type Status = :ok | :err
		x Status := :nope
	"});
	assert!(err.contains("has no atom `:nope`"), "got: {err}");
}

#[test]
fn duplicate_atom_in_type_errors() {
	let err = fail(indoc! {"
		type Status = :ok | :ok
		fn f() Status { :ok }
		f()
	"});
	assert!(err.contains("duplicate atom `:ok` in sum type"), "got: {err}");
}

#[test]
fn int_cast_gives_tag() {
	check(
		indoc! {"
			type Status = :ok | :err
			x Status := :ok
			int(x)
		"},
		"0",
	);
	check(
		indoc! {"
			type Status = :ok | :err
			x Status := :err
			int(x)
		"},
		"1",
	);
}

#[test]
fn struct_field_type() {
	check(
		indoc! {"
			type Status = :ok | :err
			struct Res { s Status }
			r := Res{ s: :err }
			r.s
		"},
		"err",
	);
}

#[test]
fn anonymous_sum_in_signature_errors() {
	fail(indoc! {"
		fn f() :ok | :err { :err }
		f()
	"});
}

#[test]
fn anonymous_sum_in_bind_errors() {
	fail("x :ok | :err := :ok");
}
