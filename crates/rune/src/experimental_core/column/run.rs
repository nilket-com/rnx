use super::{
	filled,
	lease::low_mask,
	plan::{Fast, Step},
	BitSlice, Column, ColumnRefusal, ColumnSlice, ColumnType, PreparedColumn,
};
use crate::experimental_core::{arithmetic::Binary, execute::ExecutorLimits};
use std::vec::Vec;

#[path = "masked.rs"]
mod masked;
use masked::{
	copy_input, copy_output, first_fault, float_operation, intersect_validity, split_binary,
};

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
	scratch: Vec<u64>,
	row_values: Vec<u64>,
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
	) -> super::Result<PreparedBatch<'plan, 'input>> {
		self.allocate_batch(columns, options, &mut canceled)
			.map_err(|(reason, _)| reason)
	}

	fn allocate_batch<'plan, 'input>(
		&'plan self,
		columns: &'input [Column<'input>],
		options: ColumnOptions,
		canceled: &mut impl FnMut() -> bool,
	) -> core::result::Result<PreparedBatch<'plan, 'input>, (ColumnRefusal, Option<Fault>)> {
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
			.and_then(|n| n.checked_sub(core::mem::size_of::<PreparedBatch<'_, '_>>()))
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
		let values = match self.output {
			ColumnType::I64 => ColumnValues::I64(filled(len, 0i64, room).map_err(allocation)?),
			ColumnType::F64 => ColumnValues::F64(filled(len, 0f64, room).map_err(allocation)?),
			ColumnType::Bool => {
				ColumnValues::Bool(filled(len.div_ceil(8), 0u8, room).map_err(allocation)?)
			}
		};
		room = room
			.checked_sub(storage_bytes(&values))
			.ok_or((ColumnRefusal::Memory, None))?;
		let scratch_len = self
			.scratch_count
			.checked_mul(BLOCK)
			.ok_or((ColumnRefusal::Memory, None))?;
		let scratch = filled(scratch_len, 0u64, room).map_err(allocation)?;
		room = room
			.checked_sub(scratch.capacity() * 8)
			.ok_or((ColumnRefusal::Memory, None))?;
		let row_values = filled(self.nodes.len(), 0u64, room).map_err(allocation)?;
		Ok(PreparedBatch {
			plan: self,
			columns,
			options,
			values,
			validity,
			len,
			charge,
			scratch,
			row_values,
		})
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
		match self.run(&mut canceled) {
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
				let mut direct_output = false;
				if fast.is_none() {
					for (index, node) in plan.nodes.iter().enumerate() {
						let out = node.buffer * BLOCK;
						match node.step {
							Step::Input(i) => {
								copy_input(inputs[i], offsets[i], &mut scratch[out..out + count]);
							}
							Step::Constant(v) => scratch[out..out + count].fill(v),
							Step::Not(a) => {
								let a = plan.nodes[a].buffer * BLOCK;
								for j in 0..count {
									scratch[out + j] = u64::from(scratch[a + j] == 0);
								}
							}
							Step::Binary(op, a, b) => {
								let ty = plan.nodes[a].ty;
								let literal = if ty == ColumnType::I64 && op == Binary::Mul {
									match (plan.nodes[a].step, plan.nodes[b].step) {
										(_, Step::Constant(c)) => Some((false, c as i64)),
										(Step::Constant(c), _) => Some((true, c as i64)),
										_ => None,
									}
								} else {
									None
								};
								let a = plan.nodes[a].buffer * BLOCK;
								let b = plan.nodes[b].buffer * BLOCK;

								if index == plan.result
									&& index + 1 == plan.nodes.len()
									&& ty == ColumnType::I64 && op == Binary::Add
								{
									let ColumnValues::I64(output) = &mut values else {
										unreachable!("verified integer result")
									};
									failed |= engine.add_pair_output(
										&scratch[a..a + count],
										&scratch[b..b + count],
										&mut output[row..row + count],
									);
									direct_output = true;
								} else if let Some((swap, c)) = literal {
									let (x, y, output) =
										split_binary(&mut scratch, a, b, out, count);
									failed |=
										engine.mul_constant(if swap { y } else { x }, c, output);
									if engine.avx2() && c != 0 && c.unsigned_abs() < (1u64 << 32) {
										path = Path::Avx2;
									}
								} else {
									failed |= apply_binary(
										op,
										ty,
										&mut scratch,
										[a, b, out],
										count,
										engine,
									);
								}
								if ty == ColumnType::I64 && op == Binary::Add && engine.avx2() {
									path = Path::Avx2;
								}
							}
						}
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

				if fast.is_none() && !direct_output {
					let start = plan.nodes[plan.result].buffer * BLOCK;
					copy_output(&scratch[start..start + count], &mut values, row);
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

fn binary(op: Binary, ty: ColumnType, a: u64, b: u64) -> (u64, bool) {
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

// Dispatch is per primitive/block. None of the element loops selects an opcode.
fn apply_binary(
	op: Binary,
	ty: ColumnType,
	slots: &mut [u64],
	buffers: [usize; 3],
	len: usize,
	engine: crate::experimental_column_primitives::Engine,
) -> bool {
	use Binary::*;
	let [a, b, out] = buffers;
	let (x, y, output) = split_binary(slots, a, b, out, len);
	macro_rules! apply {
		($f:expr) => {{
			let mut failed = false;
			for ((&a, &b), out) in x.iter().zip(y).zip(output) {
				let (v, fault) = ($f)(a, b);
				*out = v;
				failed |= fault;
			}
			failed
		}};
	}
	macro_rules! int {
		($method:ident) => {
			apply!(|a: u64, b: u64| {
				let (v, f) = (a as i64).$method(b as i64);
				(v as u64, f)
			})
		};
	}
	macro_rules! cmp {
		($decode:expr, $op:tt) => { apply!(|a: u64, b: u64| (u64::from(($decode)(a) $op ($decode)(b)), false)) };
	}
	macro_rules! numeric {
		($decode:expr) => { match op { Eq => cmp!($decode, ==), Ne => cmp!($decode, !=), Lt => cmp!($decode, <), Le => cmp!($decode, <=), Gt => cmp!($decode, >), Ge => cmp!($decode, >=), _ => unreachable!("verified comparison") } };
	}
	match (ty, op) {
		(ColumnType::I64, Add) => engine.add_pair(x, y, output),
		(ColumnType::I64, Sub) => int!(overflowing_sub),
		(ColumnType::I64, Mul) => crate::experimental_column_primitives::checked_mul(x, y, output),
		(ColumnType::I64, _) => numeric!(|v: u64| v as i64),
		(ColumnType::F64, Add) => {
			apply!(|a, b| ((f64::from_bits(a) + f64::from_bits(b)).to_bits(), false))
		}
		(ColumnType::F64, Sub) => {
			apply!(|a, b| ((f64::from_bits(a) - f64::from_bits(b)).to_bits(), false))
		}
		(ColumnType::F64, Mul) => {
			apply!(|a, b| ((f64::from_bits(a) * f64::from_bits(b)).to_bits(), false))
		}
		(ColumnType::F64, _) => numeric!(f64::from_bits),
		(ColumnType::Bool, BitAnd) => apply!(|a, b| (a & b, false)),
		(ColumnType::Bool, BitOr) => apply!(|a, b| (a | b, false)),
		(ColumnType::Bool, BitXor) => apply!(|a, b| (a ^ b, false)),
		(ColumnType::Bool, Eq) => cmp!(|v| v, ==),
		(ColumnType::Bool, Ne) => cmp!(|v| v, !=),
		_ => unreachable!("verified primitive"),
	}
}

#[cfg(test)]
mod packed_tests {
	use super::*;
	use crate::experimental_core::column::{BitSlice, ColumnChunk};

	#[test]
	fn packed_selection_and_cancellation_bound() {
		let values = [0x69; 520];
		for force in [true, false] {
			let engine = crate::experimental_column_primitives::Engine::new(force);
			for offset in 0..16 {
				let chunks = [ColumnChunk::new(
					ColumnSlice::Bool(BitSlice::new(&values, offset, 3087).unwrap()),
					None,
				)
				.unwrap()];
				let column = Column::new(ColumnType::Bool, &chunks).unwrap();
				let mut out = [0u8; 386];
				let result = packed_not(column, &mut out, engine, &mut || false);
				assert_eq!(result.is_some(), offset.is_multiple_of(8));
				if result.is_none() {
					assert_eq!(out, [0; 386]);
				}
			}
			let chunks = [ColumnChunk::new(
				ColumnSlice::Bool(BitSlice::new(&values, 8, 3087).unwrap()),
				None,
			)
			.unwrap()];
			let column = Column::new(ColumnType::Bool, &chunks).unwrap();
			// Observe the private prefix only in this internal test: no block
			// between cancellation checks processes more than 1024 logical rows.
			for (stop, prefix) in [0, 1024, 2048, 3072, 3080, 3087].into_iter().enumerate() {
				let mut guarded = [0xa5; 388];
				guarded[1..387].fill(0);
				let mut calls = 0;
				assert_eq!(
					packed_not(column, &mut guarded[1..387], engine, &mut || {
						calls += 1;
						calls == stop + 1
					}),
					Some(Err(ColumnRefusal::Canceled))
				);
				let output = BitSlice::new(&guarded[1..387], 0, 3087).unwrap();
				for row in 0..3087 {
					assert_eq!(
						output.bit(row),
						row < prefix && values[1 + row / 8] & (1 << (row % 8)) == 0
					);
				}
				assert_eq!(guarded[0], 0xa5);
				assert_eq!(guarded[387], 0xa5);
				assert_eq!(guarded[386] >> 7, 0);
			}
			// Reject a nonfinal partial byte before even the aligned prefix is written.
			let chunks = [8, 7, 8].map(|len| {
				ColumnChunk::new(
					ColumnSlice::Bool(BitSlice::new(&values, 0, len).unwrap()),
					None,
				)
				.unwrap()
			});
			let column = Column::new(ColumnType::Bool, &chunks).unwrap();
			let mut out = [0xa5; 3];
			assert_eq!(packed_not(column, &mut out, engine, &mut || false), None);
			assert_eq!(out, [0xa5; 3]);
		}
	}
}
