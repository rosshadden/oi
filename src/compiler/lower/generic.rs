use super::*;

// Extend `subst` by matching a declared type against a concrete arg type.
fn unify(
	declared: &TypeExpr,
	concrete: &Typ,
	params: &[String],
	subst: &mut HashMap<String, Typ>,
) -> Result<(), String> {
	if let TypeExpr::Name(n) = declared
		&& params.contains(n)
	{
		return match subst.get(n) {
			Some(bound) if bound != concrete => Err(format!("`{n}` bound to both {bound} and {concrete}")),
			_ => {
				subst.insert(n.clone(), concrete.clone());
				Ok(())
			}
		};
	}
	match (declared, concrete) {
		(TypeExpr::Array(e), Typ::Array(c)) => unify(e, c, params, subst),
		(TypeExpr::FixedArray(e, n), Typ::FixedArray(c, cn)) if n == cn => unify(e, c, params, subst),
		(TypeExpr::Option(e), Typ::Option(c)) => unify(e, c, params, subst),
		(TypeExpr::Result(e), Typ::Result(c)) => unify(e, c, params, subst),
		(TypeExpr::Tuple(elems), Typ::Tuple(fields)) if elems.len() == fields.len() => {
			elems.iter().zip(fields).try_for_each(|(e, (_, f))| unify(e, f, params, subst))
		}
		// a non-param name, atom-sum, etc: trust it, the call emits against the real signature anyway
		_ => Ok(()),
	}
}

// A monomorph cache key.
fn mangle(name: &str, subst: &HashMap<String, Typ>, order: &[String]) -> String {
	let mut sym = format!("oi_{name}");
	for p in order {
		sym.push('$');
		sym.push_str(&subst[p].to_string());
	}
	sym
}

impl<'a> Translator<'a> {
	// Emit a call to a generic fn, declaring a monomorphed instance on first use.
	pub(super) fn call_generic(
		&mut self,
		name: &str,
		def: &GenericFnDef,
		args: &[Spanned<Expr>],
		span: Span,
	) -> Result<(Value, Typ), Diagnostic> {
		if args.len() != def.params.len() {
			return Err(Diagnostic::new(
				format!("`{name}` expects {} argument(s), got {}", def.params.len(), args.len()),
				span.into_range(),
			)
			.with_label("wrong number of arguments"));
		}
		let mut subst = HashMap::new();
		let mut vals = Vec::with_capacity(args.len());
		for (arg, param) in args.iter().zip(&def.params) {
			let (val, typ) = self.expr(arg)?;
			unify(&param.typ, &typ, &def.type_params, &mut subst)
				.map_err(|msg| Diagnostic::new(msg, arg.1.into_range()).with_label("type mismatch"))?;
			vals.push(val);
		}
		if let Some(missing) = def.type_params.iter().find(|p| !subst.contains_key(*p)) {
			return Err(
				Diagnostic::new(format!("cannot infer type parameter `{missing}`"), span.into_range())
					.with_label("not determined by any argument"),
			);
		}
		let sig = self.declare_instance(name, def, subst, span)?;
		Ok(self.emit_call(&sig, &vals))
	}

	// Declare a monomorphed instance's signature, reusing a prior one if it exists.
	pub(super) fn declare_instance(
		&mut self,
		name: &str,
		def: &GenericFnDef,
		subst: HashMap<String, Typ>,
		span: Span,
	) -> Result<FnSig, Diagnostic> {
		let sym = mangle(name, &subst, &def.type_params);
		if let Some(sig) = self.mono.get(&sym) {
			return Ok(sig.clone());
		}
		let Some((ret_te, ret_span)) = &def.ret else {
			return Err(Diagnostic::new(
				format!("generic function `{name}` needs an explicit return type"),
				span.into_range(),
			)
			.with_label("called here"));
		};

		let types = TypeCtx {
			structs: self.structs,
			enums: self.enums,
			aliases: self.aliases,
			type_params: &subst,
		};
		let params: Vec<(String, Typ, bool)> = def
			.params
			.iter()
			.map(|p| Ok((p.name.clone(), types.resolve(&p.typ, p.span)?, p.mutable)))
			.collect::<Result<_, Diagnostic>>()?;
		let ret = types.resolve(ret_te, *ret_span)?;

		let mut sig = self.module.make_signature();
		sig.params
			.extend(params.iter().map(|(_, t, _)| AbiParam::new(cl_type(t, self.int))));
		if !matches!(ret, Typ::Tuple(ref f) if f.is_empty()) {
			sig.returns.push(AbiParam::new(cl_type(&ret, self.int)));
		}
		let id = self
			.module
			.declare_function(&sym, Linkage::Local, &sig)
			.expect("declare function");
		let fn_sig = FnSig {
			id,
			params: params.into_iter().map(|(_, t, _)| t).collect(),
			ret,
		};
		self.mono.insert(sym.clone(), fn_sig.clone());
		self.pending.push((sym, def.clone(), subst));
		Ok(fn_sig)
	}
}
