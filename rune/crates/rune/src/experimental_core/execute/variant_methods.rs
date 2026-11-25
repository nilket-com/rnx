//! C17a Option and Result method calls. Operands were evaluated in source order.
use super::*;
use crate::experimental_core::heap::VariantOutcome;
use crate::function::variant::Operation;

impl Executor {
	#[cold]
	#[inline(never)]
	#[allow(clippy::too_many_arguments)]
	pub(super) fn variant_method_call(
		&mut self,
		code: &Code,
		store: &mut Store,
		function: &super::super::code::Function,
		base: usize,
		out: u32,
		method: Operation,
		start: u32,
		count: u32,
	) -> Result<()> {
		let invalid = || error(ErrorKind::InvalidCode);
		if count != method.arity() || out >= function.slots {
			return Err(invalid());
		}
		let end = start.checked_add(count).ok_or_else(invalid)?;
		let args = function
			.args
			.get(start as usize..end as usize)
			.ok_or_else(invalid)?;
		let read = |slot: u32| {
			if slot >= function.slots {
				return Err(invalid());
			}
			self.slots
				.get(base.checked_add(slot as usize).ok_or_else(invalid)?)
				.copied()
				.ok_or_else(invalid)
		};
		let receiver = read(args[0])?;
		let message = args.get(1).map(|slot| read(*slot)).transpose()?;
		let outcome = match method {
			Operation::Readable => VariantOutcome::Value(Value::Bool(store.is_readable(receiver)?)),
			Operation::CheckCallback => {
				let success = store.variant_success(receiver)?;
				store
					.check_callback_conversion(message.ok_or_else(invalid)?, TypeName::Function)?;
				VariantOutcome::Value(Value::Bool(success))
			}
			Operation::CheckReturn => {
				store.check_callback_conversion(receiver, TypeName::Result)?;
				let (tag, ok, child) = store.variant_parts(receiver)?;
				VariantOutcome::Value(store.variant(tag, ok, child)?)
			}
			Operation::Method(method) => store
				.variant_method(method, receiver, message, &self.format_limits)
				.map_err(|e| {
					// A wrong receiver needs the compile-time absence proof.
					match e.detail {
						Detail::MissingInstanceFunction { hash, .. }
							if !code.absent_method(hash) =>
						{
							invalid()
						}
						_ => e,
					}
				})?,
		};
		match outcome {
			VariantOutcome::Value(value) => self.put(
				store,
				base.checked_add(out as usize).ok_or_else(invalid)?,
				value,
			),
			VariantOutcome::Panic(message) => {
				self.pending_host_message = Some(message);
				Err(error(ErrorKind::Panic).with_detail(Detail::PanicMessage))
			}
		}
	}
}
