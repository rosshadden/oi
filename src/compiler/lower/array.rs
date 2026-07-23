use super::*;

impl<'a> Translator<'a> {
	// array handle: { data @ 0, len @ 8, cap @ 16 }
	pub(super) fn array_data(&mut self, header: Value) -> Value {
		self.b.ins().load(self.int, MemFlags::new(), header, 0)
	}

	pub(super) fn array_len(&mut self, header: Value) -> Value {
		self.b.ins().load(self.int, MemFlags::new(), header, 8)
	}

	pub(super) fn array_cap(&mut self, header: Value) -> Value {
		self.b.ins().load(self.int, MemFlags::new(), header, 16)
	}

	pub(super) fn make_array(&mut self, data: Value, len: Value) -> Value {
		let header = self.call_alloc(3);
		self.b.ins().store(MemFlags::new(), data, header, 0);
		self.b.ins().store(MemFlags::new(), len, header, 8);
		self.b.ins().store(MemFlags::new(), len, header, 16);
		header
	}

	// Evaluate an array-typed operand.
	pub(super) fn array_operand(&mut self, collection: &Spanned<Expr>, what: &str) -> Result<TypedVal, Diagnostic> {
		let (ptr, typ) = self.expr(collection)?;
		match typ {
			Typ::Array(_) | Typ::FixedArray(..) => Ok((ptr, typ)),
			_ => Err(
				Diagnostic::new(format!("cannot {what} {typ}"), collection.1.into_range()).with_label("not an array"),
			),
		}
	}

	// (data pointer, length) for an array.
	pub(super) fn array_parts(&mut self, val: Value, typ: &Typ) -> (Value, Value) {
		match typ {
			Typ::FixedArray(_, n) => (val, self.b.ins().iconst(self.int, *n as i64)),
			_ => (self.array_data(val), self.array_len(val)),
		}
	}

	pub(super) fn int_value(&mut self, e: &Spanned<Expr>, what: &str) -> Result<Value, Diagnostic> {
		let (v, t) = self.expr(e)?;
		if !matches!(t, Typ::Int(_)) {
			return Err(
				Diagnostic::new(format!("{what} must be Int, got {t}"), e.1.into_range()).with_label("not an Int"),
			);
		}
		Ok(v)
	}

	// Bounds-check `idx` and return the element address.
	pub(super) fn elem_addr(&mut self, data: Value, len: Value, elem: &Typ, idx: Value) -> Value {
		let oob = self.b.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, idx, len);

		let panic_block = self.b.create_block();
		let ok_block = self.b.create_block();
		self.b.ins().brif(oob, panic_block, &[], ok_block, &[]);
		self.b.seal_block(panic_block);
		self.b.seal_block(ok_block);

		self.b.switch_to_block(panic_block);
		let func = self.import_fn(runtime::PANIC_OOB, &[self.int, self.int], None);
		self.b.ins().call(func, &[idx, len]);
		self.b.ins().trap(TrapCode::HEAP_OUT_OF_BOUNDS);

		self.b.switch_to_block(ok_block);
		let off = self.b.ins().imul_imm(idx, elem_size(elem));
		self.b.ins().iadd(data, off)
	}

	pub(super) fn load_index(&mut self, data: Value, len: Value, elem: &Typ, idx: Value) -> Value {
		let addr = self.elem_addr(data, len, elem, idx);
		self.b.ins().load(cl_type(elem, self.int), MemFlags::new(), addr, 0)
	}

	pub(super) fn store_index(&mut self, data: Value, len: Value, elem: &Typ, idx: Value, val: Value) {
		let addr = self.elem_addr(data, len, elem, idx);
		self.b.ins().store(MemFlags::new(), val, addr, 0);
	}
}
