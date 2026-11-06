//! Private write-only output for specialized, single-chunk numeric execution.
use super::*;
use crate::experimental_column_primitives::{Engine, NumericOutput};

// Numeric variants always have length zero until run_flat's final conversion.
// Debug therefore cannot inspect even partially written allocation contents.
#[derive(Debug)]
pub(super) enum Output {
	Initialized(ColumnValues),
	I64(NumericOutput<i64>),
	F64(NumericOutput<f64>),
}

impl Output {
	pub(super) fn reserve(ty: ColumnType, len: usize, room: usize) -> super::super::Result<Self> {
		match ty {
			ColumnType::I64 => {
				super::super::reserved(len, room).map(|v| Self::I64(NumericOutput::new(v, len)))
			}
			ColumnType::F64 => {
				super::super::reserved(len, room).map(|v| Self::F64(NumericOutput::new(v, len)))
			}
			ColumnType::Bool => unreachable!("numeric flat admission"),
		}
	}

	pub(super) fn bytes(&self) -> usize {
		match self {
			Self::Initialized(v) => storage_bytes(v),
			Self::I64(v) => v.capacity() * 8,
			Self::F64(v) => v.capacity() * 8,
		}
	}
}

#[inline]
pub(super) fn eligible(plan: &PreparedColumn, columns: &[Column<'_>], masked: bool) -> bool {
	!masked
		&& columns.iter().all(|c| c.chunks.len() == 1)
		&& matches!(
			plan.fast,
			Some(
				Fast::I64Add(..)
					| Fast::Fused(..)
					| Fast::F64Add(..)
					| Fast::F64PairAdd(..)
					| Fast::F64PairSub(..)
					| Fast::F64PairMul(..)
			)
		)
}

#[derive(Debug)]
pub(super) struct FlatBatch<'plan, 'input> {
	pub(super) plan: &'plan PreparedColumn,
	pub(super) columns: &'input [Column<'input>],
	pub(super) options: ColumnOptions,
	pub(super) values: Output,
	pub(super) validity: Option<Vec<u8>>,
	pub(super) len: usize,
	pub(super) charge: usize,
	pub(super) valid_rows: usize,
	pub(super) scratch: Vec<u64>,
	pub(super) row_values: Vec<u64>,
}
impl FlatBatch<'_, '_> {
	pub(super) fn storage_bytes(&self) -> usize {
		self.values.bytes()
			+ self.validity.as_ref().map_or(0, Vec::capacity)
			+ self.scratch.capacity() * 8
			+ self.row_values.capacity() * 8
	}
	pub(super) fn run_flat(
		self,
		canceled: &mut impl FnMut() -> bool,
	) -> core::result::Result<ColumnResult, (ColumnRefusal, Option<Fault>)> {
		let Self {
			plan,
			columns,
			options,
			mut values,
			validity,
			len,
			charge,
			valid_rows,
			scratch: _,
			mut row_values,
		} = self;
		assert!(eligible(plan, columns, validity.is_some()));
		let engine = Engine::new(options.force_scalar);
		let mut inputs = [ColumnSlice::I64(&[]); 3];
		for (input, column) in inputs.iter_mut().zip(columns) {
			*input = column.chunks[0].values;
		}
		let mut row = 0;
		while row < len {
			if canceled() {
				return Err((ColumnRefusal::Canceled, None));
			}
			let count = (len - row).min(BLOCK);
			let end = row + count;
			let failed = match (plan.fast, &mut values) {
				(Some(Fast::I64Add(i, c)), Output::I64(out)) => {
					let ColumnSlice::I64(x) = inputs[i] else {
						unreachable!("verified type")
					};
					out.add(engine, &x[row..end], c)
				}
				(Some(Fast::Fused(i, j)), Output::I64(out)) => {
					let (ColumnSlice::I64(x), ColumnSlice::I64(y)) = (inputs[i], inputs[j]) else {
						unreachable!("verified type")
					};
					out.fused(engine, &x[row..end], &y[row..end])
				}
				(Some(Fast::F64Add(i, c)), Output::F64(out)) => {
					let ColumnSlice::F64(x) = inputs[i] else {
						unreachable!("verified type")
					};
					out.floats(engine, &x[row..end], c);
					false
				}
				(
					Some(
						fast @ (Fast::F64PairAdd(..) | Fast::F64PairSub(..) | Fast::F64PairMul(..)),
					),
					Output::F64(out),
				) => {
					let (op, i, j) = float_operation(fast);
					let (ColumnSlice::F64(x), ColumnSlice::F64(y)) = (inputs[i], inputs[j]) else {
						unreachable!("verified type")
					};
					out.pair(engine, op, &x[row..end], &y[row..end]);
					false
				}
				_ => unreachable!("private flat output admission"),
			};
			if failed {
				return Err(first_fault(
					plan,
					&inputs,
					&[row; 3],
					row,
					count,
					None,
					&mut row_values,
				));
			}
			row = end;
		}
		if canceled() {
			return Err((ColumnRefusal::Canceled, None));
		}
		// The primitive owner verifies complete coverage. Publication still occurs
		// only after arithmetic success and the final cancellation check above.
		let values = match values {
			Output::I64(out) => ColumnValues::I64(out.finish()),
			Output::F64(out) => ColumnValues::F64(out.finish()),
			Output::Initialized(_) => unreachable!("private flat output"),
		};
		Ok(ColumnResult {
			values,
			validity,
			len,
			valid_rows,
			remaining_work: options.work - charge,
			path: if len != 0 && engine.avx2() {
				Path::Avx2
			} else {
				Path::Scalar
			},
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn write_only_owner_drop_and_panic_never_publish_unwritten_storage() {
		for len in [0, 1, 3, 4, 5, BLOCK + 1] {
			let owner = Output::reserve(ColumnType::I64, len, len * 8).unwrap();
			assert!(owner.bytes() >= len * 8);
			drop(owner);
			let mut owner = Output::reserve(ColumnType::F64, len, len * 8).unwrap();
			if let Output::F64(v) = &mut owner {
				v.floats(Engine::new(true), &std::vec![2.0; len / 2], 1.0);
			}
			let _ = std::format!("{owner:?}");
			drop(owner);
			let result = std::panic::catch_unwind(|| {
				let mut owner = Output::reserve(ColumnType::I64, len, len * 8).unwrap();
				if let Output::I64(v) = &mut owner {
					if len != 0 {
						v.add(Engine::new(true), &[6], 1);
					}
				}
				panic!("partial writer");
			});
			assert!(result.is_err());
		}
	}

	#[test]
	fn reservation_preserves_requested_limit_and_actual_capacity_accounting() {
		assert!(matches!(
			Output::reserve(ColumnType::I64, 2, 15),
			Err(ColumnRefusal::Memory)
		));
		assert!(matches!(
			Output::reserve(ColumnType::F64, usize::MAX, usize::MAX),
			Err(ColumnRefusal::Memory)
		));
		let owner = Output::reserve(ColumnType::I64, 17, 1024).unwrap();
		assert!(owner.bytes() >= 17 * 8 && owner.bytes() <= 1024);
	}
}
