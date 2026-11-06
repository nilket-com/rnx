use super::{
	filled,
	lease::low_mask,
	plan::{Fast, Operand, Step},
	BitSlice, Column, ColumnRefusal, ColumnSlice, ColumnType, PreparedColumn,
};
use crate::experimental_core::{arithmetic::Binary, execute::ExecutorLimits};
use std::vec::Vec;

#[path = "masked.rs"]
mod masked;
use masked::{first_fault, float_operation, intersect_validity};
#[path = "operands.rs"]
mod operands;

#[path = "flat.rs"]
mod flat;
use flat::Output;

#[path = "boolean.rs"]
mod boolean;

const BLOCK: usize = 1024;

/// Request configuration. The work budget is copied, never consumed on refusal.
#[derive(Clone, Copy, Debug)]
pub struct ColumnOptions {
	/// Original scalar work allowance for the complete batch.
	pub work: usize,
	/// The same frame/slot limits as the scalar executor.
	pub executor_limits: ExecutorLimits,
	/// Portable implementation control, including hosts without SIMD.
	pub force_scalar: bool,
}

/// Executed implementation, for adapters' explain and measurement witnesses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Path {
	/// Precompiled portable primitive chain; no per-row VM invocation.
	Scalar,
	/// Runtime-selected AVX2 primitive.
	Avx2,
}

/// Owned initialized output. No value is returned while execution is speculative.
#[derive(Debug)]
pub enum ColumnValues {
	/// Signed integer output.
	I64(Vec<i64>),
	/// Floating point output.
	F64(Vec<f64>),
	/// Packed Boolean output; padding bits are zero.
	Bool(Vec<u8>),
}

/// A successful batch; the adapter may now transfer output storage to Arrow.
#[derive(Debug)]
pub struct ColumnResult {
	/// Owned values, with deterministic zero payloads at null positions.
	pub values: ColumnValues,
	/// Intersected validity or None for all-valid input chunks.
	pub validity: Option<Vec<u8>>,
	/// Logical row count (needed for packed output).
	pub len: usize,
	/// Number of valid rows computed by the fuel-reservation pass.
	pub valid_rows: usize,
	/// Exact remaining allowance after all non-null rows.
	pub remaining_work: usize,
	/// Actual primitive dispatch path.
	pub path: Path,
}

/// Diagnostic evidence only; replay supplies the interpreter's public failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fault {
	/// First failing logical row.
	pub row: usize,
	/// Original finalized-code PC within that row.
	pub pc: usize,
}

/// Terminal outcome of one attempt. Replay never carries partial output.
#[derive(Debug)]
pub enum ColumnOutcome {
	/// Fully validated output and committed fuel consumption.
	Complete(ColumnResult),
	/// Run the scalar adapter once from original inputs and budget, with the
	/// column path disabled for that entire replay.
	Replay {
		/// Why this optional attempt could not complete.
		reason: ColumnRefusal,
		/// Bounded first-fault evidence when arithmetic failed.
		fault: Option<Fault>,
	},
}

#[derive(Clone, Copy, Default)]
struct Cursor {
	chunk: usize,
	offset: usize,
}
impl Cursor {
	fn current<'a>(&mut self, column: Column<'a>) -> Option<super::ColumnChunk<'a>> {
		while let Some(chunk) = column.chunks.get(self.chunk) {
			if self.offset < chunk.values.len() {
				return Some(*chunk);
			}
			self.chunk += 1;
			self.offset = 0;
		}
		None
	}
}

fn storage_bytes(values: &ColumnValues) -> usize {
	match values {
		ColumnValues::I64(v) => v.capacity() * 8,
		ColumnValues::F64(v) => v.capacity() * 8,
		ColumnValues::Bool(v) => v.capacity(),
	}
}

/// A fully allocated private batch. Holding it borrows both plan and inputs;
/// dropping it publishes nothing and consumes no fuel. Allocation/validity work
/// is separate from invoke so kernel and end-to-end costs can be measured honestly.
#[derive(Debug)]
pub struct PreparedBatch<'plan, 'input> {
	plan: &'plan PreparedColumn,
	columns: &'input [Column<'input>],
	options: ColumnOptions,
	values: ColumnValues,
	validity: Option<Vec<u8>>,
	len: usize,
	charge: usize,
	valid_rows: usize,
	scratch: Vec<u64>,
	row_values: Vec<u64>,
}

/// Allocated batch token. Dispatch consumes it before entering either traversal.
#[derive(Debug)]
pub struct PreparedToken<'plan, 'input> {
	inner: BatchKind<'plan, 'input>,
}
#[derive(Debug)]
enum BatchKind<'plan, 'input> {
	Initialized(PreparedBatch<'plan, 'input>),
	Flat(flat::FlatBatch<'plan, 'input>),
}
impl PreparedToken<'_, '_> {
	/// Actual owned capacity, including this token's inline allocation metadata.
	pub fn owned_bytes(&self) -> usize {
		core::mem::size_of::<Self>()
			+ match &self.inner {
				BatchKind::Initialized(b) => b.owned_bytes() - core::mem::size_of_val(b),
				BatchKind::Flat(b) => b.storage_bytes(),
			}
	}
	/// Consume the token before entering the selected execution frame.
	#[inline]
	pub fn invoke(self, mut canceled: impl FnMut() -> bool) -> ColumnOutcome {
		match self.inner {
			BatchKind::Initialized(batch) => batch.invoke(canceled),
			BatchKind::Flat(batch) => match batch.run_flat(&mut canceled) {
				Ok(result) => ColumnOutcome::Complete(result),
				Err((reason, fault)) => ColumnOutcome::Replay { reason, fault },
			},
		}
	}
}

impl PreparedColumn {
	/// Prepare and execute one private batch. A refusal leaves original inputs
	/// and allowance untouched. Cancellation is checked at most every 1024 rows.
	pub fn execute<'input>(
		&self,
		columns: &'input [Column<'input>],
		options: ColumnOptions,
		mut canceled: impl FnMut() -> bool,
	) -> ColumnOutcome {
		match self.allocate_batch(columns, options, &mut canceled) {
			Ok(batch) => batch.invoke(canceled),
			Err((reason, fault)) => ColumnOutcome::Replay { reason, fault },
		}
	}

	/// Allocate private output and bounded scratch, intersect null masks and
	/// reserve (without consuming) exact fuel. No input pointer outlives this
	/// token. Adapters must replay from the original budget on any refusal.
	pub fn prepare_batch<'plan, 'input>(
		&'plan self,
		columns: &'input [Column<'input>],
		options: ColumnOptions,
		mut canceled: impl FnMut() -> bool,
	) -> super::Result<PreparedToken<'plan, 'input>> {
		self.allocate_batch(columns, options, &mut canceled)
			.map_err(|(reason, _)| reason)
	}

	fn allocate_batch<'plan, 'input>(
		&'plan self,
		columns: &'input [Column<'input>],
		options: ColumnOptions,
		canceled: &mut impl FnMut() -> bool,
	) -> core::result::Result<PreparedToken<'plan, 'input>, (ColumnRefusal, Option<Fault>)> {
		let allocation = |error| (error, None);
		if columns.len() != self.arity
			|| columns
				.iter()
				.zip(self.input_types())
				.any(|(c, ty)| c.ty != *ty)
		{
			return Err((ColumnRefusal::Type, None));
		}
		let len = columns[0].len;
		if columns.iter().any(|c| c.len != len) {
			return Err((ColumnRefusal::Bounds, None));
		}
		if options.executor_limits.frames == 0 || options.executor_limits.slots < self.frame_slots {
			return Err((ColumnRefusal::Memory, None));
		}
		if canceled() {
			return Err((ColumnRefusal::Canceled, None));
		}
		let mut room = self
			.limit
			.checked_sub(self.bytes)
			.and_then(|n| n.checked_sub(core::mem::size_of::<PreparedToken<'_, '_>>()))
			.ok_or((ColumnRefusal::Memory, None))?;
		let masked = columns
			.iter()
			.any(|c| c.chunks.iter().any(|chunk| chunk.validity.is_some()));
		let mut validity = if masked {
			Some(filled(len.div_ceil(8), 0u8, room).map_err(allocation)?)
		} else {
			None
		};
		if let Some(bits) = &validity {
			room = room
				.checked_sub(bits.capacity())
				.ok_or((ColumnRefusal::Memory, None))?;
		}
		let mut valid = len;
		if let Some(bits) = &mut validity {
			valid = intersect_validity(columns, bits, len, canceled).map_err(allocation)?;
		}

		let charge = valid
			.checked_mul(self.row_charge)
			.filter(|&n| n <= options.work)
			.ok_or((ColumnRefusal::Fuel, None))?;
		let values = if flat::eligible(self, columns, masked) {
			Output::reserve(self.output, len, room).map_err(allocation)?
		} else {
			Output::Initialized(match self.output {
				ColumnType::I64 => ColumnValues::I64(filled(len, 0i64, room).map_err(allocation)?),
				ColumnType::F64 => ColumnValues::F64(filled(len, 0f64, room).map_err(allocation)?),
				ColumnType::Bool => {
					ColumnValues::Bool(filled(len.div_ceil(8), 0u8, room).map_err(allocation)?)
				}
			})
		};
		room = room
			.checked_sub(values.bytes())
			.ok_or((ColumnRefusal::Memory, None))?;
		let scratch_len = self
			.scratch_count
			.checked_mul(if self.boolean_plan { BLOCK / 64 } else { BLOCK })
			.ok_or((ColumnRefusal::Memory, None))?;
		let scratch = filled(scratch_len, 0u64, room).map_err(allocation)?;
		room = room
			.checked_sub(scratch.capacity() * 8)
			.ok_or((ColumnRefusal::Memory, None))?;
		let row_values = filled(
			if self.boolean_plan {
				0
			} else {
				self.nodes.len()
			},
			0u64,
			room,
		)
		.map_err(allocation)?;
		let inner = match values {
			Output::Initialized(values) => BatchKind::Initialized(PreparedBatch {
				plan: self,
				columns,
				options,
				values,
				validity,
				len,
				charge,
				valid_rows: valid,
				scratch,
				row_values,
			}),
			values => BatchKind::Flat(flat::FlatBatch {
				plan: self,
				columns,
				options,
				values,
				validity,
				len,
				charge,
				valid_rows: valid,
				scratch,
				row_values,
			}),
		};
		Ok(PreparedToken { inner })
	}
}

impl PreparedBatch<'_, '_> {
	/// Actual private owned capacity, including the token shell but not the
	/// separately owned prepared plan or borrowed input buffers.
	pub fn owned_bytes(&self) -> usize {
		core::mem::size_of::<Self>()
			+ storage_bytes(&self.values)
			+ self.validity.as_ref().map_or(0, Vec::capacity)
			+ self.scratch.capacity() * 8
			+ self.row_values.capacity() * 8
	}

	/// Run exactly once; success transfers owned output, refusal destroys it.
	pub fn invoke(self, mut canceled: impl FnMut() -> bool) -> ColumnOutcome {
		let result = if self.plan.boolean_plan {
			boolean::run(self, &mut canceled)
		} else {
			self.run(&mut canceled)
		};
		match result {
			Ok(result) => ColumnOutcome::Complete(result),
			Err((reason, fault)) => ColumnOutcome::Replay { reason, fault },
		}
	}

	fn run(
		self,
		canceled: &mut impl FnMut() -> bool,
	) -> core::result::Result<ColumnResult, (ColumnRefusal, Option<Fault>)> {
		if self.validity.is_some() {
			return self.run_masked(canceled);
		}
		let Self {
			plan,
			columns,
			options,
			mut values,
			validity,
			len,
			charge,
			valid_rows,
			mut scratch,
			mut row_values,
		} = self;
		let mut cursors = [Cursor::default(); 3];
		let engine = crate::experimental_column_primitives::Engine::new(options.force_scalar);
		if let (Some(Fast::BoolNot(i)), ColumnValues::Bool(out)) = (plan.fast, &mut values) {
			if validity.is_none() {
				if let Some(result) = packed_not(columns[i], out, engine, canceled) {
					let path = result.map_err(|reason| (reason, None))?;
					return Ok(ColumnResult {
						values,
						validity,
						len,
						valid_rows,
						remaining_work: options.work - charge,
						path,
					});
				}
			}
		}
		let mut path = Path::Scalar;
		let mut row = 0;
		while row < len {
			if canceled() {
				return Err((ColumnRefusal::Canceled, None));
			}
			let mut count = (len - row).min(BLOCK);
			let mut inputs = [ColumnSlice::I64(&[]); 3];
			let mut offsets = [0; 3];
			for (i, column) in columns.iter().enumerate() {
				let chunk = cursors[i].current(*column).expect("verified column length");
				count = count.min(chunk.values.len() - cursors[i].offset);
				inputs[i] = chunk.values;
				offsets[i] = cursors[i].offset;
			}
			{
				let fast = match (plan.fast, &mut values) {
					(Some(Fast::I64Add(i, c)), ColumnValues::I64(out)) => {
						let ColumnSlice::I64(x) = inputs[i] else {
							unreachable!("verified type")
						};
						Some(engine.add(
							&x[offsets[i]..offsets[i] + count],
							c,
							&mut out[row..row + count],
						))
					}
					(Some(Fast::F64Add(i, c)), ColumnValues::F64(out)) => {
						let ColumnSlice::F64(x) = inputs[i] else {
							unreachable!("verified type")
						};
						engine.floats(
							&x[offsets[i]..offsets[i] + count],
							c,
							&mut out[row..row + count],
						);
						Some(false)
					}
					(Some(Fast::Fused(i, j)), ColumnValues::I64(out)) => {
						let (ColumnSlice::I64(x), ColumnSlice::I64(y)) = (inputs[i], inputs[j])
						else {
							unreachable!("verified type")
						};
						Some(engine.fused(
							&x[offsets[i]..offsets[i] + count],
							&y[offsets[j]..offsets[j] + count],
							&mut out[row..row + count],
						))
					}
					(
						Some(
							fast @ (Fast::F64PairAdd(..)
							| Fast::F64PairSub(..)
							| Fast::F64PairMul(..)),
						),
						ColumnValues::F64(out),
					) => {
						let (op, i, j) = float_operation(fast);
						let (ColumnSlice::F64(x), ColumnSlice::F64(y)) = (inputs[i], inputs[j])
						else {
							unreachable!("verified type")
						};
						engine.float_pair(
							op,
							&x[offsets[i]..offsets[i] + count],
							&y[offsets[j]..offsets[j] + count],
							&mut out[row..row + count],
						);
						Some(false)
					}
					(Some(Fast::BoolNot(i)), ColumnValues::Bool(out)) if row.is_multiple_of(8) => {
						let ColumnSlice::Bool(x) = inputs[i] else {
							unreachable!("verified type")
						};
						x.bytes_at(offsets[i], count).map(|x| {
							engine.bits(x, &mut out[row / 8..(row + count) / 8]);
							false
						})
					}
					_ => None,
				};
				if fast.is_some() && engine.avx2() {
					path = Path::Avx2;
				}
				let mut failed = fast.unwrap_or(false);
				if fast.is_none() {
					let (fault, simd) = operands::execute(
						plan,
						&inputs,
						&offsets,
						&mut scratch,
						&mut values,
						row,
						count,
						None,
						engine,
					);
					failed |= fault;
					if simd {
						path = Path::Avx2;
					}
				}
				if failed {
					return Err(first_fault(
						plan,
						&inputs,
						&offsets,
						row,
						count,
						None,
						&mut row_values,
					));
				}
			}
			row += count;
			for cursor in &mut cursors[..plan.arity] {
				cursor.offset += count;
			}
		}
		if canceled() {
			return Err((ColumnRefusal::Canceled, None));
		}
		Ok(ColumnResult {
			values,
			validity,
			len,
			valid_rows,
			remaining_work: options.work - charge,
			path,
		})
	}
}

// Select the typed chunk traversal before writing anything. A partial byte is
// permitted only at the end of the column; other layouts use the general zipper.
// The byte blocks retain the same 1024-logical-row cancellation bound.
#[inline(never)]
fn packed_not(
	column: Column<'_>,
	out: &mut [u8],
	engine: crate::experimental_column_primitives::Engine,
	canceled: &mut impl FnMut() -> bool,
) -> Option<core::result::Result<Path, ColumnRefusal>> {
	let mut remaining = column.len;
	for chunk in column.chunks {
		let ColumnSlice::Bool(bits) = chunk.values else {
			return None;
		};
		if bits.is_empty() {
			continue;
		}
		remaining -= bits.len();
		if bits.bytes_at(0, bits.len() / 8 * 8).is_none()
			|| (!bits.len().is_multiple_of(8) && remaining != 0)
		{
			return None;
		}
	}
	let mut path = Path::Scalar;
	let mut output = out;
	for chunk in column.chunks {
		let ColumnSlice::Bool(bits) = chunk.values else {
			unreachable!("verified packed input")
		};
		if bits.is_empty() {
			continue;
		}
		let bytes = bits
			.bytes_at(0, bits.len() / 8 * 8)
			.expect("verified alignment");
		let (full, rest) = output.split_at_mut(bytes.len());
		for (input, output) in bytes.chunks(BLOCK / 8).zip(full.chunks_mut(BLOCK / 8)) {
			if canceled() {
				return Some(Err(ColumnRefusal::Canceled));
			}
			engine.bits(input, output);
		}
		if !bytes.is_empty() && engine.avx2() {
			path = Path::Avx2;
		}
		if !bits.len().is_multiple_of(8) {
			if canceled() {
				return Some(Err(ColumnRefusal::Canceled));
			}
			let mut tail = 0;
			for bit in 0..bits.len() % 8 {
				tail |= u8::from(!bits.bit(bytes.len() * 8 + bit)) << bit;
			}
			rest[0] = tail;
		}
		output = rest;
	}
	if canceled() {
		return Some(Err(ColumnRefusal::Canceled));
	}
	Some(Ok(path))
}

fn input(values: ColumnSlice<'_>, index: usize) -> u64 {
	match values {
		ColumnSlice::I64(v) => v[index] as u64,
		ColumnSlice::F64(v) => v[index].to_bits(),
		ColumnSlice::Bool(v) => u64::from(v.bit(index)),
	}
}

pub(super) fn binary(op: Binary, ty: ColumnType, a: u64, b: u64) -> (u64, bool) {
	use Binary::*;
	macro_rules! compare {
		($a:expr, $b:expr) => {
			(
				u64::from(match op {
					Eq => $a == $b,
					Ne => $a != $b,
					Lt => $a < $b,
					Le => $a <= $b,
					Gt => $a > $b,
					Ge => $a >= $b,
					_ => unreachable!("verified comparison"),
				}),
				false,
			)
		};
	}
	match ty {
		ColumnType::I64 => {
			let (a, b) = (a as i64, b as i64);
			let (v, fault) = match op {
				Add => a.overflowing_add(b),
				Sub => a.overflowing_sub(b),
				Mul => a.overflowing_mul(b),
				_ => return compare!(a, b),
			};
			(v as u64, fault)
		}
		ColumnType::F64 => {
			let (a, b) = (f64::from_bits(a), f64::from_bits(b));
			let v = match op {
				Add => a + b,
				Sub => a - b,
				Mul => a * b,
				_ => return compare!(a, b),
			};
			(v.to_bits(), false)
		}
		ColumnType::Bool => (
			match op {
				BitAnd => a & b,
				BitOr => a | b,
				BitXor => a ^ b,
				Eq => u64::from(a == b),
				Ne => u64::from(a != b),
				_ => unreachable!("verified Boolean operation"),
			},
			false,
		),
	}
}
