//! Borrowed Arrow columns and owned output transfer for prepared Rune functions.
//!
//! Owned descriptors, code, plans, scratch and bulk output buffers are checked
//! against the host's configured limits and allocated fallibly. The small owner
//! allocations inside Polars' Buffer/SharedStorage conversion and
//! Series::from_arrow use Polars' infallible allocator behavior. Its try_new
//! constructors validate shapes; they do not catch OOM. The process allocation
//! counter is an additional observation, not an OS memory or peak-RSS bound.
#![deny(warnings, clippy::all)]
// Keep the complete Rune Failure without allocating another error wrapper.
#![allow(clippy::result_large_err)]

use polars::prelude as p;
use polars_arrow::array::{Array, BooleanArray, PrimitiveArray};
use polars_arrow::bitmap::Bitmap;
use polars_arrow::datatypes::ArrowDataType;
use rnx::column::{
	self as host, BitSlice, Chunk, ColumnSlice, ColumnType, ColumnValues, Input, Kernel, Options,
};

mod script;
pub(crate) use script::install;

/// A fully constructed Series and its uncommitted work remainder.
pub struct Applied {
	pub series: p::Series,
	pub remaining_work: usize,
}

/// Scalar failures retain their original typed cause and trace.
#[derive(Debug)]
// Boxing can fail while reporting allocation failure; preserve the full Failure by value.
#[allow(clippy::large_enum_variant)]
pub enum Error {
	Invalid(&'static str),
	Memory,
	Engine(String),
	Arrow(p::PolarsError),
	Scalar(host::Error),
}

impl std::fmt::Display for Error {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Invalid(message) => write!(f, "column: {message}"),
			Self::Memory => write!(f, "column: memory allowance exceeded"),
			Self::Engine(message) => f.write_str(message),
			Self::Arrow(error) => error.fmt(f),
			Self::Scalar(error) => error.fmt(f),
		}
	}
}
impl std::error::Error for Error {}

/// Run on the existing joined engine thread. No input is copied or rechunked,
/// and no descriptor or input owner survives the join. The Rust host explicitly
/// supplies work; this function never reads or commits the caller's TLS budget.
pub fn apply(
	kernel: &mut Kernel,
	series: &[&p::Series],
	options: Options,
	canceled: impl FnMut() -> bool + Send,
) -> Result<Applied, Error> {
	kernel.reset_observation();
	crate::engine::run("column_kernel.map", || {
		apply_joined(kernel, series, options, canceled)
	})
	.map_err(|error| Error::Engine(error.to_string()))?
}

fn apply_joined(
	kernel: &mut Kernel,
	series: &[&p::Series],
	mut options: Options,
	canceled: impl FnMut() -> bool,
) -> Result<Applied, Error> {
	if series.len() != kernel.input_types().len() {
		return Err(Error::Invalid("input arity"));
	}
	let mut room = kernel
		.limits()
		.bytes
		.checked_sub(kernel.retained_bytes())
		.and_then(|n| n.checked_sub(options.adapter_bytes))
		.ok_or(Error::Memory)?;
	let before = room;
	let mut all = [Vec::new(), Vec::new(), Vec::new()];
	for (i, (series, ty)) in series.iter().zip(kernel.input_types()).enumerate() {
		let dtype = match ty {
			ColumnType::I64 => p::DataType::Int64,
			ColumnType::F64 => p::DataType::Float64,
			ColumnType::Bool => p::DataType::Boolean,
		};
		if series.dtype() != &dtype {
			return Err(Error::Invalid(
				"physical input dtype differs from signature",
			));
		}
		let count = series.chunks().len();
		if count
			.checked_mul(size_of::<Chunk<'_>>())
			.is_none_or(|n| n > room)
		{
			return Err(Error::Memory);
		}
		all[i].try_reserve_exact(count).map_err(|_| Error::Memory)?;
		room = room
			.checked_sub(
				all[i]
					.capacity()
					.checked_mul(size_of::<Chunk<'_>>())
					.ok_or(Error::Memory)?,
			)
			.ok_or(Error::Memory)?;
		for array in series.chunks() {
			let values = match ty {
				ColumnType::I64 => ColumnSlice::I64(
					array
						.as_any()
						.downcast_ref::<PrimitiveArray<i64>>()
						.ok_or(Error::Invalid("i64 chunk"))?
						.values()
						.as_slice(),
				),
				ColumnType::F64 => ColumnSlice::F64(
					array
						.as_any()
						.downcast_ref::<PrimitiveArray<f64>>()
						.ok_or(Error::Invalid("f64 chunk"))?
						.values()
						.as_slice(),
				),
				ColumnType::Bool => ColumnSlice::Bool(bits(
					array
						.as_any()
						.downcast_ref::<BooleanArray>()
						.ok_or(Error::Invalid("bool chunk"))?
						.values(),
				)?),
			};
			let validity = array.validity().map(bits).transpose()?;
			all[i].push(Chunk { values, validity });
		}
	}
	options.adapter_bytes = options
		.adapter_bytes
		.checked_add(before - room)
		.ok_or(Error::Memory)?;
	let mut inputs = [Input {
		ty: ColumnType::I64,
		chunks: &[],
	}; 3];
	for (i, ty) in kernel.input_types().iter().enumerate() {
		inputs[i] = Input {
			ty: *ty,
			chunks: &all[i],
		};
	}
	let output = kernel
		.map(&inputs[..series.len()], options, canceled)
		.map_err(Error::Scalar)?;
	match transfer(output) {
		Ok(output) => Ok(output),
		Err(error) => {
			kernel.discard_output();
			Err(error)
		}
	}
}

fn bits(bitmap: &Bitmap) -> Result<BitSlice<'_>, Error> {
	let (bytes, offset, len) = bitmap.as_slice();
	BitSlice::new(bytes, offset, len).map_err(|_| Error::Invalid("bitmap bounds"))
}

/// Checked constructors move the full owned buffers. Polars' shared-owner
/// metadata allocations use its allocator; try_new validates shape, not OOM.
fn transfer(output: host::Output) -> Result<Applied, Error> {
	#[cfg(test)]
	TRANSFERS.with(|count| count.set(count.get() + 1));
	let validity = output
		.validity
		.map(|bytes| Bitmap::try_new(bytes, output.len))
		.transpose()
		.map_err(Error::Arrow)?;
	let array: Box<dyn Array> = match output.values {
		ColumnValues::I64(values) => {
			if values.len() != output.len {
				return Err(Error::Invalid("output length"));
			}
			Box::new(
				PrimitiveArray::try_new(ArrowDataType::Int64, values.into(), validity)
					.map_err(Error::Arrow)?,
			)
		}
		ColumnValues::F64(values) => {
			if values.len() != output.len {
				return Err(Error::Invalid("output length"));
			}
			Box::new(
				PrimitiveArray::try_new(ArrowDataType::Float64, values.into(), validity)
					.map_err(Error::Arrow)?,
			)
		}
		ColumnValues::Bool(values) => Box::new(
			BooleanArray::try_new(
				ArrowDataType::Boolean,
				Bitmap::try_new(values, output.len).map_err(Error::Arrow)?,
				validity,
			)
			.map_err(Error::Arrow)?,
		),
	};
	let series = p::Series::from_arrow("column".into(), array).map_err(Error::Arrow)?;
	Ok(Applied {
		series,
		remaining_work: output.remaining_work,
	})
}

#[cfg(test)]
thread_local! { static TRANSFERS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }

#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn output_owners_keep_the_original_value_and_validity_pointers() {
		let values = vec![1i64, 2, 3, 4];
		let ptr = values.as_ptr();
		let validity = vec![0b1011u8];
		let mask = validity.as_ptr();
		let out = transfer(host::Output {
			values: ColumnValues::I64(values),
			validity: Some(validity),
			len: 4,
			remaining_work: 17,
		})
		.unwrap();
		let array = out.series.chunks()[0]
			.as_any()
			.downcast_ref::<PrimitiveArray<i64>>()
			.unwrap();
		assert_eq!(array.values().as_ptr(), ptr);
		assert_eq!(array.validity().unwrap().as_slice().0.as_ptr(), mask);
		assert_eq!(out.remaining_work, 17);
		let values = vec![0b0010_1101u8; 5];
		let ptr = values.as_ptr();
		let out = transfer(host::Output {
			values: ColumnValues::Bool(values),
			validity: None,
			len: 40,
			remaining_work: 19,
		})
		.unwrap();
		let array = out.series.chunks()[0]
			.as_any()
			.downcast_ref::<BooleanArray>()
			.unwrap();
		assert_eq!(array.values().as_slice().0.as_ptr(), ptr);
	}
	#[test]
	fn malformed_private_output_is_not_published() {
		assert!(
			transfer(host::Output {
				values: ColumnValues::I64(vec![1]),
				validity: None,
				len: 2,
				remaining_work: 0
			})
			.is_err()
		);
		assert!(
			transfer(host::Output {
				values: ColumnValues::Bool(vec![0]),
				validity: None,
				len: 9,
				remaining_work: 0
			})
			.is_err()
		);
		assert!(
			transfer(host::Output {
				values: ColumnValues::I64(vec![1; 9]),
				validity: Some(vec![0]),
				len: 9,
				remaining_work: 0
			})
			.is_err()
		);
	}

	#[test]
	fn output_over_configured_limit_refuses_before_polars_owner_allocation() {
		let limits = host::Limits {
			bytes: 64 * 1024 * 1024,
			..host::Limits::default()
		};
		let mut kernel = Kernel::prepare(
			"pub fn map(x) { x + 1 }",
			"map",
			&[ColumnType::I64],
			ColumnType::I64,
			limits,
		)
		.unwrap();
		// The requested values alone exceed the entire configured allowance by
		// one element. All input construction happens before the observation.
		let input = p::Series::from_arrow(
			"x".into(),
			Box::new(PrimitiveArray::from_vec(vec![1i64; limits.bytes / 8 + 1])),
		)
		.unwrap();
		crate::engine::run("column allocation control", || {
			TRANSFERS.with(|count| count.set(0));
			let options = Options {
				work: usize::MAX,
				executor_limits: host::ExecutorLimits::default(),
				mode: host::Mode::Auto,
				adapter_bytes: 0,
			};
			let result = apply_joined(&mut kernel, &[&input], options, || false);
			assert!(matches!(
				result,
				Err(Error::Scalar(host::Error {
					cause: host::Cause::Memory,
					..
				}))
			));
			assert_eq!(TRANSFERS.with(std::cell::Cell::get), 0);
			assert!(!kernel.last_path().completed);
		})
		.unwrap();
	}
}
