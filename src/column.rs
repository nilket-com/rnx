//! Prepared scalar functions over immutable columns, independent of VM dispatch.
//!
//! Inputs stay borrowed until this synchronous call returns. Successful output
//! owns its buffers. A refused optional attempt is discarded before exactly one
//! scalar replay; only the returned remaining work is committed by a host.
#![deny(warnings, clippy::all)]
// Preserve Rune's complete Failure by value, including allocation failures:
// boxing it would introduce another allocation on the error path.
#![allow(clippy::result_large_err)]

use rune::experimental_core::{column as core, execute::Failure, frontend::Program};
use rune::runtime::{RuntimeContext, VmError};
use rune::sync::Arc;
use rune::{Hash, Sources, Unit};

pub use core::{BitSlice, ColumnRefusal, ColumnSlice, ColumnType, ColumnValues};
pub use rune::experimental_core::{execute::ExecutorLimits, heap::Limits};

mod prepare;
mod scalar;
mod views;
pub use views::{Chunk, Input};

/// Select an implementation without changing the function's semantics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
	/// Runtime-selected precompiled primitives, with one scalar replay on refusal.
	#[default]
	Auto,
	/// Portable primitives, including on hosts with AVX2.
	Portable,
	/// The original scalar interpreter; no optional attempt.
	Reference,
}

/// Explicit caller-owned work; no implicit thread-local allowance in this API.
#[derive(Clone, Copy, Debug)]
pub struct Options {
	pub work: usize,
	pub executor_limits: ExecutorLimits,
	pub mode: Mode,
	/// Live metadata owned by an adapter outside this module, including capacity.
	pub adapter_bytes: usize,
}

/// Observed execution, not an inference from CPU capabilities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Path {
	ScalarPrimitives,
	Avx2,
	ScalarFallback,
	Replay,
	Canceled,
}

/// Reset before validation on every invocation. Failed calls never retain a
/// previous success. Counts concern rows actually evaluated, excluding nulls.
#[derive(Clone, Debug, Default)]
pub struct Observation {
	pub path: Option<Path>,
	pub rows: usize,
	pub valid_rows: usize,
	pub attempts: usize,
	pub replays: usize,
	pub refusal: Option<ColumnRefusal>,
	pub completed: bool,
}

/// Fully initialized, unpublished output. The adapter transfers these buffers.
#[derive(Debug)]
pub struct Output {
	pub values: ColumnValues,
	pub validity: Option<Vec<u8>>,
	pub len: usize,
	pub remaining_work: usize,
}

/// Original structured causes survive the scalar invocation and Store teardown.
#[derive(Debug)]
pub enum Cause {
	Invalid(&'static str),
	Memory,
	Canceled,
	Compile(String),
	Core(Failure),
	CoreSetup(rune::experimental_core::error::Error),
	Stock(VmError),
}

#[derive(Debug)]
pub struct Error {
	pub row: Option<usize>,
	pub remaining_work: usize,
	pub cause: Cause,
}

impl std::fmt::Display for Error {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "column")?;
		if let Some(row) = self.row {
			write!(f, " row {row}")?;
		}
		write!(f, ": {:?}", self.cause)
	}
}
impl std::error::Error for Error {}

enum Scalar {
	Core {
		program: Program,
		function: u32,
		charge: usize,
	},
	Stock {
		runtime: Arc<RuntimeContext>,
		unit: Arc<Unit>,
		sources: Sources,
		hash: Hash,
	},
}

/// A handle owns immutable compiled code and no input or output buffers.
pub struct Kernel {
	inputs: [ColumnType; 3],
	arity: usize,
	output: ColumnType,
	scalar: Scalar,
	prepared: Option<core::PreparedColumn>,
	refusal: Option<ColumnRefusal>,
	limits: Limits,
	retained_bytes: usize,
	last: Observation,
}

impl Kernel {
	pub fn input_types(&self) -> &[ColumnType] {
		&self.inputs[..self.arity]
	}

	pub fn output_type(&self) -> ColumnType {
		self.output
	}
	pub fn retained_bytes(&self) -> usize {
		self.retained_bytes
	}
	pub fn limits(&self) -> Limits {
		self.limits
	}
	pub fn last_path(&self) -> &Observation {
		&self.last
	}
	pub fn reset_observation(&mut self) {
		self.last = Observation::default();
	}
	/// An adapter failed to construct its output after primitive completion.
	/// The caller must discard the output and must not commit its work debit.
	pub fn discard_output(&mut self) {
		self.last.completed = false;
	}
	pub fn row_charge(&self) -> Option<usize> {
		match &self.scalar {
			Scalar::Core { charge, .. } => Some(*charge),
			Scalar::Stock { .. } => None,
		}
	}

	/// Formatting does not discard the typed failure held by the caller.
	pub fn format_error(&self, error: &Error) -> String {
		let message = match (&self.scalar, &error.cause) {
			(Scalar::Core { program, .. }, Cause::Core(failure)) => {
				format!(
					"{}; location {:?}; calls {:?}",
					failure.message(program),
					failure.error.location,
					failure.call_chain
				)
			}
			(Scalar::Stock { sources, .. }, Cause::Stock(error)) => {
				error.diagnostic(sources).to_string()
			}
			_ => format!("{:?}", error.cause),
		};
		format!("column row {:?}: {message}", error.row)
	}

	pub fn explain(&self) -> String {
		format!(
			"inputs={:?}; output={:?}; null-in=null-out; scalar={}; primitives={}; refusal={:?}; row_charge={:?}; retained_bytes={}",
			self.input_types(),
			self.output,
			if self.prepared.is_some() {
				"execution-engine"
			} else {
				"stock-vm"
			},
			if self.prepared.is_some() {
				"portable/runtime-avx2"
			} else {
				"none"
			},
			self.refusal,
			self.row_charge(),
			self.retained_bytes
		)
	}

	/// Execute synchronously with at most one optional attempt and one replay.
	/// The caller commits work only after it has constructed its final output.
	#[allow(clippy::result_large_err)]
	pub fn map(
		&mut self,
		inputs: &[Input<'_>],
		options: Options,
		mut canceled: impl FnMut() -> bool,
	) -> Result<Output, Error> {
		self.reset_observation();
		let fail = |cause| Error {
			row: None,
			remaining_work: options.work,
			cause,
		};
		let len = views::validate(inputs, self.input_types()).map_err(fail)?;
		self.last.rows = len;
		let room = self
			.limits
			.bytes
			.checked_sub(self.retained_bytes)
			.and_then(|n| n.checked_sub(options.adapter_bytes))
			.ok_or_else(|| fail(Cause::Memory))?;
		if canceled() {
			self.last.path = Some(Path::Canceled);
			return Err(fail(Cause::Canceled));
		}
		if options.mode != Mode::Reference && self.prepared.is_some() {
			self.last.attempts = 1;
			let attempt = self.attempt(inputs, options, room, &mut canceled);
			match attempt {
				core::ColumnOutcome::Complete(result) => {
					self.last.valid_rows = result.valid_rows;
					self.last.path = Some(match result.path {
						core::Path::Scalar => Path::ScalarPrimitives,
						core::Path::Avx2 => Path::Avx2,
					});
					self.last.completed = true;
					return Ok(Output {
						values: result.values,
						validity: result.validity,
						len,
						remaining_work: result.remaining_work,
					});
				}
				core::ColumnOutcome::Replay {
					reason: ColumnRefusal::Canceled,
					..
				} => {
					self.last.path = Some(Path::Canceled);
					return Err(fail(Cause::Canceled));
				}
				core::ColumnOutcome::Replay { reason, .. } => {
					self.last.refusal = Some(reason);
					self.last.replays = 1;
					self.last.path = Some(Path::Replay);
				}
			}
		} else {
			self.last.path = Some(Path::ScalarFallback);
			self.last.refusal = self.refusal;
		}
		let result = scalar::run(
			&self.scalar,
			self.output,
			inputs,
			len,
			self.limits,
			room,
			options,
			&mut canceled,
			&mut self.last,
		);
		self.last.completed = result.is_ok();
		result
	}

	fn attempt(
		&self,
		inputs: &[Input<'_>],
		options: Options,
		room: usize,
		canceled: &mut impl FnMut() -> bool,
	) -> core::ColumnOutcome {
		let run = || -> Result<core::ColumnOutcome, ColumnRefusal> {
			let (descriptors, bytes) = views::descriptors(inputs, room)?;
			let empty = core::Column::new(ColumnType::I64, &[])?;
			let mut columns = [empty; 3];
			for (index, input) in inputs.iter().enumerate() {
				columns[index] = core::Column::new(input.ty, &descriptors[index])?;
			}
			let cached = self.prepared.as_ref().expect("admitted handle");
			// CK1's allowance is immutable. For bounded host memory, prepare a
			// temporary plan with all live metadata deducted, including the cached
			// plan. The unbounded ordinary path reuses the original plan directly.
			let bounded;
			let prepared = if self.limits.bytes != usize::MAX || options.adapter_bytes != 0 {
				let Scalar::Core {
					program, function, ..
				} = &self.scalar
				else {
					unreachable!()
				};
				bounded = core::PreparedColumn::prepare(
					program,
					*function,
					self.input_types(),
					self.output,
					Limits {
						bytes: room.checked_sub(bytes).ok_or(ColumnRefusal::Memory)?,
						..self.limits
					},
				)?;
				&bounded
			} else {
				cached
			};
			Ok(prepared.execute(
				&columns[..self.arity],
				core::ColumnOptions {
					work: options.work,
					executor_limits: options.executor_limits,
					force_scalar: options.mode == Mode::Portable,
				},
				canceled,
			))
		};
		match run() {
			Ok(outcome) => outcome,
			Err(reason) => core::ColumnOutcome::Replay {
				reason,
				fault: None,
			},
		}
	}
}

#[cfg(all(test, not(feature = "cross-verification")))]
mod tests;

// Independent test oracle, not a second production scan.
#[cfg(all(test, not(feature = "cross-verification")))]
fn validity_count(bits: &[u8], len: usize) -> usize {
	let whole_bytes = len / 8;
	let (words, tail) = bits[..whole_bytes].as_chunks::<8>();
	let full: usize = words
		.iter()
		.map(|word| u64::from_le_bytes(*word).count_ones() as usize)
		.sum();
	let partial = if len.is_multiple_of(8) {
		0
	} else {
		(bits[whole_bytes] & ((1 << (len % 8)) - 1)).count_ones() as usize
	};
	full + partial + tail.iter().map(|b| b.count_ones() as usize).sum::<usize>()
}
