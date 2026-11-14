//! A directly awaited block is a separate captured-parameter function.
use super::*;

impl Lower<'_, '_> {
	#[cold]
	#[inline(never)]
	pub(super) fn refuse_async_block(&mut self, do_move: bool, span: Span) -> Result<()> {
		let at = self.location(span);
		if self.opaque.is_some() {
			(self.resolve)(Resolve::AsyncBlockValue { do_move }, at)?;
		}
		Err(self.refused(span, Refusal::AsyncBlock))
	}

	#[inline(never)]
	pub(super) fn await_block(
		&mut self,
		block: &hir::ExprAsyncBlock<'_>,
		out: u32,
		span: Span,
	) -> Result<()> {
		let at = self.location(span);
		// Admission precedes every effect. Inspect identities in the checked
		// returned delta, not the spelling of variables or generated source.
		if block.do_move {
			if let Some(proof) = self.opaque {
				if let Some(variable) = proof.retained_capture(self.exprs, block.captures)? {
					(self.resolve)(Resolve::RetainedAsyncMove { variable }, at)?;
					return Err(self.refused(span, Refusal::AsyncBlock));
				}
			}
		}

		let function = (self.resolve)(Resolve::Script(block.hash), at)?;
		let mut arguments = Vec::try_with_capacity(block.captures.len())?;
		for &variable in block.captures {
			let source = self.variable(variable, span)?;
			let value = if block.do_move {
				let target = self.slot()?;
				let (args_start, args_len) = self.function.arguments(&[source])?;
				self.emit(
					Op::Call {
						out: target,
						function: super::super::code::CAPTURE_TAKE,
						args_start,
						args_len,
					},
					span,
				)?;
				self.moved.try_push((variable, span))?;
				target
			} else {
				source
			};
			arguments.try_push(value)?;
		}
		let (args_start, args_len) = self.function.arguments(&arguments)?;
		let call = self.transaction.script_call(function, args_len, at)?;
		self.emit(call.op(out, args_start), span)?;
		Ok(())
	}
}
