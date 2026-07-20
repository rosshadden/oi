//! Functions a compiled Oi program calls at runtime.
//! Backend-agnostic: the JIT registers them as symbols, an object backend would link them in.

use std::collections::HashMap;
use std::ffi::{CStr, c_char};
use std::mem::size_of;

pub const STR_CONCAT: &str = "oi_str_concat";
pub const ALLOC: &str = "oi_alloc";
pub const WRITE: &str = "oi_write";
pub const WRITE_SEP: &str = "oi_write_sep";
pub const SLICE: &str = "oi_slice";
pub const PANIC_OOB: &str = "oi_panic_oob";
pub const ARRAY_RESERVE: &str = "oi_array_reserve";
pub const ARRAY_EXTEND: &str = "oi_array_extend";
pub const STR_EQ: &str = "oi_str_eq";
pub const STR_CONTAINS: &str = "oi_str_contains";
pub const ASSERT_FAIL: &str = "oi_assert_fail";
pub const PANIC: &str = "oi_panic";
pub const MAP_NEW: &str = "oi_map_new";
pub const MAP_GET: &str = "oi_map_get";
pub const MAP_SET: &str = "oi_map_set";
pub const MAP_DELETE: &str = "oi_map_delete";

// Type tag shared with the compiler.
#[repr(i64)]
#[derive(Clone, Copy)]
pub enum Tag {
	Bool,
	Int,
	UInt,
	Float,
	Str,
	Raw,
}

impl Tag {
	// Checked conversion from the raw i64 the JIT passes across the ABI.
	fn from_i64(v: i64) -> Tag {
		match v {
			0 => Tag::Bool,
			1 => Tag::Int,
			2 => Tag::UInt,
			3 => Tag::Float,
			4 => Tag::Str,
			5 => Tag::Raw,
			_ => {
				eprintln!("invalid tag: {v}");
				std::process::abort();
			}
		}
	}
}

unsafe fn cstr<'a>(ptr: *const u8) -> &'a CStr {
	unsafe { CStr::from_ptr(ptr as *const c_char) }
}

// Render one value to a string.
fn render(tag: Tag, bits: i64, width: i64, quote: bool) -> String {
	match tag {
		Tag::Bool => (bits == 1).to_string(),
		Tag::Int => bits.to_string(),
		Tag::UInt => (bits as u64).to_string(),
		Tag::Float => match width {
			16 => format!("{:?}", f16::from_bits(bits as u16)),
			32 => format!("{:?}", f32::from_bits(bits as u32)),
			_ => format!("{:?}", f64::from_bits(bits as u64)),
		},
		Tag::Str | Tag::Raw => {
			let s = unsafe { cstr(bits as *const u8) }.to_string_lossy();
			if quote && matches!(tag, Tag::Str) {
				format!("{s:?}")
			} else {
				s.into_owned()
			}
		}
	}
}

// Write a rendered value fragment.
pub extern "C" fn write(tag: i64, bits: i64, width: i64, quote: i64, stderr: i64) {
	let s = render(Tag::from_i64(tag), bits, width, quote != 0);
	if stderr != 0 { eprint!("{s}") } else { print!("{s}") }
}

// Write the ", " separator before every element but the first.
pub extern "C" fn write_sep(i: i64, stderr: i64) {
	if i > 0 {
		if stderr != 0 { eprint!(", ") } else { print!(", ") }
	}
}

// Panic with an out-of-bounds message.
pub extern "C" fn panic_oob(index: i64, len: i64) {
	eprintln!("index out of range: the length is {len} but the index is {index}");
	std::process::abort();
}

// Print `{prefix}{msg}` and abort.
unsafe fn abort_with(prefix: &str, msg: *const u8) -> ! {
	let msg = unsafe { cstr(msg) }.to_string_lossy();
	eprintln!("{prefix}{msg}");
	std::process::abort();
}

/// Print an assertion failure message and abort.
/// # Safety
/// `msg` must be a valid NUL-terminated C string.
pub unsafe extern "C" fn assert_fail(msg: *const u8) {
	unsafe { abort_with("assertion failed: ", msg) }
}

/// Print a panic message and abort.
/// # Safety
/// `msg` must be a valid NUL-terminated C string.
pub unsafe extern "C" fn panic(msg: *const u8) {
	unsafe { abort_with("panic: ", msg) }
}

/// # Safety
/// `collection` and `value` must be valid NUL-terminated C strings.
pub unsafe extern "C" fn str_contains(collection: *const u8, value: *const u8) -> i64 {
	let h = unsafe { cstr(collection) }.to_string_lossy();
	let n = unsafe { cstr(value) }.to_string_lossy();
	h.contains(n.as_ref()) as i64
}

/// Compare two 0-terminated strings.
/// # Safety
/// `a` and `b` must be valid NUL-terminated C strings.
pub unsafe extern "C" fn str_eq(a: *const u8, b: *const u8) -> i64 {
	let a = unsafe { cstr(a) };
	let b = unsafe { cstr(b) };
	(a == b) as i64
}

/// Concatenate two 0-terminated strings into a fresh one.
/// # Safety
/// `a` and `b` must be valid NUL-terminated C strings.
pub unsafe extern "C" fn str_concat(a: *const u8, b: *const u8) -> *const u8 {
	let a = unsafe { cstr(a) }.to_bytes();
	let b = unsafe { cstr(b) }.to_bytes();
	let mut out = Vec::with_capacity(a.len() + b.len() + 1);
	out.extend_from_slice(a);
	out.extend_from_slice(b);
	out.push(0);
	Box::leak(out.into_boxed_slice()).as_ptr()
}

// Allocate `size` zeroed bytes for a composite value (e.g. a tuple's field slots).
pub extern "C" fn alloc(size: i64) -> *mut u8 {
	let size = size.max(1) as usize;
	Box::leak(vec![0u8; size].into_boxed_slice()).as_mut_ptr()
}

// Array header layout shared with the compiler (lower/array.rs, offsets 0/8/16).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Header {
	data: i64,
	len: i64,
	cap: i64,
}

/// View the range `[start, end)` of an array.
/// The view shares the parent's element buffer.
/// Panics if out of range.
/// # Safety
/// `header` must point to a valid array header (see `Header`).
pub unsafe extern "C" fn slice(header: *const Header, start: i64, end: i64, elem_size: i64) -> *const Header {
	let Header { data, len, .. } = unsafe { *header };
	if start < 0 || start > end || end > len {
		eprintln!("slice range {start}..{end} out of bounds for array of length {len}");
		std::process::abort();
	}
	let view_len = end - start;
	let out = alloc(size_of::<Header>() as i64) as *mut Header;
	unsafe {
		// cap == len: slice can't grow in-place
		*out = Header {
			data: data + start * elem_size,
			len: view_len,
			cap: view_len,
		};
	}
	out
}

/// Ensure the array has capacity for at least `min_cap` elements.
/// Grows by doubling, at least to `min_cap`. Updates data and cap in place.
/// # Safety
/// `header` must point to a valid array header (see `Header`).
pub unsafe extern "C" fn array_reserve(header: *mut Header, min_cap: i64, elem_size: i64) {
	let Header { data, len, cap } = unsafe { *header };
	if min_cap <= cap {
		return;
	}
	let new_cap = (cap.max(1) * 2).max(min_cap);
	let new_data = alloc(new_cap * elem_size);
	unsafe {
		std::ptr::copy_nonoverlapping(data as *const u8, new_data, (len * elem_size) as usize);
		(*header).data = new_data as i64;
		(*header).cap = new_cap;
	}
}

/// Append all elements of `src` to `dst`, growing dst's buffer as needed.
/// # Safety
/// `dst` and `src` must point to valid array headers (see `Header`).
pub unsafe extern "C" fn array_extend(dst: *mut Header, src: *const Header, elem_size: i64) {
	let dst_len = unsafe { (*dst).len };
	let Header {
		data: src_data,
		len: src_len,
		..
	} = unsafe { *src };
	unsafe { array_reserve(dst, dst_len + src_len, elem_size) };
	unsafe {
		let dst_data = (*dst).data as *mut u8;
		let dst_tail = dst_data.add((dst_len * elem_size) as usize);
		std::ptr::copy_nonoverlapping(src_data as *const u8, dst_tail, (src_len * elem_size) as usize);
		(*dst).len = dst_len + src_len;
	}
}

#[derive(PartialEq, Eq, Hash)]
enum MapKey {
	Raw(i64),
	Str(Vec<u8>),
}

fn map_key(tag: Tag, bits: i64) -> MapKey {
	match tag {
		Tag::Str => MapKey::Str(unsafe { cstr(bits as *const u8) }.to_bytes().to_vec()),
		_ => MapKey::Raw(bits),
	}
}

pub struct OiMap {
	entries: HashMap<MapKey, i64>,
}

pub extern "C" fn map_new() -> *mut OiMap {
	Box::into_raw(Box::new(OiMap {
		entries: HashMap::new(),
	}))
}

/// # Safety
/// `map` must be a valid, live `OiMap` pointer.
pub unsafe extern "C" fn map_get(map: *mut OiMap, tag: i64, bits: i64) -> i64 {
	let map = unsafe { &*map };
	match map.entries.get(&map_key(Tag::from_i64(tag), bits)) {
		Some(v) => *v,
		None => {
			eprintln!("key not found in map");
			std::process::abort();
		}
	}
}

/// # Safety
/// `map` must be a valid, live `OiMap` pointer.
pub unsafe extern "C" fn map_set(map: *mut OiMap, tag: i64, bits: i64, value: i64) {
	let map = unsafe { &mut *map };
	map.entries.insert(map_key(Tag::from_i64(tag), bits), value);
}

/// Remove a map entry if present.
/// # Safety
/// `map` must be a valid, live `OiMap` pointer.
pub unsafe extern "C" fn map_delete(map: *mut OiMap, tag: i64, bits: i64) {
	let map = unsafe { &mut *map };
	map.entries.remove(&map_key(Tag::from_i64(tag), bits));
}
