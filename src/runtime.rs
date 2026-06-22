//! Functions a compiled Oi program calls at runtime.
//! Backend-agnostic: the JIT registers them as symbols, an object backend would link them in.

use std::ffi::{CStr, c_char};

pub const STR_CONCAT: &str = "oi_str_concat";
pub const ALLOC: &str = "oi_alloc";
pub const PRINT: &str = "oi_print";
pub const WRITE: &str = "oi_write";
pub const WRITE_SEP: &str = "oi_write_sep";
pub const SLICE: &str = "oi_slice";
pub const PANIC_OOB: &str = "oi_panic_oob";

// Type tag shared with the compiler.
#[repr(i64)]
#[derive(Clone, Copy)]
pub enum Tag {
	Bool,
	Int,
	Float,
	Str,
	Raw,
}

// Render one value to a string.
fn render(tag: Tag, bits: i64, quote: bool) -> String {
	match tag {
		Tag::Bool => (bits == 1).to_string(),
		Tag::Int => bits.to_string(),
		Tag::Float => format!("{:?}", f64::from_bits(bits as u64)),
		Tag::Str | Tag::Raw => {
			let s = unsafe { CStr::from_ptr(bits as *const c_char) }.to_string_lossy();
			if quote && matches!(tag, Tag::Str) {
				format!("{s:?}")
			} else {
				s.into_owned()
			}
		}
	}
}

// Print a top-level value with a newline.
pub extern "C" fn print(tag: Tag, bits: i64) {
	println!("{}", render(tag, bits, false));
}

// Write a value fragment with no newline.
pub extern "C" fn write(tag: Tag, bits: i64) {
	print!("{}", render(tag, bits, true));
}

// Write the ", " that separates collection elements, before every element but the first.
pub extern "C" fn write_sep(i: i64) {
	if i > 0 {
		print!(", ");
	}
}

// Panic with an out-of-bounds message.
pub extern "C" fn panic_oob(index: i64, len: i64) {
	eprintln!("index out of range: the length is {len} but the index is {index}");
	std::process::abort();
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

// Allocate `size` zeroed bytes for a composite value (e.g. a tuple's field slots).
pub extern "C" fn alloc(size: i64) -> *mut u8 {
	// TODO: address this without leaking
	let size = size.max(1) as usize;
	Box::leak(vec![0u8; size].into_boxed_slice()).as_mut_ptr()
}

// View the range `[start, end)` of an array.
// The view shares the parent's element buffer.
// Panics if out of range.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn slice(header: *const i64, start: i64, end: i64, elem_size: i64) -> *const i64 {
	let (data, len) = unsafe { (*header, *header.add(1)) };
	if start < 0 || start > end || end > len {
		eprintln!("slice range {start}..{end} out of bounds for array of length {len}");
		std::process::abort();
	}
	let out = alloc(16) as *mut i64;
	unsafe {
		*out = data + start * elem_size;
		*out.add(1) = end - start;
	}
	out
}
