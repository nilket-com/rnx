//! Direct string operations require the installed provider and a fixed receiver type.
use super::*;
use crate::function::string_builder::Operation;

impl Lower<'_, '_> {
	pub(super) fn proven_string(&self, id: hir::ExprId, fuel: &mut usize) -> bool {
		use hir::ExprKind as E;
		if *fuel == 0 {
			return false;
		}
		*fuel -= 1;
		let Some(expr) = self.exprs.get_checked(id) else {
			return false;
		};
		match expr.kind {
			E::Lit(hir::Lit::Str(_)) | E::Template(_) => true,
			E::Group(id) => self.proven_string(id, fuel),
			E::Block(b) if b.label.is_none() => {
				b.value.is_some_and(|i| self.proven_string(i, fuel))
			}
			E::Binary(b) if matches!(b.op, crate::ast::BinOp::Add(_)) => {
				self.proven_string(b.lhs, fuel) && self.proven_string(b.rhs, fuel)
			}
			E::Call(call) => match call.call {
				hir::Call::Meta { hash } => {
					Operation::constructor(hash).is_some_and(|op| self.context.string_builder(op))
						|| (matches!(hash, STRING_FROM | STRING_FROM_ASSOC)
							&& self.context.string_from())
						|| self.context.host(hash).is_some_and(|binding| {
							binding.signature.output
								== crate::host::HostOutputKind::Scalar(
									crate::host::HostType::String,
								)
						})
				}
				hir::Call::Associated {
					target,
					hash,
					name_hash,
				} if hash == name_hash
					&& hash == Hash::ident("clone")
					&& call.args.is_empty()
					&& !self.constants.method_script.closed(hash) =>
				{
					self.proven_string(target, fuel)
				}
				_ => false,
			},
			E::Variable(v) => {
				self.text_init(v)
					.is_some_and(|i| self.proven_string(i, fuel))
					&& self.exprs.expressions().all(|e| {
						let (mut lhs, rhs) = match e.kind {
							E::Assign(a) => (a.lhs, Some(a.rhs)),
							E::Binary(b)
								if b.op.is_assign()
									&& !matches!(b.op, crate::ast::BinOp::AddAssign(_)) =>
							{
								(b.lhs, None)
							}
							_ => return true,
						};
						let variable = loop {
							match self.exprs.get_checked(lhs).map(|e| &e.kind) {
								Some(E::Group(inner)) => lhs = *inner,
								Some(E::Variable(x)) => break Some(*x),
								Some(_) => break None,
								None => return false,
							}
						};
						variable != Some(v) || rhs.is_some_and(|i| self.proven_string(i, fuel))
					})
			}
			_ => false,
		}
	}

	#[cold]
	#[inline(never)]
	pub(super) fn call_string_builder(
		&mut self,
		call: &hir::ExprCall<'_>,
		target: Option<hir::ExprId>,
		op: Operation,
		out: u32,
		span: Span,
	) -> Result<()> {
		if !self.context.string_builder(op) {
			return Err(self.refused(
				span,
				if target.is_some() {
					Refusal::Method
				} else {
					Refusal::Native
				},
			));
		}
		let count = usize::from(target.is_some()) + call.args.len();
		if count != op.arity() as usize {
			return Err(self.refused(span, Refusal::Arity));
		}
		let mut ids = Vec::try_with_capacity(count)?;
		if let Some(id) = target {
			ids.try_push(id)?;
		}
		ids.try_extend_from_slice(call.args)?;
		let operands = self.operands(&ids)?;
		let (args_start, args_len) = self.function.arguments(&operands)?;
		self.emit(
			Op::Call {
				out,
				function: op.id(),
				args_start,
				args_len,
			},
			span,
		)?;
		Ok(())
	}
}

const STRING_FROM: Hash = crate::hash_in!(crate, ::std::string::String::from);
const STRING_FROM_ASSOC: Hash = crate::hash_in!(crate, ::std::string::String.from);
