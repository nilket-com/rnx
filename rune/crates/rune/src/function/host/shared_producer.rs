//! Private bridge packets. The checked adapter gets a shared reference and a
//! byte allowance; it cannot retain a handle into the source Store.
use crate as rune;

#[derive(crate::Any)]
pub(crate) struct ProducerReceiver {
	pub(crate) receiver: crate::runtime::Value,
	pub(crate) remaining: usize,
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
