use super::*;

impl<'a> Translator<'a> {
	// Widen/narrow integers.
	// Sign-extend `val` to i64, clamp to `[low, hi]`.
	pub(super) fn clamp_to_width(
		&mut self,
		val: Value,
		extend_signed: bool,
		low: Option<(i64, bool)>,
		hi: i64,
		hi_unsigned: bool,
		target_cl: types::Type,
	) -> Value {
		let src_cl = self.b.func.dfg.value_type(val);
		let v64 = if src_cl == types::I64 {
			val
		} else if extend_signed {
			self.b.ins().sextend(types::I64, val)
		} else {
			self.b.ins().uextend(types::I64, val)
		};
		let v64 = match low {
			Some((low, lo_unsigned)) => {
				let lo_c = self.b.ins().iconst(types::I64, low);
				let cc = if lo_unsigned {
					IntCC::UnsignedLessThan
				} else {
					IntCC::SignedLessThan
				};
				let lt = self.b.ins().icmp(cc, v64, lo_c);
				self.b.ins().select(lt, lo_c, v64)
			}
			None => v64,
		};
		let hi_c = self.b.ins().iconst(types::I64, hi);
		let cc = if hi_unsigned {
			IntCC::UnsignedGreaterThan
		} else {
			IntCC::SignedGreaterThan
		};
		let gt = self.b.ins().icmp(cc, v64, hi_c);
		let v64 = self.b.ins().select(gt, hi_c, v64);
		if target_cl == types::I64 {
			v64
		} else {
			self.b.ins().ireduce(target_cl, v64)
		}
	}

	// Dispatch a call to a compiler builtin.
	pub(super) fn builtin_call(
		&mut self,
		name: &str,
		args: &[Spanned<Expr>],
		span: Span,
	) -> Result<Option<(Value, Typ)>, Diagnostic> {
		match name {
			"print" | "write" | "eprint" | "ewrite" => {
				if args.is_empty() {
					return Err(
						Diagnostic::new(format!("`{name}` takes at least 1 argument"), span.into_range())
							.with_label("missing argument"),
					);
				}
				let stderr = matches!(name, "eprint" | "ewrite");
				let newline = matches!(name, "print" | "eprint");
				for (i, arg) in args.iter().enumerate() {
					if i > 0 {
						self.write_lit(" ", stderr);
					}
					let (val, typ) = self.expr(arg)?;
					self.emit_print(val, &typ, false, stderr);
				}
				if newline {
					self.write_lit("\n", stderr);
				}
				Ok(Some((self.b.ins().iconst(self.int, 0), Typ::Tuple(vec![]))))
			}

			// TODO: migrate to `assert!` macro once we have macros
			"assert" => {
				if args.is_empty() || args.len() > 2 {
					return Err(Diagnostic::new(
						format!("`assert` takes 1 or 2 arguments, got {}", args.len()),
						span.into_range(),
					)
					.with_label("wrong number of arguments"));
				}
				let (cond, cond_typ) = self.expr(&args[0])?;
				if cond_typ != Typ::Bool {
					return Err(Diagnostic::new(
						format!("`assert` condition must be Bool, got {cond_typ}"),
						args[0].1.into_range(),
					)
					.with_label("not a Bool"));
				}
				let msg = if args.len() == 2 {
					let (msg_val, msg_typ) = self.expr(&args[1])?;
					if msg_typ != Typ::Str {
						return Err(Diagnostic::new(
							format!("`assert` message must be Str, got {msg_typ}"),
							args[1].1.into_range(),
						)
						.with_label("not a Str"));
					}
					msg_val
				} else {
					self.str_const("assertion failed")
				};

				let fail_block = self.b.create_block();
				let ok_block = self.b.create_block();
				self.b.ins().brif(cond, ok_block, &[], fail_block, &[]);
				self.b.seal_block(fail_block);
				self.b.seal_block(ok_block);

				self.b.switch_to_block(fail_block);
				let func = self.import_fn(runtime::ASSERT_FAIL, &[self.int], None);
				self.b.ins().call(func, &[msg]);
				self.b.ins().trap(TrapCode::HEAP_OUT_OF_BOUNDS);

				self.b.switch_to_block(ok_block);
				Ok(Some((cond, Typ::Bool)))
			}

			// TODO: migrate to `panic!` macro once we have macros
			"panic" => {
				if args.len() != 1 {
					return Err(Diagnostic::new(
						format!("`panic` takes 1 argument, got {}", args.len()),
						span.into_range(),
					)
					.with_label("wrong number of arguments"));
				}
				let (msg, msg_typ) = self.expr(&args[0])?;
				if msg_typ != Typ::Str {
					return Err(Diagnostic::new(
						format!("`panic` message must be Str, got {msg_typ}"),
						args[0].1.into_range(),
					)
					.with_label("not a Str"));
				}
				let func = self.import_fn(runtime::PANIC, &[self.int], None);
				self.b.ins().call(func, &[msg]);
				self.b.ins().trap(TrapCode::HEAP_OUT_OF_BOUNDS);

				// unreachable, but needed for codegen
				let dead = self.b.create_block();
				self.b.seal_block(dead);
				self.b.switch_to_block(dead);
				Ok(Some((self.b.ins().iconst(self.int, 0), Typ::Tuple(vec![]))))
			}

			"error" => {
				if args.len() != 1 {
					return Err(Diagnostic::new(
						format!("`error` takes 1 argument, got {}", args.len()),
						span.into_range(),
					)
					.with_label("wrong number of arguments"));
				}
				let (msg, msg_typ) = self.expr(&args[0])?;
				if msg_typ != Typ::Str {
					return Err(Diagnostic::new(
						format!("`error` message must be Str, got {msg_typ}"),
						args[0].1.into_range(),
					)
					.with_label("not a Str"));
				}
				Ok(Some((msg, Typ::Error)))
			}

			_ => self.cast_call(name, args, span),
		}
	}

	// A numeric cast builtin.
	pub(super) fn cast_call(
		&mut self,
		name: &str,
		args: &[Spanned<Expr>],
		span: Span,
	) -> Result<Option<(Value, Typ)>, Diagnostic> {
		// `int` and `float` are aliases for the default-width casts
		let name = match name {
			"int" => "i32",
			"float" => "f64",
			other => other,
		};

		if matches!(name, "isize" | "usize") {
			let signed = name == "isize";
			let (val, typ) = self.cast_operand(name, args, span)?;
			let out = match (&typ, signed) {
				(Typ::ISize, true) | (Typ::USize, false) => val,
				// isize -> usize: clamp negative to 0
				(Typ::ISize, false) => {
					let zero = self.b.ins().iconst(self.int, 0);
					let lt = self.b.ins().icmp(IntCC::SignedLessThan, val, zero);
					self.b.ins().select(lt, zero, val)
				}
				// usize -> isize: saturate at isize::MAX
				(Typ::USize, true) => {
					let max_v = self.b.ins().iconst(self.int, i64::MAX);
					let gt = self.b.ins().icmp(IntCC::UnsignedGreaterThan, val, max_v);
					self.b.ins().select(gt, max_v, val)
				}
				// int -> isize: sign-extend
				(Typ::Int(_), true) => {
					let src_cl = cl_type(&typ, self.int);
					if src_cl == self.int {
						val
					} else {
						self.b.ins().sextend(self.int, val)
					}
				}
				// uint -> usize: zero-extend
				(Typ::UInt(_), false) => {
					let src_cl = cl_type(&typ, self.int);
					if src_cl == self.int {
						val
					} else {
						self.b.ins().uextend(self.int, val)
					}
				}
				// int -> usize: sign-extend then clamp negative to 0
				(Typ::Int(_), false) => {
					let src_cl = cl_type(&typ, self.int);
					let v = if src_cl == self.int {
						val
					} else {
						self.b.ins().sextend(self.int, val)
					};
					let zero = self.b.ins().iconst(self.int, 0);
					let lt = self.b.ins().icmp(IntCC::SignedLessThan, v, zero);
					self.b.ins().select(lt, zero, v)
				}
				// uint -> isize: zero-extend then saturate at isize::MAX
				(Typ::UInt(_), true) => {
					let src_cl = cl_type(&typ, self.int);
					let v = if src_cl == self.int {
						val
					} else {
						self.b.ins().uextend(self.int, val)
					};
					let max_v = self.b.ins().iconst(self.int, i64::MAX);
					let gt = self.b.ins().icmp(IntCC::UnsignedGreaterThan, v, max_v);
					self.b.ins().select(gt, max_v, v)
				}
				_ => {
					return Err(
						Diagnostic::new(format!("cannot cast {typ} to {name}"), args[0].1.into_range())
							.with_label("not an integer"),
					);
				}
			};
			let out_typ = if signed { Typ::ISize } else { Typ::USize };
			return Ok(Some((out, out_typ)));
		}

		if let Some(target) = int_cast_width('i', name) {
			let (val, typ) = self.cast_operand(name, args, span)?;
			let target_cl = cl_type(&Typ::Int(target), self.int);
			let out = match &typ {
				Typ::Int(w) if *w == target => val,
				Typ::Int(_) => self.clamp_to_width(
					val,
					true,
					Some((int_min(target), false)),
					int_max(target),
					false,
					target_cl,
				),
				Typ::Enum(_) | Typ::Option(_) | Typ::Result(_) | Typ::AtomSum(_) => {
					let variants = self.variants_of(&typ);
					let tag = self.enum_tag(&variants, val);
					if target_cl == types::I64 {
						tag
					} else {
						self.b.ins().ireduce(target_cl, tag)
					}
				}
				_ => {
					return Err(
						Diagnostic::new(format!("cannot cast {typ} to i{target}"), args[0].1.into_range())
							.with_label("not an integer"),
					);
				}
			};
			return Ok(Some((out, Typ::Int(target))));
		}

		if let Some(target) = int_cast_width('u', name) {
			let (val, typ) = self.cast_operand(name, args, span)?;
			let target_cl = cl_type(&Typ::UInt(target), self.int);
			let out = match &typ {
				Typ::UInt(w) if *w == target => val,
				Typ::UInt(_) => self.clamp_to_width(val, false, None, uint_max(target), true, target_cl),
				Typ::Int(_) => self.clamp_to_width(val, true, Some((0, false)), uint_max(target), true, target_cl),
				_ => {
					return Err(
						Diagnostic::new(format!("cannot cast {typ} to u{target}"), args[0].1.into_range())
							.with_label("not an integer"),
					);
				}
			};
			return Ok(Some((out, Typ::UInt(target))));
		}

		if matches!(name, "f16" | "f32" | "f64" | "f128") {
			let target: u16 = match name {
				"f16" => 16,
				"f32" => 32,
				"f128" => 128,
				_ => 64,
			};
			if args.len() != 1 {
				return Err(
					Diagnostic::new(format!("`{name}` cast takes exactly 1 argument"), span.into_range())
						.with_label("wrong number of arguments"),
				);
			}
			if target == 16 || target == 128 {
				return Err(Diagnostic::new(
					format!("f{target} casts are not yet supported by the JIT backend"),
					span.into_range(),
				)
				.with_label("not yet implemented"));
			}
			let (val, typ) = self.expr(&args[0])?;
			let target_cl = cl_type(&Typ::Float(target), self.int);
			let out = match &typ {
				Typ::Float(w) if *w == target => val,
				Typ::Float(_) if target == 64 => self.b.ins().fpromote(types::F64, val),
				Typ::Float(_) => self.b.ins().fdemote(types::F32, val),
				Typ::Int(_) => self.b.ins().fcvt_from_sint(target_cl, val),
				_ => {
					return Err(
						Diagnostic::new(format!("cannot cast {typ} to f{target}"), args[0].1.into_range())
							.with_label("not a number"),
					);
				}
			};
			return Ok(Some((out, Typ::Float(target))));
		}

		Ok(None)
	}

	// Evaluate the sole operand of a single-argument cast.
	// Errors on wrong arity.
	pub(super) fn cast_operand(
		&mut self,
		name: &str,
		args: &[Spanned<Expr>],
		span: Span,
	) -> Result<(Value, Typ), Diagnostic> {
		if args.len() != 1 {
			return Err(
				Diagnostic::new(format!("`{name}` cast takes exactly 1 argument"), span.into_range())
					.with_label("wrong number of arguments"),
			);
		}
		self.expr(&args[0])
	}
}
