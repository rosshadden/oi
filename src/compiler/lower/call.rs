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
	) -> Result<(Value, Typ), Diagnostic> {
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
	pub(super) fn emit_call(&mut self, sig: &FnSig, vals: &[Value]) -> (Value, Typ) {
		let func = self.module.declare_func_in_func(sig.id, self.b.func);
		let call = self.b.ins().call(func, vals);
		let ret_val = if matches!(sig.ret, Typ::Tuple(ref f) if f.is_empty()) {
			self.b.ins().iconst(self.int, 0)
		} else {
			self.b.inst_results(call)[0]
		};
		(ret_val, sig.ret.clone())
	}

	// Call through a `Typ::Fn` value.
	// ex: a variable holding an anon fn
	pub(super) fn call_value(
		&mut self,
		name: &str,
		callee: Value,
		params: &[Typ],
		ret: &Typ,
		args: &[Spanned<Expr>],
		span: Span,
	) -> Result<(Value, Typ), Diagnostic> {
		if args.len() != params.len() {
			return Err(Diagnostic::new(
				format!("`{name}` expects {} argument(s), got {}", params.len(), args.len()),
				span.into_range(),
			)
			.with_label("wrong number of arguments"));
		}
		let mut vals = Vec::with_capacity(args.len());
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
		let is_unit = matches!(ret, Typ::Tuple(f) if f.is_empty());
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
}
