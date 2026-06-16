//! Functions a compiled Oi program calls at runtime. Backend-agnostic: the JIT
//! registers them as symbols, an object backend would link them in.

use std::ffi::{CStr, c_char};

pub const PRINT_BOOL: &str = "oi_print_bool";
pub const PRINT_INT: &str = "oi_print_int";
pub const PRINT_FLOAT: &str = "oi_print_float";
pub const PRINT_STR: &str = "oi_print_str";
pub const STR_CONCAT: &str = "oi_str_concat";

pub extern "C" fn print_bool(x: i64) {
	println!("{}", x == 1);
}

pub extern "C" fn print_int(x: i64) {
	println!("{x}");
}

pub extern "C" fn print_float(x: f64) {
	println!("{x:?}");
}

pub extern "C" fn print_str(s: *const u8) {
	let s = unsafe { CStr::from_ptr(s as *const c_char) };
	println!("{}", s.to_string_lossy());
}

// Concatenate two 0-terminated strings into a fresh one.
pub extern "C" fn str_concat(a: *const u8, b: *const u8) -> *const u8 {
	let a = unsafe { CStr::from_ptr(a as *const c_char) }.to_bytes();
	let b = unsafe { CStr::from_ptr(b as *const c_char) }.to_bytes();
	let mut out = Vec::with_capacity(a.len() + b.len() + 1);
	out.extend_from_slice(a);
	out.extend_from_slice(b);
	out.push(0);
	// TODO: address this without leaking
	Box::leak(out.into_boxed_slice()).as_ptr()
}
