use super::*;

impl<'a> Translator<'a> {
	pub(super) fn import_fn(
		&mut self,
		name: &str,
		params: &[types::Type],
		ret: Option<types::Type>,
	) -> codegen::ir::FuncRef {
		let mut sig = self.module.make_signature();
		for &p in params {
			sig.params.push(AbiParam::new(p));
		}
		if let Some(r) = ret {
			sig.returns.push(AbiParam::new(r));
		}
		let id = self.module.declare_function(name, Linkage::Import, &sig).unwrap();
		self.module.declare_func_in_func(id, self.b.func)
	}

	// Emit a call to a resolved fn.
	pub(super) fn call_sig(
		&mut self,
		name: &str,
		sig: FnSig,
		recv: Option<Value>,
		args: &[Spanned<Expr>],
		span: Span,
	) -> Result<TypedVal, Diagnostic> {
		let self_n = recv.is_some() as usize;
		if args.len() + self_n != sig.params.len() {
			return Err(Diagnostic::new(
				format!(
					"`{name}` expects {} argument(s), got {}",
					sig.params.len() - self_n,
					args.len()
				),
				span.into_range(),
			)
			.with_label("wrong number of arguments"));
		}
		let mut vals = Vec::with_capacity(args.len() + self_n);
		let mut expected = sig.params.iter();
		if let Some(recv) = recv {
			expected.next();
			vals.push(recv);
		}
		for arg in args {
			let want = expected.next().unwrap();
			let (val, typ) = self.check_expr(arg, want)?;
			if &typ != want {
				return Err(
					Diagnostic::new(format!("expected {want} argument, got {typ}"), arg.1.into_range())
						.with_label("wrong argument type"),
				);
			}
			vals.push(val);
		}
		Ok(self.emit_call(&sig, &vals))
	}

	// Emit the actual call instruction for a resolved fn signature.
	pub(super) fn emit_call(&mut self, sig: &FnSig, vals: &[Value]) -> TypedVal {
		let func = self.module.declare_func_in_func(sig.id, self.b.func);
		let call = self.b.ins().call(func, vals);
		let ret_val = if sig.ret.is_unit() {
			self.b.ins().iconst(self.int, 0)
		} else {
			self.b.inst_results(call)[0]
		};
		(ret_val, sig.ret.clone())
	}

	// Call through a value as a function.
	#[allow(clippy::too_many_arguments)]
	pub(super) fn call_value(
		&mut self,
		name: &str,
		callee: Value,
		env: Option<Value>,
		params: &[Typ],
		ret: &Typ,
		args: &[Spanned<Expr>],
		span: Span,
	) -> Result<TypedVal, Diagnostic> {
		if args.len() != params.len() {
			return Err(Diagnostic::new(
				format!("`{name}` expects {} argument(s), got {}", params.len(), args.len()),
				span.into_range(),
			)
			.with_label("wrong number of arguments"));
		}
		let mut vals = Vec::with_capacity(args.len() + 1);
		for (arg, want) in args.iter().zip(params) {
			let (val, typ) = self.check_expr(arg, want)?;
			if &typ != want {
				return Err(
					Diagnostic::new(format!("expected {want} argument, got {typ}"), arg.1.into_range())
						.with_label("wrong argument type"),
				);
			}
			vals.push(val);
		}
		let mut sig = self.module.make_signature();
		sig.params.extend(params.iter().map(|t| AbiParam::new(cl_type(t, self.int))));
		if let Some(env) = env {
			sig.params.push(AbiParam::new(self.int));
			vals.push(env);
		}
		let is_unit = ret.is_unit();
		if !is_unit {
			sig.returns.push(AbiParam::new(cl_type(ret, self.int)));
		}
		let sig_ref = self.b.import_signature(sig);
		let call = self.b.ins().call_indirect(sig_ref, callee, &vals);
		let ret_val = if is_unit {
			self.b.ins().iconst(self.int, 0)
		} else {
			self.b.inst_results(call)[0]
		};
		Ok((ret_val, ret.clone()))
	}

	pub(super) fn call_concat(&mut self, a: Value, b: Value) -> Value {
		let func = self.import_fn(runtime::STR_CONCAT, &[self.int, self.int], Some(self.int));
		let call = self.b.ins().call(func, &[a, b]);
		self.b.inst_results(call)[0]
	}

	pub(super) fn call_alloc(&mut self, n: usize) -> Value {
		self.call_alloc_bytes((n * 8) as i64)
	}

	pub(super) fn call_alloc_bytes(&mut self, bytes: i64) -> Value {
		let func = self.import_fn(runtime::ALLOC, &[self.int], Some(self.int));
		let size = self.b.ins().iconst(self.int, bytes);
		let call = self.b.ins().call(func, &[size]);
		self.b.inst_results(call)[0]
	}

	// Pack a value into an i64 slot for the map's fixed width.
	pub(super) fn map_bits(&mut self, val: Value) -> Value {
		let cl = self.b.func.dfg.value_type(val);
		let iv = if cl.is_float() {
			self.b.ins().bitcast(cl_int_for_width(cl.bits() as u16), MemFlags::new(), val)
		} else {
			val
		};
		if cl.bits() < 64 {
			self.b.ins().uextend(self.int, iv)
		} else {
			iv
		}
	}

	// Recover a value's native width/kind.
	pub(super) fn unmap_bits(&mut self, val: Value, typ: &Typ) -> Value {
		let cl = cl_type(typ, self.int);
		let iv = if cl.bits() < 64 {
			self.b.ins().ireduce(cl_int_for_width(cl.bits() as u16), val)
		} else {
			val
		};
		if cl.is_float() {
			self.b.ins().bitcast(cl, MemFlags::new(), iv)
		} else {
			iv
		}
	}

	// Type-check a map index against key type `key_typ`.
	pub(super) fn map_key(
		&mut self,
		index: &Spanned<Expr>,
		key_typ: &Typ,
	) -> Result<(runtime::Tag, Value), Diagnostic> {
		let tag = map_key_tag(key_typ).ok_or_else(|| {
			Diagnostic::new(format!("{key_typ} cannot be used as a map key"), index.1.into_range())
				.with_label("unsupported key type")
		})?;
		let (val, typ) = self.check_expr(index, key_typ)?;
		if &typ != key_typ {
			return Err(
				Diagnostic::new(format!("expected {key_typ} key, got {typ}"), index.1.into_range())
					.with_label("wrong key type"),
			);
		}
		Ok((tag, self.map_bits(val)))
	}

	pub(super) fn call_map_new(&mut self) -> Value {
		let func = self.import_fn(runtime::MAP_NEW, &[], Some(self.int));
		let call = self.b.ins().call(func, &[]);
		self.b.inst_results(call)[0]
	}

	pub(super) fn call_map_get(&mut self, map: Value, tag: runtime::Tag, bits: Value) -> Value {
		let func = self.import_fn(runtime::MAP_GET, &[self.int, self.int, self.int], Some(self.int));
		let tag_v = self.b.ins().iconst(self.int, tag as i64);
		let call = self.b.ins().call(func, &[map, tag_v, bits]);
		self.b.inst_results(call)[0]
	}

	pub(super) fn call_map_set(&mut self, map: Value, tag: runtime::Tag, bits: Value, value: Value) {
		let func = self.import_fn(runtime::MAP_SET, &[self.int, self.int, self.int, self.int], None);
		let tag_v = self.b.ins().iconst(self.int, tag as i64);
		self.b.ins().call(func, &[map, tag_v, bits, value]);
	}

	pub(super) fn call_map_delete(&mut self, map: Value, tag: runtime::Tag, bits: Value) {
		let func = self.import_fn(runtime::MAP_DELETE, &[self.int, self.int, self.int], None);
		let tag_v = self.b.ins().iconst(self.int, tag as i64);
		self.b.ins().call(func, &[map, tag_v, bits]);
	}
}
