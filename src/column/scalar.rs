use super::*;
use rune::experimental_core::{execute::Executor, heap::Store, value::Scalar as Value};
use rune::runtime::TypeHash;
use rune::{Value as StockValue, Vm};
use views::{Cursor, filled};

#[allow(clippy::too_many_arguments, clippy::result_large_err)]
pub(super) fn run(
	scalar: &Scalar,
	output: ColumnType,
	inputs: &[Input<'_>],
	len: usize,
	limits: Limits,
	mut room: usize,
	options: Options,
	canceled: &mut impl FnMut() -> bool,
	observation: &mut Observation,
) -> Result<Output, Error> {
	let fail = |cause| Error {
		row: None,
		remaining_work: options.work,
		cause,
	};
	let mut values = match output {
		ColumnType::I64 => ColumnValues::I64(filled(len, 0, &mut room).map_err(fail)?),
		ColumnType::F64 => ColumnValues::F64(filled(len, 0., &mut room).map_err(fail)?),
		ColumnType::Bool => {
			ColumnValues::Bool(filled(len.div_ceil(8), 0, &mut room).map_err(fail)?)
		}
	};
	let nullable = inputs
		.iter()
		.any(|i| i.chunks.iter().any(|c| c.validity.is_some()));
	let mut validity = if nullable {
		Some(filled(len.div_ceil(8), 0, &mut room).map_err(fail)?)
	} else {
		None
	};
	let mut remaining = options.work;
	let mut cursors = [Cursor::default(); 3];
	// Bound Rune's fallible allocations for the scalar engine separately from
	// the already charged std output buffers. The process ceiling additionally
	// covers allocator overhead and trusted host allocations when enabled.
	rune::alloc::limit::with(room, || {
		let mut engine = Engine::new(scalar, limits, options.executor_limits).map_err(fail)?;
		for row in 0..len {
			if canceled() {
				observation.path = Some(Path::Canceled);
				return Err(Error {
					row: Some(row),
					remaining_work: remaining,
					cause: Cause::Canceled,
				});
			}
			let valid = inputs
				.iter()
				.enumerate()
				.all(|(i, input)| cursors[i].valid(*input));
			if valid {
				let mut args = [Value::Unit; 3];
				for (i, input) in inputs.iter().enumerate() {
					args[i] = cursors[i].value(*input);
				}
				let result =
					engine
						.call(&args[..inputs.len()], &mut remaining)
						.map_err(|cause| Error {
							row: Some(row),
							remaining_work: remaining,
							cause,
						})?;
				match (&mut values, result) {
					(ColumnValues::I64(out), Value::I64(value)) => out[row] = value,
					(ColumnValues::F64(out), Value::F64(value)) => out[row] = value,
					(ColumnValues::Bool(out), Value::Bool(value)) => {
						if value {
							out[row / 8] |= 1 << (row % 8);
						}
					}
					_ => {
						return Err(Error {
							row: Some(row),
							remaining_work: remaining,
							cause: Cause::Invalid("scalar result type"),
						});
					}
				}
				if let Some(bits) = &mut validity {
					bits[row / 8] |= 1 << (row % 8);
				}
				observation.valid_rows += 1;
			}
			// Every cursor advances even when an earlier input was null. Its
			// current chunk must first skip empties, including null-skipped inputs.
			for (i, input) in inputs.iter().enumerate() {
				cursors[i].valid(*input);
				cursors[i].advance();
			}
		}
		Ok(())
	})
	.call()?;
	Ok(Output {
		values,
		validity,
		len,
		remaining_work: remaining,
	})
}

enum Engine<'a> {
	Core {
		program: &'a Program,
		function: u32,
		charge: usize,
		executor: Executor,
		store: Store,
	},
	Stock {
		vm: Vm,
		hash: Hash,
	},
}

impl<'a> Engine<'a> {
	fn new(
		scalar: &'a Scalar,
		limits: Limits,
		executor_limits: ExecutorLimits,
	) -> Result<Self, Cause> {
		Ok(match scalar {
			Scalar::Core {
				program,
				function,
				charge,
			} => {
				let mut executor =
					Executor::with_limits(executor_limits).map_err(Cause::CoreSetup)?;
				executor.set_native_enabled(false);
				Self::Core {
					program,
					function: *function,
					charge: *charge,
					executor,
					store: Store::new(limits).map_err(Cause::CoreSetup)?,
				}
			}
			Scalar::Stock {
				runtime,
				unit,
				hash,
				..
			} => Self::Stock {
				vm: Vm::new(runtime.clone(), unit.clone()),
				hash: *hash,
			},
		})
	}

	#[allow(clippy::result_large_err)]
	fn call(&mut self, args: &[Value], remaining: &mut usize) -> Result<Value, Cause> {
		match self {
			Self::Core {
				program,
				function,
				charge,
				executor,
				store,
			} => {
				let mut roots = [store.root_unit(), store.root_unit(), store.root_unit()];
				for (root, arg) in roots.iter_mut().zip(args) {
					*root = match arg {
						Value::I64(v) => store.root_i64(*v),
						Value::F64(v) => store.root_f64(*v),
						Value::Bool(v) => store.root_bool(*v),
						_ => unreachable!(),
					};
				}
				let refs = [&roots[0], &roots[1], &roots[2]];
				let result =
					executor.invoke(program, store, *function, &refs[..args.len()], *remaining);
				for root in roots {
					store.root_release(root).expect("owned scalar root");
				}
				match result {
					Ok(root) => {
						*remaining = remaining
							.checked_sub(*charge)
							.expect("verified successful row charge");
						let scalar = store.root_scalar(&root);
						store.root_release(root).expect("owned result root");
						scalar
							.map_err(Cause::CoreSetup)?
							.ok_or(Cause::Invalid("non-scalar result"))
					}
					Err(failure) => {
						*remaining = failure.remaining_work.unwrap_or(*remaining);
						Err(Cause::Core(failure))
					}
				}
			}
			Self::Stock { vm, hash } => {
				let mut values = [
					StockValue::empty(),
					StockValue::empty(),
					StockValue::empty(),
				];
				for (value, arg) in values.iter_mut().zip(args) {
					*value = match arg {
						Value::I64(v) => (*v).into(),
						Value::F64(v) => (*v).into(),
						Value::Bool(v) => (*v).into(),
						_ => unreachable!(),
					};
				}
				let [a, b, c] = values;
				let (result, left) = rune::runtime::budget::with(*remaining, || {
					let result = match args.len() {
						1 => vm.call(*hash, (a,)),
						2 => vm.call(*hash, (a, b)),
						3 => vm.call(*hash, (a, b, c)),
						_ => unreachable!(),
					};
					(
						result,
						rune::runtime::budget::remaining().unwrap_or(usize::MAX),
					)
				})
				.call();
				*remaining = left;
				let value = result.map_err(Cause::Stock)?;
				// Integer FromValue can coerce u64, so check the physical type first.
				// The declared result type is checked separately by the caller.
				let hash = value.type_hash();
				if hash == i64::HASH {
					return rune::from_value::<i64>(value)
						.map(Value::I64)
						.map_err(|error| Cause::Stock(error.into()));
				}
				if hash == f64::HASH {
					return rune::from_value::<f64>(value)
						.map(Value::F64)
						.map_err(|error| Cause::Stock(error.into()));
				}
				if hash == bool::HASH {
					return rune::from_value::<bool>(value)
						.map(Value::Bool)
						.map_err(|error| Cause::Stock(error.into()));
				}
				Err(Cause::Invalid("non-scalar result"))
			}
		}
	}
}
