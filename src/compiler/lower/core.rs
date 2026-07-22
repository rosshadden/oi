use super::*;

impl<'a> Translator<'a> {
	// The named types in scope, bundled for resolving type annotations.
	pub(super) fn types(&self) -> TypeCtx<'a> {
		TypeCtx::new(self.structs, self.enums, self.aliases, self.type_params, self.generics)
	}

	// Look up the binding that a mutation targets.
	pub(super) fn mutable_local(&self, name: &str, span: Range<usize>, op: Mutation) -> Result<Local, Diagnostic> {
		// how the mutation reads in errors
		// (verb, verb when immutable, noun for the `mut` hint, suggest `:=`?)
		let (verb, immutable_verb, allow, suggest_declare) = match op {
			Mutation::Assign => ("assign to", "assign to", "assignment", true),
			Mutation::IndexAssign => ("assign to", "assign to element of", "assignment", true),
			Mutation::Append => ("append to", "append to", "append", false),
			Mutation::FieldAssign => ("assign field of", "assign field of", "field assignment", false),
		};
		let local = self.vars.get(name).cloned().ok_or_else(|| {
			let d = Diagnostic::new(format!("cannot {verb} undefined variable `{name}`"), span.clone())
				.with_label("not found in scope");
			if suggest_declare {
				d.with_note(format!("declare it first with `{name} := ...`"))
			} else {
				d
			}
		})?;
		if !local.mutable {
			return Err(
				Diagnostic::new(format!("cannot {immutable_verb} immutable `{name}`"), span)
					.with_label("declared without `mut`")
					.with_note(format!("use `mut {name} := ...` to allow {allow}")),
			);
		}
		Ok(local)
	}

	// Look up a variable, or error if undefined.
	pub(super) fn local(&self, name: &str, span: Range<usize>) -> Result<Local, Diagnostic> {
		self.vars.get(name).cloned().ok_or_else(|| {
			Diagnostic::new(format!("undefined variable `{name}`"), span).with_label("not found in scope")
		})
	}

	// Promote a local to a heap-boxed cell.
	// Returns the cell's address.
	pub(super) fn box_local(&mut self, name: &str, local: &Local, span: Range<usize>) -> Result<Value, Diagnostic> {
		if local.boxed {
			return Ok(self.b.use_var(local.var));
		}
		if !local.mutable {
			return Err(Diagnostic::new(format!("cannot capture `{name}` as `mut`"), span)
				.with_label("declared without `mut`")
				.with_note(format!("use `mut {name} := ...` to allow mutation")));
		}
		let cell = self.call_alloc_bytes(8);
		let cur = self.read_local(local);
		self.b.ins().store(MemFlags::new(), cur, cell, 0);
		let var = self.b.declare_var(self.int);
		self.b.def_var(var, cell);
		self.vars.insert(
			name.to_string(),
			Local {
				var,
				boxed: true,
				mutable: true,
				typ: local.typ.clone(),
			},
		);
		Ok(cell)
	}

	// Read a local's value.
	// indirects through its box if it's a mutable capture.
	pub(super) fn read_local(&mut self, local: &Local) -> Value {
		let raw = self.b.use_var(local.var);
		if local.boxed {
			let cl = cl_type(&local.typ, self.int);
			self.b.ins().load(cl, MemFlags::new(), raw, 0)
		} else {
			raw
		}
	}

	// Write a local's value.
	// Indirects through its box if it's a mutable capture.
	pub(super) fn write_local(&mut self, local: &Local, val: Value) {
		if local.boxed {
			let ptr = self.b.use_var(local.var);
			self.b.ins().store(MemFlags::new(), val, ptr, 0);
		} else {
			self.b.def_var(local.var, val);
		}
	}

	pub(super) fn unit_value(&mut self) -> TypedVal {
		(self.b.ins().iconst(self.int, 0), Typ::unit())
	}

	// `$` implicit input
	// TODO: migrate to its own submodule. idk what to call it yet so putting it here. `sigils`?
	pub(super) fn dollar(&mut self) -> TypedVal {
		self.dollar.clone().expect("`bind_dollar` runs before the body is lowered")
	}

	// Determine type of `$` once params are bound.
	pub fn bind_dollar(&mut self, params_tuple: bool) {
		let locals = self.params.clone();
		let value = if !params_tuple {
			let local = &locals[0];
			(self.b.use_var(local.var), local.typ.clone())
		} else if locals.is_empty() {
			self.unit_value()
		} else {
			let ptr = self.call_alloc(locals.len());
			let fields = locals
				.iter()
				.enumerate()
				.map(|(i, local)| {
					let val = self.b.use_var(local.var);
					self.b.ins().store(MemFlags::new(), val, ptr, (i * 8) as i32);
					(None, local.typ.clone())
				})
				.collect();
			(ptr, Typ::Tuple(fields))
		};
		self.dollar = Some(value);
	}
}
