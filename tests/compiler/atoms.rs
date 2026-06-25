use crate::helpers::*;

#[test]
fn atom_literal() {
	check(":foo", ":foo");
	check(":2", ":2");
	check(":28days_later", ":28days_later");
}

#[test]
fn atom_bind() {
	check("x := :apple\nx", ":apple");
}

#[test]
fn atom_eq_same() {
	check(":foo == :foo", "true");
}

#[test]
fn atom_eq_different() {
	check(":foo == :bar", "false");
}

#[test]
fn atom_ne() {
	check(":foo != :bar", "true");
}

#[test]
fn atom_bound_eq() {
	check("x := :apple\nx == :apple", "true");
}

#[test]
fn atom_bound_ne() {
	check("x := :apple\nx == :banana", "false");
}

#[test]
fn atom_two_bindings_eq() {
	check("a := :thing\nb := :thing\na == b", "true");
}

#[test]
fn atom_in_match() {
	check(
		r#"x := :ok
match x {
	:ok { "yes" }
	else { "no" }
}"#,
		"yes",
	);
}
