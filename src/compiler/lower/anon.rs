use super::*;
use crate::ast::Param;

impl<'a> Translator<'a> {
	// Declare an anon fn literal.
	pub(super) fn declare_anon_fn(
		&mut self,
		params: &[Param],
		params_tuple: bool,
		ret: &Spanned<TypeExpr>,
		body: &[Spanned<Expr>],
		span: Span,
	) -> Result<(Value, Typ), Diagnostic> {
		let def = GenericFnDef {
			params: params.to_vec(),
			params_tuple,
			ret: Some(ret.clone()),
			body: body.to_vec(),
			type_params: vec![],
		};
		let sig = self.declare_instance(&format!("anon${}", span.start), &def, HashMap::new(), span)?;
		let func_ref = self.module.declare_func_in_func(sig.id, self.b.func);
		let addr = self.b.ins().func_addr(self.int, func_ref);
		Ok((addr, Typ::Fn(sig.params, Box::new(sig.ret))))
	}
}
