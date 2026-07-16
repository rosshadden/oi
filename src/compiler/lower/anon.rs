use super::*;
use crate::ast::{Capture, Param};

impl<'a> Translator<'a> {
	// Declare an anon fn literal.
	#[allow(clippy::too_many_arguments)]
	pub(super) fn declare_anon_fn(
		&mut self,
		captures: &[Capture],
		params: &[Param],
		params_tuple: bool,
		ret: &Spanned<TypeExpr>,
		body: &[Spanned<Expr>],
		span: Span,
	) -> Result<(Value, Typ), Diagnostic> {
		let mut resolved = Vec::with_capacity(captures.len());
		for c in captures {
			let (name, boxed) = match c {
				Capture::Mut(name) => (name, true),
				Capture::ReadOnly(name) | Capture::Move(name) => (name, false),
			};
			let local = self.local(name, span.into_range())?;
			let val = if boxed {
				self.box_local(name, &local, span.into_range())?
			} else {
				self.read_local(&local)
			};
			resolved.push((name.clone(), local.typ, boxed, val));
		}

		let def = GenericFnDef {
			params: params.to_vec(),
			params_tuple,
			ret: Some(ret.clone()),
			body: body.to_vec(),
			type_params: vec![],
			captures: resolved.iter().map(|(n, t, boxed, _)| (n.clone(), t.clone(), *boxed)).collect(),
		};
		let sig = self.declare_instance(&format!("anon${}", span.start), &def, HashMap::new(), span)?;
		let func_ref = self.module.declare_func_in_func(sig.id, self.b.func);
		let addr = self.b.ins().func_addr(self.int, func_ref);
		if resolved.is_empty() {
			return Ok((addr, Typ::Fn(sig.params, Box::new(sig.ret))));
		}

		let env = self.call_alloc_bytes(((1 + resolved.len()) * 8) as i64);
		self.b.ins().store(MemFlags::new(), addr, env, 0);
		for (i, (_, _, _, val)) in resolved.iter().enumerate() {
			self.b.ins().store(MemFlags::new(), *val, env, ((i + 1) * 8) as i32);
		}
		Ok((env, Typ::Closure(sig.params, Box::new(sig.ret))))
	}
}
