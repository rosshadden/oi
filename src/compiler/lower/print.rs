use super::*;

impl<'a> Translator<'a> {
	pub fn write_lit(&mut self, s: &str, stderr: bool) {
		let ptr = self.str_const(s);
		self.emit_frag(runtime::Tag::Raw, ptr, 0, false, stderr);
	}

	fn emit_frag(&mut self, tag: runtime::Tag, bits: Value, width: u16, quote: bool, stderr: bool) {
		let tag = self.b.ins().iconst(self.int, tag as i64);
		let width = self.b.ins().iconst(self.int, width as i64);
		let quote = self.b.ins().iconst(self.int, quote as i64);
		let stderr_v = self.b.ins().iconst(self.int, stderr as i64);
		let func = self.import_fn(
			runtime::WRITE,
			&[self.int, self.int, self.int, self.int, self.int],
			None,
		);
		self.b.ins().call(func, &[tag, bits, width, quote, stderr_v]);
	}

	// Enum `Display`.
	pub(super) fn enum_name_str(&mut self, variants: &[VariantInfo], val: Value) -> Value {
		let tag = self.enum_tag(variants, val);
		let mut ptr = self.str_const("");
		for v in variants {
			let s = self.str_const(&v.name);
			let disc = self.b.ins().iconst(self.int, v.disc);
			let hit = self.b.ins().icmp(IntCC::Equal, tag, disc);
			ptr = self.b.ins().select(hit, s, ptr);
		}
		ptr
	}

	pub fn emit_print(&mut self, val: Value, typ: &Typ, quote: bool, stderr: bool) {
		match typ {
			Typ::Tuple(fields) => {
				self.write_lit("(", stderr);
				for (i, (name, ft)) in fields.iter().enumerate() {
					if i > 0 {
						self.write_lit(", ", stderr);
					}
					if let Some(name) = name {
						self.write_lit(&format!("{name}: "), stderr);
					}
					let cl = cl_type(ft, self.int);
					let fv = self.b.ins().load(cl, MemFlags::new(), val, (i * 8) as i32);
					self.emit_print(fv, ft, true, stderr);
				}
				self.write_lit(")", stderr);
			}

			// array length is only known at runtime, so emit a loop
			Typ::Array(elem) | Typ::FixedArray(elem, _) => {
				self.write_lit("[", stderr);
				let (data, len) = self.array_parts(val, typ);
				let i = self.b.declare_var(self.int);
				let zero = self.b.ins().iconst(self.int, 0);
				self.b.def_var(i, zero);

				let header = self.b.create_block();
				let body = self.b.create_block();
				let exit = self.b.create_block();
				self.b.ins().jump(header, &[]);

				self.b.switch_to_block(header);
				let iv = self.b.use_var(i);
				let more = self.b.ins().icmp(IntCC::SignedLessThan, iv, len);
				self.b.ins().brif(more, body, &[], exit, &[]);
				self.b.seal_block(body);
				self.b.seal_block(exit);

				self.b.switch_to_block(body);
				let iv = self.b.use_var(i);
				let stderr_v = self.b.ins().iconst(self.int, stderr as i64);
				let sep = self.import_fn(runtime::WRITE_SEP, &[self.int, self.int], None);
				self.b.ins().call(sep, &[iv, stderr_v]);
				let off = self.b.ins().imul_imm(iv, elem_size(elem));
				let addr = self.b.ins().iadd(data, off);
				let ev = self.b.ins().load(cl_type(elem, self.int), MemFlags::new(), addr, 0);
				self.emit_print(ev, elem, true, stderr);
				let next = self.b.ins().iadd_imm(iv, 1);
				self.b.def_var(i, next);
				self.b.ins().jump(header, &[]);
				self.b.seal_block(header);

				self.b.switch_to_block(exit);
				self.write_lit("]", stderr);
			}

			Typ::Struct(sname, fields) => {
				let sname = sname.clone();
				let fields = fields.clone();
				self.write_lit(&format!("{sname}{{"), stderr);
				for (i, f) in fields.iter().enumerate() {
					if i > 0 {
						self.write_lit(", ", stderr);
					}
					self.write_lit(&format!("{}: ", f.name), stderr);
					let cl = cl_type(&f.typ, self.int);
					let fv = self.b.ins().load(cl, MemFlags::new(), val, (i * 8) as i32);
					self.emit_print(fv, &f.typ, true, stderr);
				}
				self.write_lit("}", stderr);
			}

			Typ::Atom => {
				self.emit_frag(runtime::Tag::Raw, val, 0, false, stderr);
			}

			Typ::Enum(_) | Typ::Option(_) | Typ::Result(_) | Typ::AtomSum(_) => {
				let variants = self.variants_of(typ);
				let ptr = self.enum_name_str(&variants, val);
				self.emit_frag(runtime::Tag::Raw, ptr, 0, false, stderr);
			}

			Typ::Range => {
				let cl = cl_int_for_width(32);
				let start = self.b.ins().load(cl, MemFlags::new(), val, 0);
				let end = self.b.ins().load(cl, MemFlags::new(), val, 8);
				self.emit_print(start, &Typ::Int(32), false, stderr);
				self.write_lit("..", stderr);
				self.emit_print(end, &Typ::Int(32), false, stderr);
			}

			Typ::Fn(..) | Typ::Closure(..) => self.write_lit("<fn>", stderr),
			Typ::Map(..) => self.write_lit("<map>", stderr),

			_ => {
				let tag = match typ {
					Typ::Bool => runtime::Tag::Bool,
					Typ::Int(_) | Typ::ISize => runtime::Tag::Int,
					Typ::UInt(_) | Typ::USize => runtime::Tag::UInt,
					Typ::Float(_) => runtime::Tag::Float,
					Typ::Str | Typ::Error => runtime::Tag::Str,
					Typ::Atom
					| Typ::Tuple(_)
					| Typ::Array(_)
					| Typ::FixedArray(..)
					| Typ::Struct(..)
					| Typ::Enum(_)
					| Typ::Option(_)
					| Typ::Result(_)
					| Typ::AtomSum(_)
					| Typ::Range
					| Typ::Fn(..)
					| Typ::Closure(..)
					| Typ::Map(..) => {
						unreachable!("handled above")
					}
				};
				// normalize to pointer-sized before passing to the runtime
				let (bits, float_width) = match typ {
					Typ::Float(16) => {
						let i16v = self.b.ins().bitcast(types::I16, MemFlags::new(), val);
						(self.b.ins().uextend(self.int, i16v), 16)
					}
					Typ::Float(32) => {
						let i32v = self.b.ins().bitcast(types::I32, MemFlags::new(), val);
						(self.b.ins().uextend(self.int, i32v), 32)
					}
					Typ::Float(64) => (self.b.ins().bitcast(self.int, MemFlags::new(), val), 64),
					Typ::Float(128) => {
						panic!("f128 printing not yet supported by the JIT backend")
					}
					Typ::Float(w) => panic!("unsupported float width f{w}"),
					Typ::Int(w) if *w < 64 => (self.b.ins().sextend(self.int, val), 0),
					Typ::UInt(w) if *w < 64 => (self.b.ins().uextend(self.int, val), 0),
					_ => (val, 0),
				};
				self.emit_frag(tag, bits, float_width, quote, stderr);
			}
		}
	}
}
