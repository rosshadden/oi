use super::*;
use crate::ast::{Capture, Param};

impl<'a> Translator<'a> {
	// Declare an anon fn literal.
	pub(super) fn declare_anon_fn(
		&mut self,
		captures: &Option<Vec<Capture>>,
		params: &[Param],
		params_tuple: bool,
		ret: &Spanned<TypeExpr>,
		body: &[Spanned<Expr>],
		span: Span,
	) -> Result<TypedVal, Diagnostic> {
		let inferred;
		let captures: &[Capture] = match captures {
			Some(list) => list,
			None => {
				let mut names: Vec<_> = free_vars(body)
					.into_iter()
					.filter(|n| self.vars.contains_key(n) && !params.iter().any(|p| &p.name == n))
					.collect();
				names.sort();
				inferred = names.into_iter().map(Capture::ReadOnly).collect::<Vec<_>>();
				&inferred
			}
		};
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

// Every identifier referenced in `body`.
fn free_vars(body: &[Spanned<Expr>]) -> HashSet<String> {
	let mut out = HashSet::new();
	body.iter().for_each(|(e, _)| collect(e, &mut out));
	out
}

fn collect(expr: &Expr, out: &mut HashSet<String>) {
	use Expr::*;
	if let Ident(name) = expr {
		out.insert(name.clone());
	}
	if let AnonFn {
		captures: Some(list), ..
	} = expr
	{
		for c in list {
			let (Capture::ReadOnly(n) | Capture::Mut(n) | Capture::Move(n)) = c;
			out.insert(n.clone());
		}
	}
	let mut child = |e: &Spanned<Expr>| collect(&e.0, out);
	match expr {
		Bind { value: Some(v), .. } => child(v),
		Assign { value, .. } | FieldAssign { value, .. } | Append { value, .. } => child(value),
		IndexAssign { index, value, .. } => {
			child(index);
			child(value);
		}
		OptionInit { arg, .. } | ResultInit { arg, .. } => child(arg),
		Call { args, .. } | Array(args) | EnumShorthand { args, .. } => args.iter().for_each(&mut child),
		MethodCall { recv, args, .. } => {
			child(recv);
			args.iter().for_each(&mut child);
		}
		Return(Some(v)) | Negative(v) | Not(v) | Propagate(v) => child(v),
		If { cond, then, els } => {
			child(cond);
			then.iter().for_each(&mut child);
			els.iter().flatten().for_each(&mut child);
		}
		Loop { cond, body } => {
			cond.iter().for_each(|c| child(c));
			body.iter().for_each(&mut child);
		}
		For { iter, body, .. } => {
			child(iter);
			body.iter().for_each(&mut child);
		}
		Tuple(elems) | StructLit { fields: elems, .. } => elems.iter().for_each(|(_, e)| child(e)),
		Field { tuple, .. } => child(tuple),
		Index { collection, index } => {
			child(collection);
			child(index);
		}
		Slice { collection, start, end } => {
			child(collection);
			start.iter().for_each(|s| child(s));
			end.iter().for_each(|e| child(e));
		}
		Range { start, end } => {
			start.iter().for_each(|s| child(s));
			end.iter().for_each(|e| child(e));
		}
		Match {
			subject,
			arms,
			else_body,
		} => {
			child(subject);
			for arm in arms {
				arm.patterns.iter().for_each(&mut child);
				arm.body.iter().for_each(&mut child);
			}
			else_body.iter().flatten().for_each(&mut child);
		}
		OrElse { value, body } => {
			child(value);
			body.iter().for_each(&mut child);
		}
		Pipe { value, step } => {
			child(value);
			child(step);
		}
		AnonFn { body, .. } => body.iter().for_each(&mut child),
		Binary(_, a, b) => {
			child(a);
			child(b);
		}
		_ => {}
	}
}
