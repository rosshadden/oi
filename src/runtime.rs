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
pub const ARRAY_RESERVE: &str = "oi_array_reserve";
pub const ARRAY_EXTEND: &str = "oi_array_extend";
pub const STR_EQ: &str = "oi_str_eq";

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

// Compare two 0-terminated strings.
pub extern "C" fn str_eq(a: *const u8, b: *const u8) -> i64 {
	let a = unsafe { CStr::from_ptr(a as *const c_char) };
	let b = unsafe { CStr::from_ptr(b as *const c_char) };
	(a == b) as i64
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
	let view_len = end - start;
	let out = alloc(24) as *mut i64;
	unsafe {
		*out = data + start * elem_size;
		*out.add(1) = view_len;
		*out.add(2) = view_len; // cap == len: slice can't grow in-place
	}
	out
}

// Ensure the array has capacity for at least `min_cap` elements.
// Grows by doubling, at least to `min_cap`. Updates data and cap in place.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn array_reserve(header: *mut i64, min_cap: i64, elem_size: i64) {
	let (data, len, cap) = unsafe { (*header, *header.add(1), *header.add(2)) };
	if min_cap <= cap {
		return;
	}
	let new_cap = cap.max(1) * 2;
	let new_cap = new_cap.max(min_cap);
	let new_data = alloc(new_cap * elem_size) as *mut u8;
	unsafe {
		std::ptr::copy_nonoverlapping(data as *const u8, new_data, (len * elem_size) as usize);
		*header = new_data as i64;
		*header.add(2) = new_cap;
	}
}

// Append all elements of `src` to `dst`, growing dst's buffer as needed.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn array_extend(dst: *mut i64, src: *const i64, elem_size: i64) {
	let (_, dst_len, _) = unsafe { (*dst, *dst.add(1), *dst.add(2)) };
	let (src_data, src_len) = unsafe { (*src, *src.add(1)) };
	array_reserve(dst, dst_len + src_len, elem_size);
	let new_len = dst_len + src_len;
	unsafe {
		let dst_data = *dst as *mut u8;
		let dst_tail = dst_data.add((dst_len * elem_size) as usize);
		std::ptr::copy_nonoverlapping(src_data as *const u8, dst_tail, (src_len * elem_size) as usize);
		*dst.add(1) = new_len;
	}
}
