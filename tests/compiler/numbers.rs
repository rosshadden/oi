use crate::helpers::*;

#[test]
fn int() {
	check("123", "123");
	check("1_000_000_000", "1000000000");
	check("1_2_3_4_5", "12345");
}

#[test]
fn float() {
	check("123.0", "123.0");
	check("123.0000", "123.0");
	check("10_000.22", "10000.22");
}

#[test]
fn hex() {
	check("0x7B", "123");
	check("0xFF", "255");
	check("0xFF_00", "65280");
	// TODO: add once we support 64-bit numbers
	// check("0xFF80_0000_0000_0000", "0xFF80000000000000");
}

#[test]
fn binary() {
	check("0b01111011", "123");
	check("0b1_1111_1111", "511");
}

#[test]
fn octal() {
	check("0o173", "123");
	check("0o7_5_5", "493");
}
