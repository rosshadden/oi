use crate::helpers::*;

#[test]
fn variable() {
	check("x := 42\nx", "42");
}

#[test]
fn assign() {
	check("mut x := 1\nx = 2\nx", "2");
}

#[test]
fn assign_from_self() {
	check("mut x := 10\nx = x + 5\nx", "15");
}

#[test]
fn assign_string() {
	check(r#"mut s := "old"; s = "new"; s"#, "new");
}
