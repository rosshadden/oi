use super::*;

impl<'a> Translator<'a> {
	pub(super) fn emit_eq(&mut self, a: Value, b: Value, typ: &Typ) -> Value {
		match typ {
			Typ::Float(_) => self.b.ins().fcmp(FloatCC::Equal, a, b),
			Typ::Str | Typ::Error => {
				let func = self.import_fn(runtime::STR_EQ, &[self.int, self.int], Some(self.int));
				let call = self.b.ins().call(func, &[a, b]);
				self.b.inst_results(call)[0]
			}
			_ => self.b.ins().icmp(IntCC::Equal, a, b),
		}
	}

	// Compare two boxed enums.
	// Checks that tags match, and for variants that every field matches
	pub(super) fn emit_enum_eq(&mut self, a: Value, b: Value, variants: &[VariantInfo]) -> Value {
		let ta = self.enum_tag(variants, a);
		let tb = self.enum_tag(variants, b);
		let tags_eq = self.b.ins().icmp(IntCC::Equal, ta, tb);
		let eq = self.b.declare_var(types::I8);
		self.b.def_var(eq, tags_eq);
		let merge = self.b.create_block();
		for v in variants.iter().filter(|v| !v.payload.is_empty()) {
			let disc = self.b.ins().iconst(self.int, v.disc);
			let same = self.b.ins().icmp(IntCC::Equal, ta, disc);
			let hit = self.b.ins().band(tags_eq, same);
			let (body, next) = (self.b.create_block(), self.b.create_block());
			self.b.ins().brif(hit, body, &[], next, &[]);
			self.b.seal_block(body);
			self.b.seal_block(next);
			self.b.switch_to_block(body);
			for (i, ft) in v.payload.iter().enumerate() {
				let fa = self
					.b
					.ins()
					.load(cl_type(ft, self.int), MemFlags::new(), a, ((i + 1) * 8) as i32);
				let fb = self
					.b
					.ins()
					.load(cl_type(ft, self.int), MemFlags::new(), b, ((i + 1) * 8) as i32);
				let fe = self.emit_eq(fa, fb, ft);
				let fe = self.b.ins().icmp_imm(IntCC::NotEqual, fe, 0);
				let prev = self.b.use_var(eq);
				let acc = self.b.ins().band(prev, fe);
				self.b.def_var(eq, acc);
			}
			self.b.ins().jump(merge, &[]);
			self.b.switch_to_block(next);
		}
		self.b.ins().jump(merge, &[]);
		self.b.switch_to_block(merge);
		self.b.seal_block(merge);
		self.b.use_var(eq)
	}

	// Sign-extend the low `w` bits of `val` within its Cranelift container.
	// A no-op for standard widths (8, 16, 32, 64).
	pub(super) fn reduce_int(&mut self, val: Value, w: u16) -> Value {
		let cl = cl_type(&Typ::Int(w), self.int);
		let shift = cl.bits() as i64 - w as i64;
		if shift == 0 {
			return val;
		}
		let shift_v = self.b.ins().iconst(cl, shift);
		let up = self.b.ins().ishl(val, shift_v);
		self.b.ins().sshr(up, shift_v)
	}

	// Zero-extend (mask) `val` to exactly `w` bits within its Cranelift container.
	pub(super) fn reduce_uint(&mut self, val: Value, w: u16) -> Value {
		let cl = cl_type(&Typ::UInt(w), self.int);
		if cl.bits() as u16 == w {
			return val;
		}
		let mask = ((1u64 << w) - 1) as i64;
		let mask_v = self.b.ins().iconst(cl, mask);
		self.b.ins().band(val, mask_v)
	}

	pub(super) fn binop(
		&mut self,
		op: Op,
		l: &Spanned<Expr>,
		r: &Spanned<Expr>,
		span: Span,
	) -> Result<(Value, Typ), Diagnostic> {
		let (lv, lt) = self.expr(l)?;
		let (rv, rt) = self.expr(r)?;

		// string concatenation
		if let (Op::Add, Typ::Str, Typ::Str) = (op, &lt, &rt) {
			return Ok((self.call_concat(lv, rv), Typ::Str));
		}

		#[derive(Clone, Copy)]
		enum NumKind {
			Int,
			UInt,
			Float,
		}
		// NOTE: might go with V-style int/float promotion eventually
		let kind = match (&lt, &rt) {
			(Typ::Int(lw), Typ::Int(rw)) if lw == rw => NumKind::Int,
			(Typ::ISize, Typ::ISize) => NumKind::Int,
			(Typ::UInt(lw), Typ::UInt(rw)) if lw == rw => NumKind::UInt,
			(Typ::USize, Typ::USize) => NumKind::UInt,
			(Typ::Float(lw), Typ::Float(rw)) if lw == rw => NumKind::Float,
			_ => {
				return Err(
					Diagnostic::new(format!("cannot apply `{op}` to {lt} and {rt}"), span.into_range())
						.with_label("operands have mismatched types"),
				);
			}
		};
		if let (Op::Mod, NumKind::Float) = (op, kind) {
			// TODO: cranelift has no float remainder
			return Err(
				Diagnostic::new("`%` is not yet supported on floats".to_string(), span.into_range())
					.with_label("only integer operands"),
			);
		}
		let b = self.b.ins();
		let out = match (op, kind) {
			(Op::Add, NumKind::Float) => b.fadd(lv, rv),
			(Op::Add, _) => b.iadd(lv, rv),
			(Op::Sub, NumKind::Float) => b.fsub(lv, rv),
			(Op::Sub, _) => b.isub(lv, rv),
			(Op::Mul, NumKind::Float) => b.fmul(lv, rv),
			(Op::Mul, _) => b.imul(lv, rv),
			(Op::Div, NumKind::Float) => b.fdiv(lv, rv),
			(Op::Div, NumKind::UInt) => b.udiv(lv, rv),
			(Op::Div, NumKind::Int) => b.sdiv(lv, rv),
			(Op::Mod, NumKind::Float) => unreachable!("float `%` rejected above"),
			(Op::Mod, NumKind::UInt) => b.urem(lv, rv),
			(Op::Mod, NumKind::Int) => b.srem(lv, rv),
		};
		// For non-standard widths, wrap the result back to the declared bit width.
		let out = match &lt {
			Typ::Int(w) if cl_type(&Typ::Int(*w), self.int).bits() as u16 != *w => self.reduce_int(out, *w),
			Typ::UInt(w) if cl_type(&Typ::UInt(*w), self.int).bits() as u16 != *w => self.reduce_uint(out, *w),
			_ => out,
		};
		Ok((out, lt))
	}

	pub(super) fn cmp(
		&mut self,
		icc: IntCC,
		fcc: FloatCC,
		l: &Spanned<Expr>,
		r: &Spanned<Expr>,
		span: Span,
	) -> Result<(Value, Typ), Diagnostic> {
		// evaluate the typed/pinned side first so a `.variant` shorthand can borrow its enum type
		let ((lv, lt), (rv, rt)) = if let Expr::EnumShorthand { .. } = &l.0 {
			let (rv, rt) = self.expr(r)?;
			(self.check_expr(l, &rt)?, (rv, rt))
		} else {
			let (lv, lt) = self.expr(l)?;
			let rhs = self.check_expr(r, &lt)?;
			((lv, lt), rhs)
		};

		// () == ()
		if let (Typ::Tuple(lf), Typ::Tuple(rf)) = (&lt, &rt)
			&& lf.is_empty()
			&& rf.is_empty()
		{
			let result = match icc {
				IntCC::Equal => self.b.ins().iconst(self.int, 1),
				IntCC::NotEqual => self.b.ins().iconst(self.int, 0),
				_ => {
					return Err(
						Diagnostic::new("unit type `()` only supports `==` and `!=`", span.into_range())
							.with_label("unsupported comparison"),
					);
				}
			};
			return Ok((result, Typ::Bool));
		}

		let icc = if matches!((&lt, &rt), (Typ::UInt(_), Typ::UInt(_)) | (Typ::USize, Typ::USize)) {
			unsigned_cc(icc)
		} else {
			icc
		};
		let raw = match (&lt, &rt) {
			(Typ::Int(_), Typ::Int(_))
			| (Typ::UInt(_), Typ::UInt(_))
			| (Typ::ISize, Typ::ISize)
			| (Typ::USize, Typ::USize)
			| (Typ::Bool, Typ::Bool)
			| (Typ::Atom, Typ::Atom) => self.b.ins().icmp(icc, lv, rv),
			(l, r) if l == r && matches!(l, Typ::Enum(_) | Typ::Option(_) | Typ::Result(_) | Typ::AtomSum(_)) => {
				let variants = self.variants_of(l);
				if !enum_boxed(&variants) {
					self.b.ins().icmp(icc, lv, rv)
				} else if let IntCC::Equal | IntCC::NotEqual = icc {
					let eq = self.emit_enum_eq(lv, rv, &variants);
					if icc == IntCC::Equal {
						eq
					} else {
						self.b.ins().icmp_imm(IntCC::Equal, eq, 0)
					}
				} else {
					return Err(Diagnostic::new(
						format!("only `==`&`!=` are supported because `{l}` has payloads"),
						span.into_range(),
					)
					.with_label("ordering needs a plain value"));
				}
			}
			(Typ::Float(_), Typ::Float(_)) => self.b.ins().fcmp(fcc, lv, rv),
			(Typ::Str, Typ::Str) if icc == IntCC::Equal || icc == IntCC::NotEqual => {
				let eq = self.emit_eq(lv, rv, &Typ::Str);
				// emit_eq returns 1 for equal, invert for Ne
				// wrap in icmp so uextend below works consistently
				if icc == IntCC::NotEqual {
					self.b.ins().icmp_imm(IntCC::Equal, eq, 0)
				} else {
					self.b.ins().icmp_imm(IntCC::NotEqual, eq, 0)
				}
			}
			_ => {
				return Err(
					Diagnostic::new(format!("cannot compare {lt} and {rt}"), span.into_range())
						.with_label("operands have mismatched types"),
				);
			}
		};
		let out = self.b.ins().uextend(self.int, raw);
		Ok((out, Typ::Bool))
	}

	// Short-circuits. `&&` only evaluates the right side when the left is true, and `||` does the inverse.
	pub(super) fn logical(
		&mut self,
		and: bool,
		l: &Spanned<Expr>,
		r: &Spanned<Expr>,
	) -> Result<(Value, Typ), Diagnostic> {
		let (lv, lt) = self.expr(l)?;
		if lt != Typ::Bool {
			return Err(Diagnostic::new(format!("expected Bool, got {lt}"), l.1.into_range())
				.with_label("logical operators need Bool operands"));
		}

		// result defaults to the short-circuit value
		let result = self.b.declare_var(self.int);
		let short = self.b.ins().iconst(self.int, if and { 0 } else { 1 });
		self.b.def_var(result, short);

		let rhs_block = self.b.create_block();
		let merge = self.b.create_block();
		let (then, els) = if and { (rhs_block, merge) } else { (merge, rhs_block) };
		self.b.ins().brif(lv, then, &[], els, &[]);

		self.b.switch_to_block(rhs_block);
		self.b.seal_block(rhs_block);
		let (rv, rt) = self.expr(r)?;
		if rt != Typ::Bool {
			return Err(Diagnostic::new(format!("expected Bool, got {rt}"), r.1.into_range())
				.with_label("logical operators need Bool operands"));
		}
		self.b.def_var(result, rv);
		self.b.ins().jump(merge, &[]);

		self.b.switch_to_block(merge);
		self.b.seal_block(merge);
		Ok((self.b.use_var(result), Typ::Bool))
	}
}
