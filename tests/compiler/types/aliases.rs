use crate::helpers::*;

#[test]
fn alias_primitive_in_param() {
	let src = indoc! {"
		type Score = int
		fn double(s Score) Score { s * 2 }
		double(21)
	"};
	check(src, "42");
}

#[test]
fn alias_primitive_in_return() {
	let src = indoc! {"
		type Name = str
		fn greet() Name { \"hello\" }
		greet()
	"};
	check(src, "hello");
}

#[test]
fn alias_chains() {
	let src = indoc! {"
		type Meters = int
		type Distance = Meters
		fn add(a Distance, b Distance) Distance { a + b }
		add(3, 4)
	"};
	check(src, "7");
}

#[test]
fn alias_tuple_in_param_and_return() {
	let src = indoc! {"
		type Point = (int, int)
		fn make(x int, y int) Point { (x, y) }
		p := make(3, 4)
		print(p.0, p.1)
	"};
	check(src, "3 4");
}

#[test]
fn alias_array_in_param() {
	let src = indoc! {"
		type Row = []int
		fn first(r Row) int { r[0] }
		first([10 20 30])
	"};
	check(src, "10");
}

#[test]
fn alias_in_struct_field() {
	let src = indoc! {"
		type Hp = int
		struct Unit { hp Hp }
		u := Unit { hp: 100 }
		u.hp
	"};
	check(src, "100");
}

#[test]
fn fn_type_alias_parses() {
	// NOTE: function type aliases are not yet supported, but for now they parse
	let src = "type Op = fn (int) int";
	run(src);
}

#[test]
fn alias_of_result_long_form() {
	let src = indoc! {r#"
		type Found = Result[int, Error]
		fn find(x int) Found {
			if x > 0 { return x }
			return error("negative")
		}
		find(5)
	"#};
	check(src, "ok");
}

#[test]
fn unknown_alias_target_errors() {
	let src = indoc! {"
		type Foo = Nope
		fn f(x Foo) Foo { x }
		f(1)
	"};
	assert!(fail(src).contains("unknown type"));
}
