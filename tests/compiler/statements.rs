use crate::helpers::*;

#[test]
fn stmts() {
	let src = indoc! {"
		x := 3
		y := x * x
		z := y + x
		z
	"};
	check(src, "12");
}
