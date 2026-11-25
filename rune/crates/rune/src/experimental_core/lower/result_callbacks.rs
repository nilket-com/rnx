//! Lower checked Result callbacks onto ordinary script frames.
use super::*;
use crate::function::variant::{Callback, Operation};

pub(crate) fn refusal(method: Callback, at: Location) -> Error {
	let mut error = Error::refused(Refusal::Method).at(at);
	error.operation = method.refusal();
	error
}

pub(crate) fn constructor(call: &hir::ExprCall<'_>) -> bool {
	matches!(call.call, hir::Call::Meta { hash } if call.args.len() == 1 &&
		(hash == crate::hash!(::std::result::Result::Ok) || hash == crate::hash!(::std::result::Result::Err)))
}

impl Lower<'_, '_> {
	pub(super) fn direct_result(&self, mut target: hir::ExprId) -> bool {
		for _ in 0..128 {
			let Some(expr) = self.exprs.get_checked(target) else {
				return false;
			};
			match expr.kind {
				hir::ExprKind::Group(inner) => target = inner,
				hir::ExprKind::Block(b) if b.label.is_none() => {
					let Some(value) = b.value else { return false };
					target = value;
				}
				hir::ExprKind::Call(call) => return constructor(call),
				_ => return false,
			}
		}
		false
	}

	fn callback_check(
		&mut self,
		check: Operation,
		args: &[u32],
		out: u32,
		span: Span,
	) -> Result<()> {
		let (args_start, args_len) = self.function.arguments(args)?;
		self.emit(
			Op::Call {
				out,
				function: check.id(),
				args_start,
				args_len,
			},
			span,
		)?;
		Ok(())
	}

	#[cold]
	#[inline(never)]
	pub(super) fn result_callback(
		&mut self,
		values: &[u32],
		method: Callback,
		out: u32,
		span: Span,
	) -> Result<()> {
		let &[receiver, function] = values else {
			return Err(refusal(method, self.location(span)));
		};
		let success = self.slot()?;
		self.callback_check(Operation::CheckCallback, values, success, span)?;
		let index = self.field_index(0, span)?;
		let payload = self.slot()?;
		self.emit(
			Op::TupleField {
				out: payload,
				value: receiver,
				index,
			},
			span,
		)?;
		let branch = self.emit(Op::Jump { target: 0 }, span)?;
		let yes = self.pc();
		let (args_start, args_len) = self.function.arguments(&[payload])?;
		let result = self.slot()?;
		self.emit(
			Op::CallValue {
				out: result,
				value: function,
				args_start,
				args_len,
			},
			span,
		)?;
		match method {
			Callback::Map => {
				self.emit(
					Op::Variant {
						out,
						tag: Type::Result,
						success: true,
						payload: Some(result),
					},
					span,
				)?;
			}
			Callback::AndThen => {
				self.callback_check(Operation::CheckReturn, &[result], out, span)?
			}
		}
		let jump = self.emit(Op::Jump { target: 0 }, span)?;
		let no = self.pc();
		self.emit(
			Op::Variant {
				out,
				tag: Type::Result,
				success: false,
				payload: Some(payload),
			},
			span,
		)?;
		self.function.patch(
			branch,
			Op::Branch {
				condition: success,
				yes,
				no,
			},
		)?;
		self.function.patch(jump, Op::Jump { target: self.pc() })
	}
}
