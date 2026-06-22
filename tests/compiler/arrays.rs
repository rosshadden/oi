use crate::helpers::*;

#[test]
fn array_literal() {
	check("[1, 2, 3]", "[1, 2, 3]");
}

#[test]
fn array_of_strings() {
	// strings are quoted when printed inside an array
	check(r#"["a", "b"]"#, r#"["a", "b"]"#);
}

#[test]
fn array_of_floats() {
	check("[1.5, 2.5]", "[1.5, 2.5]");
}

#[test]
fn array_of_bools() {
	check("[true, false, true]", "[true, false, true]");
}

#[test]
fn single_element() {
	check("[42]", "[42]");
}

#[test]
fn trailing_comma() {
	check("[1, 2,]", "[1, 2]");
}

#[test]
fn index_literal() {
	check("a := [10, 20, 30]\na[1]", "20");
}

#[test]
fn index_variable() {
	check("a := [10, 20, 30]\ni := 2\na[i]", "30");
}

#[test]
fn index_expression() {
	// the index is any Int expression
	check("a := [10, 20, 30]\na[1 + 1]", "30");
}

#[test]
fn dot_index() {
	// numeric dot notation indexes like `[n]`
	check("a := [10, 20, 30]\na.0", "10");
}

#[test]
fn len_field() {
	check("a := [10, 20, 30]\na.len", "3");
}

#[test]
fn index_arithmetic() {
	check("a := [3, 4]\na[0] * a[1]", "12");
}

#[test]
fn array_in_var_prints() {
	check(r#"a := [1, 2, 3]; a"#, "[1, 2, 3]");
}

#[test]
fn nested_in_tuple() {
	// a tuple prints an array element without a trailing newline
	check(r#"(1, [2, 3], "x")"#, r#"(1, [2, 3], "x")"#);
}

#[test]
fn array_of_tuples() {
	// elements print through the same path as everything else, so composites recurse
	check("[(1, 2), (3, 4)]", "[(1, 2), (3, 4)]");
}

#[test]
fn nested_arrays() {
	check(
		"a := [10, 20]\nb := [30, 40]\n[a, b]",
		"[[10, 20], [30, 40]]",
	);
}

#[test]
fn index_into_nested() {
	check("a := [10, 20]\nb := [30, 40]\n[a, b][1]", "[30, 40]");
}

#[test]
fn mixed_types() {
	assert!(fail(r#"[1, "two"]"#).contains("must share a type"));
}

#[test]
fn empty_unsupported() {
	assert!(fail("[]").contains("empty array"));
}

#[test]
fn index_non_array() {
	assert!(fail("x := 5\nx[0]").contains("cannot index"));
}

#[test]
fn non_int_index() {
	assert!(fail(r#"a := [1, 2]; a["x"]"#).contains("index must be Int"));
}

#[test]
fn index_out_of_range() {
	// out-of-range indexing aborts at runtime
	assert!(fail("a := [1, 2]\na[5]").contains("out of range"));
}

#[test]
fn unknown_named_field() {
	assert!(fail("a := [1, 2]\na.foo").contains("no field `foo`"));
}
