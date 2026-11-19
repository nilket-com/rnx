//! Private bridge packets. The checked adapter gets a shared reference and a
//! byte allowance; it cannot retain a handle into the source Store.
use crate as rune;

#[derive(crate::Any)]
pub(crate) struct ProducerReceiver {
	pub(crate) receiver: crate::runtime::Value,
	pub(crate) remaining: usize,
}

impl ProducerReceiver {
	pub(crate) fn produce<T: crate::Any, U: crate::Any>(
		self,
		producer: impl FnOnce(
			&T,
			usize,
		) -> Result<Result<U, std::string::String>, super::OpaqueProducerLimit>,
	) -> Result<crate::runtime::Value, crate::runtime::VmError> {
		use crate::runtime::{AnyObj, Value};
		let borrowed = self.receiver.borrow_ref::<T>()?;
		// The native result and its child coexist while the Store adopts them.
		let wrappers = AnyObj::owned_allocation_bytes::<U>()
			.max(AnyObj::owned_allocation_bytes::<crate::alloc::String>())
			+ AnyObj::owned_allocation_bytes::<Result<Value, Value>>();
		let Some(credit) = self.remaining.checked_sub(wrappers) else {
			return Ok(Value::new(super::OpaqueProducerLimit)?);
		};
		let result = match producer(&borrowed, credit) {
			Ok(Ok(value)) => Ok(Value::new(value)?),
			Ok(Err(message)) => Err(crate::to_value(message)?),
			Err(limit) => return Ok(Value::new(limit)?),
		};
		Ok(Value::try_from(result)?)
	}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloatDtype {
	F32,
	F64,
}

#[derive(Debug)]
pub enum SelectionError {
	NotNames,
	Count(usize),
	NameLength(usize),
	Duplicate(std::string::String),
	/// Already quoted and escaped under the call's byte budget.
	Dtype(std::string::String),
}

#[derive(crate::Any, Debug)]
pub struct ColumnSelection(
	pub Result<(std::vec::Vec<std::string::String>, FloatDtype), SelectionError>,
);
