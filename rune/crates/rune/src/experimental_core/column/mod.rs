//! Borrowed column execution, separate from the interpreter's dispatch loop.
//!
//! Preparation inspects finalized execution instructions, never source patterns.
//! A refusal is a request for the adapter's ordinary scalar implementation, not
//! a script error. Inputs and speculative output do not enter the Store.

use super::frontend::Program;
use super::heap::Limits;
use std::vec::Vec;

mod lease;
mod run;
pub use run::{
	ColumnOptions, ColumnOutcome, ColumnResult, ColumnValues, Fault, Path,
	PreparedToken as PreparedBatch,
};
mod plan;
pub use lease::{BitSlice, Column, ColumnChunk, ColumnSlice};

/// Scalar types supported by the column contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnType {
	/// Checked signed integer arithmetic.
	I64,
	/// IEEE floating point, without reassociation.
	F64,
	/// Packed Boolean values, independent of validity.
	Bool,
}

/// Internal fast-path decisions. The adapter supplies public scalar errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnRefusal {
	/// A function or operation is outside the straight-line subset.
	Unsupported,
	/// The program has entry or formatting authority requiring the interpreter.
	EntryAuthority,
	/// A scalar type, definition or result does not match the contract.
	Type,
	/// Invalid length, offset, arity or identity.
	Bounds,
	/// Allocation or configured memory limit prevented an optional attempt.
	Memory,
	/// The original scalar work allowance is insufficient.
	Fuel,
	/// The adapter requested cancellation.
	Canceled,
	/// Checked arithmetic failed; replay supplies the exact public error.
	Arithmetic,
}

type Result<T> = core::result::Result<T, ColumnRefusal>;

/// Immutable verified graph. It contains no borrowed data or executable pointers.
/// Preparing a plan never changes the Program or an Executor.
#[derive(Debug)]
pub struct PreparedColumn {
	code: u32,
	function: u32,
	inputs: [ColumnType; 3],
	arity: usize,
	output: ColumnType,
	nodes: Vec<plan::Node>,
	result: usize,
	row_charge: usize,
	bytes: usize,
	limit: usize,
	frame_slots: usize,
	scratch_count: usize,
	fast: Option<plan::Fast>,
	special: bool,
	boolean_plan: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Driver {
	Boolean,
	F64,
	Predicate,
}

impl PreparedColumn {
	fn driver(&self) -> Driver {
		debug_assert!(self.special);
		let boolean = self.boolean_plan && self.output == ColumnType::Bool;
		let floats = !self.boolean_plan && self.output == ColumnType::F64;
		let predicate = !self.boolean_plan && self.output == ColumnType::Bool;
		debug_assert_eq!(
			usize::from(boolean) + usize::from(floats) + usize::from(predicate),
			1
		);
		if boolean {
			Driver::Boolean
		} else if floats {
			Driver::F64
		} else {
			Driver::Predicate
		}
	}

	/// Prepare a host-eligible function with one to three scalar arguments.
	/// `limits.bytes` is the adapter's existing memory allowance, not a new
	/// element cap. The adapter must deduct its other live owned storage first.
	pub fn prepare(
		program: &Program,
		function: u32,
		inputs: &[ColumnType],
		output: ColumnType,
		limits: Limits,
	) -> Result<Self> {
		plan::prepare(program, function, inputs, output, limits.bytes)
	}

	/// Whether this plan describes this exact compiled function.
	pub fn matches(&self, program: &Program, function: u32) -> bool {
		self.code == program.code.id() && self.function == function
	}

	/// Argument types in source order.
	pub fn input_types(&self) -> &[ColumnType] {
		&self.inputs[..self.arity]
	}

	/// The declared output type, checked against every instruction.
	pub fn output_type(&self) -> ColumnType {
		self.output
	}

	/// Exact finalized-code fuel charge for each non-null row.
	pub fn row_charge(&self) -> usize {
		self.row_charge
	}

	/// Actual retained plan storage, including its inline shell.
	pub fn retained_bytes(&self) -> usize {
		self.bytes
	}
}

fn reserved<T>(len: usize, room: usize) -> Result<Vec<T>> {
	if len
		.checked_mul(core::mem::size_of::<T>())
		.is_none_or(|n| n > room)
	{
		return Err(ColumnRefusal::Memory);
	}
	let mut values = Vec::new();
	values
		.try_reserve_exact(len)
		.map_err(|_| ColumnRefusal::Memory)?;
	if values
		.capacity()
		.checked_mul(core::mem::size_of::<T>())
		.is_none_or(|n| n > room)
	{
		return Err(ColumnRefusal::Memory);
	}
	crate::experimental_column_primitives::advise_huge_capacity(&mut values);
	Ok(values)
}

#[cfg(test)]
mod tests;

fn filled<T: Clone>(len: usize, value: T, room: usize) -> Result<Vec<T>> {
	let mut values = reserved(len, room)?;
	values.resize(len, value);
	Ok(values)
}
