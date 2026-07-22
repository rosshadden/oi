use super::*;
use crate::ast::TypeParam;

// Extend `subst` by matching a declared type against a concrete arg type.
pub(super) fn unify(
	declared: &TypeExpr,
	concrete: &Typ,
	params: &[TypeParam],
	subst: &mut HashMap<String, Typ>,
	generics: &Generics,
) -> Result<(), String> {
	if let TypeExpr::Name(n) = declared
		&& params.iter().any(|p| &p.name == n)
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
		(TypeExpr::Array(e), Typ::Array(c)) => unify(e, c, params, subst, generics),
		(TypeExpr::FixedArray(e, n), Typ::FixedArray(c, cn)) if n == cn => unify(e, c, params, subst, generics),
		(TypeExpr::Option(e), Typ::Option(c)) => unify(e, c, params, subst, generics),
		(TypeExpr::Result(e, _), Typ::Result(c)) => unify(e, c, params, subst, generics),
		(TypeExpr::Tuple(elems), Typ::Tuple(fields)) if elems.len() == fields.len() => elems
			.iter()
			.zip(fields)
			.try_for_each(|(e, (_, f))| unify(e, f, params, subst, generics)),
		(TypeExpr::Generic(_, gargs), Typ::Struct(sname, _)) => {
			let cached = generics.instance_args.borrow().get(sname).cloned();
			match cached {
				Some(cargs) => gargs
					.iter()
					.zip(&cargs)
					.try_for_each(|(g, c)| unify(g, c, params, subst, generics)),
				None => Ok(()),
			}
		}
		// a non-param name, atom-sum, etc: trust it, the call emits against the real signature anyway
		_ => Ok(()),
	}
}

// A monomorph cache key.
fn mangle(name: &str, subst: &HashMap<String, Typ>, order: &[TypeParam]) -> String {
	let mut sym = format!("oi_{}", name.replace('.', "__"));
	for p in order {
		sym.push('$');
		sym.push_str(&subst[&p.name].to_string());
	}
	sym
}

impl<'a> Translator<'a> {
	// Emit a call to a generic fn, declaring a monomorphed instance on first use.
	pub(super) fn call_generic(
		&mut self,
		name: &str,
		def: &GenericFnDef,
		type_args: &[Spanned<TypeExpr>],
		args: &[Spanned<Expr>],
		recv: Option<(Value, Typ)>,
		span: Span,
	) -> Result<(Value, Typ), Diagnostic> {
		let self_n = recv.is_some() as usize;
		if args.len() + self_n != def.params.len() {
			return Err(Diagnostic::new(
				format!(
					"`{name}` expects {} argument(s), got {}",
					def.params.len() - self_n,
					args.len()
				),
				span.into_range(),
			)
			.with_label("wrong number of arguments"));
		}
		let mut subst = HashMap::new();
		if !type_args.is_empty() && type_args.len() != def.type_params.len() {
			return Err(Diagnostic::new(
				format!(
					"`{name}` expects {} type argument(s), got {}",
					def.type_params.len(),
					type_args.len()
				),
				span.into_range(),
			)
			.with_label("wrong number of type arguments"));
		}
		for (param, (te, te_span)) in def.type_params.iter().zip(type_args) {
			subst.insert(param.name.clone(), self.types().resolve(te, *te_span)?);
		}
		let mut vals = Vec::with_capacity(args.len() + self_n);
		let mut declared = def.params.iter();
		if let Some((rval, rtyp)) = &recv {
			let rparam = declared.next().unwrap();
			unify(&rparam.typ, rtyp, &def.type_params, &mut subst, self.generics)
				.map_err(|msg| Diagnostic::new(msg, span.into_range()).with_label("type mismatch"))?;
			vals.push(*rval);
		}
		for (arg, param) in args.iter().zip(declared) {
			let (val, typ) = self.expr(arg)?;
			unify(&param.typ, &typ, &def.type_params, &mut subst, self.generics)
				.map_err(|msg| Diagnostic::new(msg, arg.1.into_range()).with_label("type mismatch"))?;
			vals.push(val);
		}
		if let Some(missing) = def.type_params.iter().find(|p| !subst.contains_key(&p.name)) {
			return Err(Diagnostic::new(
				format!("cannot infer type parameter `{}`", missing.name),
				span.into_range(),
			)
			.with_label("not determined by any argument"));
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

		let types = TypeCtx::new(self.structs, self.enums, self.aliases, &subst, self.generics);
		let params = types.resolve_params(&def.params)?;
		let ret = types.resolve(ret_te, *ret_span)?;

		let mut sig = self.module.make_signature();
		sig.params
			.extend(params.iter().map(|(_, t, _)| AbiParam::new(cl_type(t, self.int))));
		if !def.captures.is_empty() {
			sig.params.push(AbiParam::new(self.int));
		}
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
