//! Session conversion borrows physical arrays or the single scalar value. Neither
//! schema discovery nor scalar expansion may populate a cache on the retained frame.
use super::*;
use polars_arrow::array::PrimitiveArray;
use rune::host::{ColumnSelection, FloatDtype, OpaqueProducerLimit as Limit, SelectionError};
use std::fmt::{self, Write};

type Outcome<T> = Result<Result<T, String>, Limit>;

struct ByteCount(usize);

impl Write for ByteCount {
	fn write_str(&mut self, text: &str) -> fmt::Result {
		self.0 = self.0.checked_add(text.len()).ok_or(fmt::Error)?;
		Ok(())
	}
}

// Account for the ordinary String, its native encoding and the Store copy.
fn message<T>(args: fmt::Arguments<'_>, credit: usize) -> Outcome<T> {
	let mut bytes = ByteCount(0);
	bytes.write_fmt(args).map_err(|_| Limit)?;
	if bytes.0.checked_mul(3).ok_or(Limit)? > credit {
		return Err(Limit);
	}
	let mut text = String::new();
	text.try_reserve_exact(bytes.0).map_err(|_| Limit)?;
	text.write_fmt(args).map_err(|_| Limit)?;
	Ok(Err(text))
}

fn invalid(error: SelectionError, credit: usize) -> Outcome<Dense> {
	match error {
		SelectionError::NotNames => message(
			format_args!("to_dense: columns must be a vector of names"),
			credit,
		),
		SelectionError::Count(0) => message(
			format_args!("to_dense: a block needs at least one column"),
			credit,
		),
		SelectionError::Count(n) => {
			message(format_args!("to_dense: {n} columns, at most 4096"), credit)
		}
		SelectionError::NameLength(n) => message(
			format_args!("to_dense: a column name is 1 to 256 bytes, found {n}"),
			credit,
		),
		SelectionError::Duplicate(name) => message(
			format_args!("to_dense: duplicate column name {name:?}"),
			credit,
		),
		SelectionError::Dtype(quoted) => message(
			format_args!("to_dense: dtype must be \"f32\" or \"f64\", found {quoted}"),
			credit,
		),
	}
}

fn null_row(col: &p::Column) -> Option<usize> {
	match col {
		p::Column::Scalar(s) => s.scalar().value().is_null().then_some(0),
		p::Column::Series(s) => {
			let mut base = 0;
			for array in s.chunks() {
				if let Some(bits) = array.validity() {
					let first = bits.leading_ones();
					if first < bits.len() {
						return Some(base + first);
					}
				}
				base += array.len();
			}
			None
		}
	}
}

#[derive(Clone, Copy)]
enum Number {
	Signed(i128),
	Unsigned(u128),
	Float(f64),
}

// Preserve the original integer for diagnostics and range comparisons.
fn number(value: &p::AnyValue<'_>) -> Option<Number> {
	use p::AnyValue::*;
	Some(match value {
		Int8(v) => Number::Signed((*v).into()),
		Int16(v) => Number::Signed((*v).into()),
		Int32(v) => Number::Signed((*v).into()),
		Int64(v) => Number::Signed((*v).into()),
		Int128(v) => Number::Signed(*v),
		UInt8(v) => Number::Unsigned((*v).into()),
		UInt16(v) => Number::Unsigned((*v).into()),
		UInt32(v) => Number::Unsigned((*v).into()),
		UInt64(v) => Number::Unsigned((*v).into()),
		UInt128(v) => Number::Unsigned(*v),
		Float32(v) => Number::Float((*v).into()),
		Float64(v) => Number::Float(*v),
		_ => return None,
	})
}

enum InvalidNumber {
	Signed(i128),
	Unsigned(u128),
	NonFinite,
	Narrow(f64),
	Storage,
}

impl Number {
	fn checked(self, dtype: FloatDtype) -> Result<f64, InvalidNumber> {
		let limit = integer_limit(dtype) as u128;
		match self {
			Self::Signed(v) if v.unsigned_abs() > limit => Err(InvalidNumber::Signed(v)),
			Self::Unsigned(v) if v > limit => Err(InvalidNumber::Unsigned(v)),
			Self::Signed(v) => Ok(v as f64),
			Self::Unsigned(v) => Ok(v as f64),
			Self::Float(v) if !v.is_finite() => Err(InvalidNumber::NonFinite),
			Self::Float(v) if dtype == FloatDtype::F32 && v.abs() > f32::MAX as f64 => {
				Err(InvalidNumber::Narrow(v))
			}
			Self::Float(v) => Ok(v),
		}
	}
}
fn integer_limit(dtype: FloatDtype) -> u64 {
	match dtype {
		FloatDtype::F32 => EXACT_F32,
		FloatDtype::F64 => EXACT_F64,
	}
}

fn fill_column<'a, T: Copy + 'a>(
	col: &p::Column,
	dtype: FloatDtype,
	mut cells: impl Iterator<Item = &'a mut T>,
	cast: impl Fn(f64) -> T,
) -> Result<(), (usize, InvalidNumber)> {
	match col {
		p::Column::Scalar(s) => {
			let v = number(s.scalar().value()).ok_or((0, InvalidNumber::Storage))?;
			let v = cast(v.checked(dtype).map_err(|e| (0, e))?);
			for cell in cells {
				*cell = v;
			}
		}
		p::Column::Series(s) => {
			let mut row = 0;
			macro_rules! primitive {
				($ty:ty, $number:ident) => {{
					for chunk in s.chunks() {
						let arr = chunk
							.as_any()
							.downcast_ref::<PrimitiveArray<$ty>>()
							.ok_or((row, InvalidNumber::Storage))?;
						for (&v, cell) in arr.values().iter().zip(&mut cells) {
							*cell = cast(
								Number::$number(v.into())
									.checked(dtype)
									.map_err(|e| (row, e))?,
							);
							row += 1;
						}
					}
				}};
			}
			use p::DataType::*;
			match s.dtype() {
				Int8 => primitive!(i8, Signed),
				Int16 => primitive!(i16, Signed),
				Int32 => primitive!(i32, Signed),
				Int64 => primitive!(i64, Signed),
				Int128 => primitive!(i128, Signed),
				UInt8 => primitive!(u8, Unsigned),
				UInt16 => primitive!(u16, Unsigned),
				UInt32 => primitive!(u32, Unsigned),
				UInt64 => primitive!(u64, Unsigned),
				UInt128 => primitive!(u128, Unsigned),
				Float32 => primitive!(f32, Float),
				Float64 => primitive!(f64, Float),
				_ => return Err((0, InvalidNumber::Storage)),
			}
		}
	}
	Ok(())
}

fn buffer<T: Default + Copy>(
	cols: &[Option<&p::Column>],
	n: usize,
	dtype: FloatDtype,
	cast: impl Fn(f64) -> T,
) -> Result<Vec<T>, (usize, usize, InvalidNumber)> {
	let mut out = vec![T::default(); n];
	for (at, col) in cols.iter().enumerate() {
		let col = col.ok_or((at, 0, InvalidNumber::Storage))?;
		fill_column(col, dtype, out[at..].iter_mut().step_by(cols.len()), &cast)
			.map_err(|(row, reason)| (at, row, reason))?;
	}
	Ok(out)
}

// In addition to the bridge's debit for owned names and its Store wrappers:
// two Arc<Vec<_>> headers, borrowed column slots and lookup indices, and a
// conservative allowance for Dense::new's name-validation hash table. At
// load 7/8, old and grown tables together need < 4 buckets per name
// (two words + a control byte per bucket), plus the small-table/control tail.
fn metadata(width: usize) -> Result<usize, Limit> {
	let word = size_of::<usize>();
	width
		.checked_mul(2 * word + 4 * (2 * word + 1))
		.and_then(|n| n.checked_add(2 * (2 * word + size_of::<Vec<String>>()) + 128))
		.ok_or(Limit)
}

pub(crate) fn convert(
	frame: &DataFrame,
	columns: ColumnSelection,
	remaining: usize,
) -> Outcome<Dense> {
	let (names, dtype) = match columns.0 {
		Ok(valid) => valid,
		Err(e) => return invalid(e, remaining),
	};
	let rows = frame.0.height();
	let width = names.len();
	let n = match interchange::check_shape(rows, width) {
		Ok(n) => n,
		Err(e) => return message(format_args!("{e}"), remaining),
	};
	let credit = remaining.checked_sub(metadata(width)?).ok_or(Limit)?;
	let mut cols = vec![None; width];
	{
		let mut indices: Vec<_> = (0..width).collect();
		indices.sort_unstable_by(|&a, &b| names[a].cmp(&names[b]));
		for col in frame.0.columns() {
			if let Ok(i) = indices.binary_search_by(|&i| names[i].as_str().cmp(col.name().as_str()))
			{
				cols[indices[i]] = Some(col);
			}
		}
	}
	for (slot, name) in cols.iter().zip(&names) {
		if slot.is_none() {
			return message(
				format_args!("to_dense: not found: {name:?} not found"),
				credit,
			);
		}
	}
	for (col, name) in cols.iter().flatten().zip(&names) {
		let ty = col.dtype();
		if !(ty.is_integer() || matches!(ty, p::DataType::Float32 | p::DataType::Float64)) {
			return message(
				format_args!("to_dense: column {name:?} is {ty}, not numeric"),
				credit,
			);
		}
		if let Some(row) = null_row(col) {
			return message(
				format_args!("to_dense: a null in column {name:?} at row {row}"),
				credit,
			);
		}
	}
	let bytes = n
		.checked_mul(if dtype == FloatDtype::F32 { 4 } else { 8 })
		.ok_or(Limit)?;
	if bytes > credit {
		return Err(Limit);
	}
	let data = match dtype {
		FloatDtype::F32 => buffer(&cols, n, dtype, |v| v as f32).map(|v| Data::F32(Arc::new(v))),
		FloatDtype::F64 => buffer(&cols, n, dtype, |v| v).map(|v| Data::F64(Arc::new(v))),
	};
	let data = match data {
		Ok(v) => v,
		Err((at, row, reason)) => {
			let name = &names[at];
			let limit = integer_limit(dtype);
			return match reason {
				InvalidNumber::Signed(v) => message(
					format_args!(
						"to_dense: {v} in column {name:?} at row {row} is outside the exact integer range (|v| <= {limit})"
					),
					credit,
				),
				InvalidNumber::Unsigned(v) => message(
					format_args!(
						"to_dense: {v} in column {name:?} at row {row} is outside the exact integer range (|v| <= {limit})"
					),
					credit,
				),
				InvalidNumber::NonFinite => message(
					format_args!("to_dense: a non-finite value in column {name:?} at row {row}"),
					credit,
				),
				InvalidNumber::Narrow(v) => message(
					format_args!(
						"to_dense: {v} in column {name:?} at row {row} is outside the f32 range"
					),
					credit,
				),
				InvalidNumber::Storage => message(
					format_args!("to_dense: invalid physical storage for column {name:?}"),
					credit,
				),
			};
		}
	};
	Ok(Dense::new(data, rows, width, names))
}

#[cfg(test)]
mod tests {
	use super::*;
	use p::NamedFrom;
	fn selection(dtype: FloatDtype) -> ColumnSelection {
		ColumnSelection(Ok((vec!["b".into(), "a".into()], dtype)))
	}

	#[test]
	fn scalar_null_and_missing_column_leave_large_frame_caches_empty() {
		let cols = (0..100)
			.map(|i| {
				p::Column::new_scalar(
					format!("n{i}").into(),
					p::Scalar::new(p::DataType::Int64, p::AnyValue::Null),
					1000,
				)
			})
			.collect();
		let frame = DataFrame(p::DataFrame::new(1000, cols).unwrap());
		assert!(frame.0.cached_schema().is_none());
		for (names, want) in [
			(
				vec!["n90".into()],
				"to_dense: a null in column \"n90\" at row 0",
			),
			(
				vec!["n90".into(), "x\n".into()],
				"to_dense: not found: \"x\\n\" not found",
			),
		] {
			let got = convert(
				&frame,
				ColumnSelection(Ok((names, FloatDtype::F32))),
				usize::MAX,
			)
			.unwrap()
			.unwrap_err();
			assert_eq!(got, want);
		}
		assert!(frame.0.cached_schema().is_none());
		for col in frame.0.columns() {
			let p::Column::Scalar(s) = col else { panic!() };
			assert!(s.lazy_as_materialized_series().is_none());
		}
	}
	#[test]
	fn isolated_conversion_peak_and_refusal() {
		const CHILD: &str = "RNX_TEST_ISOLATED_DENSE_PEAK";
		if std::env::var_os(CHILD).is_none() {
			let status = std::process::Command::new(std::env::current_exe().unwrap())
				.args([
					"--exact",
					"dense::checked::tests::isolated_conversion_peak_and_refusal",
					"--nocapture",
				])
				.env(CHILD, "1")
				.status()
				.unwrap();
			assert!(status.success());
			return;
		}
		const ROWS: usize = 1 << 18;
		let s = p::Series::new("x".into(), vec![5i32; ROWS]);
		let df = DataFrame(p::DataFrame::new(ROWS, vec![s.into()]).unwrap());
		let bound = metadata(1).unwrap() + ROWS * 8;
		for credit in [bound - 1, bound] {
			let select = ColumnSelection(Ok((vec!["x".into()], FloatDtype::F64)));
			rnx::allocation::reset_peak();
			let baseline = rnx::allocation::peak();
			let result = convert(&df, select, credit);
			let grown = rnx::allocation::peak().saturating_sub(baseline);
			eprintln!(
				"checked conversion: credit={credit}, allocation peak={grown}, bound={bound}"
			);
			if credit == bound {
				assert!(result.unwrap().is_ok());
				assert!((ROWS * 8..=bound).contains(&grown), "{grown}");
			} else {
				assert!(result.is_err());
				assert!(grown <= metadata(1).unwrap(), "{grown}");
			}
		}
	}
	#[test]
	fn bounded_buffer_reads_scalar_and_chunked_columns_without_changing_source() {
		let rows = 1 << 15;
		let a = p::Column::new_scalar(
			"a".into(),
			p::Scalar::new(p::DataType::Int32, p::AnyValue::Int32(7)),
			rows,
		);
		let mut b = p::Series::new("b".into(), vec![1.5f64; rows / 2]);
		b.append(&p::Series::new("b".into(), vec![9.5f64; rows / 2]))
			.unwrap();
		let df = DataFrame(p::DataFrame::new(rows, vec![a, b.into()]).unwrap());
		for (dtype, bytes) in [(FloatDtype::F32, 4), (FloatDtype::F64, 8)] {
			let peak = metadata(2).unwrap() + rows * 2 * bytes;
			assert!(convert(&df, selection(dtype), peak - 1).is_err());
			let block = convert(&df, selection(dtype), peak).unwrap().unwrap();
			assert_eq!(block.names(), ["b", "a"]);
			match block.data() {
				Data::F32(v) => {
					assert_eq!(&v[..4], &[1.5, 7., 1.5, 7.]);
					assert_eq!(&v[rows..rows + 2], &[9.5, 7.]);
				}
				Data::F64(v) => {
					assert_eq!(&v[..4], &[1.5, 7., 1.5, 7.]);
					assert_eq!(&v[rows..rows + 2], &[9.5, 7.]);
				}
			}
			let p::Column::Scalar(a) = &df.0.columns()[0] else {
				panic!()
			};
			assert!(a.lazy_as_materialized_series().is_none());
			assert_eq!(df.0.columns()[1].n_chunks(), 2);
		}
	}
}
