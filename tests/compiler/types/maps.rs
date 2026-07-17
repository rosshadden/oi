use crate::helpers::*;
use indoc::indoc;

#[test]
fn declare_and_set_get() {
	check(
		indoc! {r#"
			mut m Map[string, int]
			m["one"] = 1
			m["one"]
		"#},
		"1",
	);
}

#[test]
fn init_expr_declare_and_set_get() {
	check(
		indoc! {r#"
			mut m := Map[string, int]{}
			m["one"] = 1
			m["one"]
		"#},
		"1",
	);
}

#[test]
fn overwrite_key() {
	check(
		indoc! {r#"
			mut m Map[string, int]
			m["a"] = 1
			m["a"] = 2
			m["a"]
		"#},
		"2",
	);
}

#[test]
fn multiple_keys() {
	check(
		indoc! {r#"
			mut m Map[string, int]
			m["one"] = 1
			m["two"] = 2
			m["one"] + m["two"]
		"#},
		"3",
	);
}

#[test]
fn int_keys() {
	check(
		indoc! {r#"
			mut m Map[int, string]
			m[1] = "a"
			m[2] = "b"
			m[1]
		"#},
		"a",
	);
}

#[test]
fn float_keys() {
	check(
		indoc! {"
			mut m Map[float, int]
			m[1.2] = 6
			m[2.1] = 9
			m[2.1]
		"},
		"9",
	);
}

#[test]
fn bool_keys() {
	check(
		indoc! {"
			mut m Map[bool, int]
			m[true] = 6
			m[false] = 9
			m[false]
		"},
		"9",
	);
}

#[test]
fn tuple_keys_fail_for_now() {
	// TODO: actually implement complex keys and fix test
	assert!(
		fail(indoc! {"
			type Point = (int, int)
			mut m Map[Point, int]
			m[(1, 2)] = 6
			m[(2, 1)] = 9
			m[(2, 1)]
		"})
		.contains("tuple cannot be used as a map key")
	);
}

#[test]
fn missing_key_panics() {
	assert!(
		fail(indoc! {r#"
			mut m Map[string, int]
			m["missing"]
		"#})
		.contains("key not found")
	);
}

#[test]
fn wrong_key_type() {
	assert!(
		fail(indoc! {r#"
			mut m Map[string, int]
			m[1]
		"#})
		.contains("expected str key")
	);
}

#[test]
fn wrong_value_type() {
	assert!(
		fail(indoc! {r#"
			mut m Map[string, int]
			m["a"] = "b"
		"#})
		.contains("type mismatch")
	);
}

#[test]
fn delete_key() {
	check(
		indoc! {r#"
			mut m Map[string, int]
			m["one"] = 1
			m["two"] = 2
			m.delete["one"]
			m["two"]
		"#},
		"2",
	);
}

#[test]
fn delete_missing_key_is_noop() {
	check(
		indoc! {r#"
			mut m Map[string, int]
			m.delete["missing"]
			1
		"#},
		"1",
	);
}

#[test]
fn deleted_key_then_lookup_panics() {
	assert!(
		fail(indoc! {r#"
			mut m Map[string, int]
			m["one"] = 1
			m.delete["one"]
			m["one"]
		"#})
		.contains("key not found")
	);
}

#[test]
fn delete_on_immutable_map_fails() {
	assert!(
		fail(indoc! {r#"
			fn f(m Map[string, int]) int {
				m.delete["one"]
				m["one"]
			}
			mut n Map[string, int]
			n["one"] = 1
			f(n)
		"#})
		.contains("immutable")
	);
}
