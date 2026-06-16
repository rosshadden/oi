use crate::helpers::*;

#[test]
fn string_concat() {
	check(r#""foo" + "bar""#, "foobar");
}
