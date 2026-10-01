//! Record 0129: a frame's numeric columns to and from rnx's neutral block
//! (`rnx::interchange::Dense`), so another adapter can take Polars data
//! without depending on this crate. The frame is borrowed, never taken.
//!
//! The conversion policy is conservative, and stated as policy:
//! - numeric columns only (integers of every width, `f32`, `f64`); a null,
//!   a boolean, a string, a date or a nested column is refused by name;
//! - an integer is admitted in the contiguous exact range only (|v| ≤ 2⁵³
//!   for `f64`, |v| ≤ 2²⁴ for `f32`), inspected as an integer before any
//!   float cast, even where a larger value happens to be representable;
//! - `f64` to `f32` is admitted for |v| ≤ `f32::MAX` and rounded to nearest;
//! - NaN and ±∞ are refused, with the column and row.
//!
//! Every limit is checked before anything proportional is allocated.
use crate::DataFrame;
use polars::prelude as p;
use rnx::interchange::{self, Data, Dense};
use rnx::rune;
use std::sync::Arc;

const EXACT_F64: u64 = 1 << 53;
const EXACT_F32: u64 = 1 << 24;

/// One column's values, row-major into `out` at stride `width`.
fn fill<T: Copy>(
	out: &mut [T],
	width: usize,
	at: usize,
	column: &p::Column,
	name: &str,
	f32_target: bool,
	conv: impl Fn(f64) -> T,
) -> Result<(), String> {
	let series = column.as_materialized_series();
	let dtype = series.dtype();
	let limit = if f32_target { EXACT_F32 } else { EXACT_F64 };
	if dtype.is_signed_integer() {
		// inspected as integers, before any float cast
		let s = series
			.cast(&p::DataType::Int64)
			.map_err(|e| format!("to_dense: {name:?}: {e}"))?;
		for (row, v) in s
			.i64()
			.map_err(|e| e.to_string())?
			.into_no_null_iter()
			.enumerate()
		{
			if v.unsigned_abs() > limit {
				return Err(format!(
					"to_dense: {v} in column {name:?} at row {row} is outside the exact integer range (|v| <= {limit})"
				));
			}
			out[row * width + at] = conv(v as f64);
		}
	} else if dtype.is_unsigned_integer() {
		let s = series
			.cast(&p::DataType::UInt64)
			.map_err(|e| format!("to_dense: {name:?}: {e}"))?;
		for (row, v) in s
			.u64()
			.map_err(|e| e.to_string())?
			.into_no_null_iter()
			.enumerate()
		{
			if v > limit {
				return Err(format!(
					"to_dense: {v} in column {name:?} at row {row} is outside the exact integer range (|v| <= {limit})"
				));
			}
			out[row * width + at] = conv(v as f64);
		}
	} else if matches!(dtype, p::DataType::Float32) {
		for (row, v) in series
			.f32()
			.map_err(|e| e.to_string())?
			.into_no_null_iter()
			.enumerate()
		{
			if !v.is_finite() {
				return Err(format!(
					"to_dense: a non-finite value in column {name:?} at row {row}"
				));
			}
			out[row * width + at] = conv(v as f64);
		}
	} else if matches!(dtype, p::DataType::Float64) {
		for (row, v) in series
			.f64()
			.map_err(|e| e.to_string())?
			.into_no_null_iter()
			.enumerate()
		{
			if !v.is_finite() {
				return Err(format!(
					"to_dense: a non-finite value in column {name:?} at row {row}"
				));
			}
			if f32_target && v.abs() > f32::MAX as f64 {
				return Err(format!(
					"to_dense: {v} in column {name:?} at row {row} is outside the f32 range"
				));
			}
			out[row * width + at] = conv(v);
		}
	} else {
		// unreachable after `preflight`, kept so `fill` never guesses
		return Err(format!("to_dense: column {name:?} is {dtype}, not numeric"));
	}
	Ok(())
}

/// Every selected column's dtype and nulls, before the row-major buffer is
/// allocated: a refusal here costs no proportional allocation.
fn preflight(cols: &[&p::Column], names: &[String]) -> Result<(), String> {
	for (column, name) in cols.iter().zip(names) {
		let dtype = column.dtype();
		if !(dtype.is_integer() || matches!(dtype, p::DataType::Float32 | p::DataType::Float64)) {
			return Err(format!("to_dense: column {name:?} is {dtype}, not numeric"));
		}
		if column.null_count() > 0 {
			let row = column
				.as_materialized_series()
				.is_null()
				.iter()
				.position(|x| x == Some(true))
				.unwrap_or(0);
			return Err(format!("to_dense: a null in column {name:?} at row {row}"));
		}
	}
	Ok(())
}

/// `df.to_dense(columns, dtype)`: the listed numeric columns, row-major.
pub(crate) fn to_dense(
	this: &DataFrame,
	columns: rune::Value,
	dtype: &str,
) -> Result<Dense, String> {
	let names = interchange::names_from(&columns, "to_dense", "columns")?;
	let f32_target = match dtype {
		"f32" => true,
		"f64" => false,
		other => {
			return Err(format!(
				"to_dense: dtype must be \"f32\" or \"f64\", found {other:?}"
			));
		}
	};
	// every limit, column and dtype before anything proportional is allocated
	let n = interchange::check_shape(this.0.height(), names.len())?;
	let cols: Vec<&p::Column> = names
		.iter()
		.map(|c| this.0.column(c).map_err(|e| format!("to_dense: {e}")))
		.collect::<Result<_, _>>()?;
	preflight(&cols, &names)?;
	let width = names.len();
	let data = if f32_target {
		let mut out = vec![0f32; n];
		for (at, (c, name)) in cols.iter().zip(&names).enumerate() {
			fill(&mut out, width, at, c, name, true, |v| v as f32)?;
		}
		Data::F32(Arc::new(out))
	} else {
		let mut out = vec![0f64; n];
		for (at, (c, name)) in cols.iter().zip(&names).enumerate() {
			fill(&mut out, width, at, c, name, false, |v| v)?;
		}
		Data::F64(Arc::new(out))
	};
	Dense::new(data, this.0.height(), width, names)
}

/// The block's columns as `Series`, copied out of the row-major data.
fn series(block: &Dense) -> Vec<p::Column> {
	use p::IntoSeries;
	let (rows, width) = (block.rows(), block.columns());
	block
		.names()
		.iter()
		.enumerate()
		.map(|(at, name)| {
			// the strided copy is moved into the Series (`from_vec`), not
			// copied again as a slice
			let s = match block.data() {
				Data::F32(v) => p::Float32Chunked::from_vec(
					name.as_str().into(),
					(0..rows).map(|r| v[r * width + at]).collect(),
				)
				.into_series(),
				Data::F64(v) => p::Float64Chunked::from_vec(
					name.as_str().into(),
					(0..rows).map(|r| v[r * width + at]).collect(),
				)
				.into_series(),
			};
			s.into()
		})
		.collect()
}

/// `DataFrame::from_dense(block)`: a new frame of the block's columns.
pub(crate) fn from_dense(block: &Dense) -> Result<DataFrame, String> {
	p::DataFrame::new(block.rows(), series(block))
		.map(DataFrame)
		.map_err(|e| format!("from_dense: {e}"))
}

/// `df.with_dense(block)`: the frame with the block's columns appended; the
/// heights must match, and a name already in the frame is Polars' error.
pub(crate) fn with_dense(this: &DataFrame, block: &Dense) -> Result<DataFrame, String> {
	if block.rows() != this.0.height() {
		return Err(format!(
			"with_dense: the block has {} rows, the frame {}",
			block.rows(),
			this.0.height()
		));
	}
	this.0
		.hstack(&series(block))
		.map(DataFrame)
		.map_err(|e| format!("with_dense: {e}"))
}

/// Record 0131: at most 65,536 rows of text, 64 MiB in all.
pub(crate) const MAX_STRING_ROWS: usize = 65_536;
pub(crate) const MAX_STRING_BYTES: usize = 64 << 20;

/// `df.strings(column)`: a `String` column as a vector of strings. The
/// dtype, nulls, row count and total bytes are checked (summing lengths, no
/// copy) before anything is copied; the frame is borrowed.
pub(crate) fn strings(this: &DataFrame, column: &str) -> Result<Vec<String>, String> {
	let op = "strings";
	let col = this.0.column(column).map_err(|e| format!("{op}: {e}"))?;
	if !matches!(col.dtype(), p::DataType::String) {
		return Err(format!(
			"{op}: column {column:?} is {}, not str",
			col.dtype()
		));
	}
	let rows = col.len();
	if rows > MAX_STRING_ROWS {
		return Err(format!("{op}: {rows} rows, at most {MAX_STRING_ROWS}"));
	}
	let ca = col.str().map_err(|e| format!("{op}: {e}"))?;
	if ca.null_count() > 0 {
		let row = ca.iter().position(|v| v.is_none()).unwrap_or(0);
		return Err(format!("{op}: a null in column {column:?} at row {row}"));
	}
	let mut total = 0usize;
	for s in ca.iter().flatten() {
		total = total
			.checked_add(s.len())
			.filter(|t| *t <= MAX_STRING_BYTES)
			.ok_or_else(|| format!("{op}: more than {MAX_STRING_BYTES} bytes of text"))?;
	}
	Ok(ca.iter().flatten().map(str::to_owned).collect())
}
