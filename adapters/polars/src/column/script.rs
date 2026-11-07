//! Handwritten script surface over the borrowed-column adapter.
use crate::generated::types::W_polars_core__series__Series as Series;
use rnx::column::{
	Cause, ColumnRefusal, ColumnType, ExecutorLimits, Kernel, Limits, Mode, Options, Path,
};
use rnx::rune::{self, Module, Value};
use rune::runtime::{VmError, budget};

#[derive(rune::Any)]
#[rune(item = ::polars)]
struct ColumnKernel(Kernel);

fn physical_type(name: &str) -> Result<ColumnType, String> {
	match name {
		"i64" => Ok(ColumnType::I64),
		"f64" => Ok(ColumnType::F64),
		"bool" => Ok(ColumnType::Bool),
		_ => Err(format!(
			"column signature: expected i64, f64 or bool, found {name:?}"
		)),
	}
}

fn column_kernel(
	source: &str,
	entry: &str,
	inputs: Value,
	output: &str,
) -> Result<ColumnKernel, String> {
	let inputs = inputs
		.borrow_ref::<rune::runtime::Vec>()
		.map_err(|_| "column signature: expected a vector of type names")?;
	if inputs.is_empty() || inputs.len() > 3 {
		return Err("column signature: one to three inputs required".into());
	}
	let mut types = [ColumnType::I64; 3];
	for (slot, name) in types.iter_mut().zip(inputs.iter()) {
		let name = name
			.borrow_string_ref()
			.map_err(|_| "column signature: expected string type names")?;
		*slot = physical_type(&name)?;
	}
	let types = &types[..inputs.len()];
	let output = physical_type(output)?;
	let defaults = Limits::default();
	let limits = Limits {
		bytes: defaults.bytes.min(rune::alloc::limit::get()),
		..defaults
	};
	crate::engine::run("column_kernel", || {
		Kernel::prepare(source, entry, types, output, limits)
	})
	.map_err(|error| error.to_string())?
	.map(ColumnKernel)
	.map_err(|error| error.to_string())
}

#[rune::function(instance)]
fn explain(kernel: &ColumnKernel) -> String {
	kernel.0.explain()
}

// Both script entry points use one cancellation type, so adding an entry does
// not instantiate a second copy of the generic column driver stack.
fn never_canceled() -> bool {
	false
}

fn map(kernel: &mut ColumnKernel, inputs: Value) -> Result<Result<Series, String>, VmError> {
	kernel.0.reset_observation();
	// An acquire guard also makes remaining() None. Never treat that as an
	// unlimited script budget; only the explicit Rust host API permits unlimited.
	let Some(work) = budget::remaining() else {
		return Ok(Err(
			"column kernel map requires a finite script budget; none is installed".into(),
		));
	};
	let result = (|| {
		let inputs = inputs
			.borrow_ref::<rune::runtime::Vec>()
			.map_err(|_| super::Error::Invalid("expected a vector of Series"))?;
		if inputs.len() != kernel.0.input_types().len() {
			return Err(super::Error::Invalid("input arity"));
		}
		// Stack-owned leases retain every original wrapper across the joined
		// worker. No shallow clone, rechunk, or full-column copy is required.
		let mut guards = [None, None, None];
		for (slot, value) in guards.iter_mut().zip(inputs.iter()) {
			*slot = Some(
				value
					.borrow_ref::<Series>()
					.map_err(|_| super::Error::Invalid("expected Series inputs"))?,
			);
		}
		let first = &guards[0].as_ref().expect("validated arity").0;
		let mut series = [first; 3];
		for (slot, guard) in series.iter_mut().zip(guards.iter()).take(inputs.len()) {
			*slot = &guard.as_ref().expect("validated lease").0;
		}
		super::apply(
			&mut kernel.0,
			&series[..inputs.len()],
			Options {
				work,
				executor_limits: ExecutorLimits::default(),
				mode: Mode::Auto,
				adapter_bytes: 0,
			},
			never_canceled,
		)
	})();
	// The worker owns a scoped copy of work. Commit only its final result or
	// scalar prefix, once, after all worker scopes and output construction end.
	let left = match &result {
		Ok(output) => output.remaining_work,
		Err(super::Error::Scalar(error)) => error.remaining_work,
		Err(_) => work,
	};
	let Some(spent) = work.checked_sub(left) else {
		kernel.0.discard_output();
		return Err(VmError::budget_exhausted());
	};
	if budget::remaining() != Some(work) || budget::consume(spent).is_err() {
		kernel.0.discard_output();
		return Err(VmError::budget_exhausted());
	}
	match result {
		Ok(output) => Ok(Ok(Series(output.series))),
		Err(super::Error::Scalar(error)) => {
			let exhausted = match &error.cause {
				Cause::Core(failure) => {
					failure.error.kind == rune::experimental_core::error::ErrorKind::FuelExhausted
				}
				Cause::Stock(error) => error.is_budget_exhausted(),
				_ => false,
			};
			if exhausted {
				Err(VmError::budget_exhausted())
			} else {
				Ok(Err(kernel.0.format_error(&error)))
			}
		}
		Err(error) => Ok(Err(error.to_string())),
	}
}

fn map_reusing(
	kernel: &mut ColumnKernel,
	inputs: Value,
	previous: Series,
) -> Result<Result<Series, String>, VmError> {
	kernel.0.reset_observation();
	// An acquire guard also makes remaining() None. Never treat that as an
	// unlimited script budget; only the explicit Rust host API permits unlimited.
	let Some(work) = budget::remaining() else {
		return Ok(Err(
			"column kernel map requires a finite script budget; none is installed".into(),
		));
	};
	let result = (|| {
		let inputs = inputs
			.borrow_ref::<rune::runtime::Vec>()
			.map_err(|_| super::Error::Invalid("expected a vector of Series"))?;
		if inputs.len() != kernel.0.input_types().len() {
			return Err(super::Error::Invalid("input arity"));
		}
		// Stack-owned leases retain every original wrapper across the joined
		// worker. No shallow clone, rechunk, or full-column copy is required.
		let mut guards = [None, None, None];
		for (slot, value) in guards.iter_mut().zip(inputs.iter()) {
			*slot = Some(
				value
					.borrow_ref::<Series>()
					.map_err(|_| super::Error::Invalid("expected Series inputs"))?,
			);
		}
		let first = &guards[0].as_ref().expect("validated arity").0;
		let mut series = [first; 3];
		for (slot, guard) in series.iter_mut().zip(guards.iter()).take(inputs.len()) {
			*slot = &guard.as_ref().expect("validated lease").0;
		}
		super::apply_reusing(
			&mut kernel.0,
			&series[..inputs.len()],
			Options {
				work,
				executor_limits: ExecutorLimits::default(),
				mode: Mode::Auto,
				adapter_bytes: 0,
			},
			previous.0,
			never_canceled,
		)
	})();
	// The worker owns a scoped copy of work. Commit only its final result or
	// scalar prefix, once, after all worker scopes and output construction end.
	let left = match &result {
		Ok(output) => output.remaining_work,
		Err(super::Error::Scalar(error)) => error.remaining_work,
		Err(_) => work,
	};
	let Some(spent) = work.checked_sub(left) else {
		kernel.0.discard_output();
		return Err(VmError::budget_exhausted());
	};
	if budget::remaining() != Some(work) || budget::consume(spent).is_err() {
		kernel.0.discard_output();
		return Err(VmError::budget_exhausted());
	}
	match result {
		Ok(output) => Ok(Ok(Series(output.series))),
		Err(super::Error::Scalar(error)) => {
			let exhausted = match &error.cause {
				Cause::Core(failure) => {
					failure.error.kind == rune::experimental_core::error::ErrorKind::FuelExhausted
				}
				Cause::Stock(error) => error.is_budget_exhausted(),
				_ => false,
			};
			if exhausted {
				Err(VmError::budget_exhausted())
			} else {
				Ok(Err(kernel.0.format_error(&error)))
			}
		}
		Err(error) => Ok(Err(error.to_string())),
	}
}

#[rune::function(instance)]
fn last_path(kernel: &ColumnKernel) -> String {
	let observed = kernel.0.last_path();
	let path = match observed.path {
		None => "none",
		Some(Path::ScalarPrimitives) => "scalar-primitives",
		Some(Path::Avx2) => "avx2",
		Some(Path::ScalarFallback) => "scalar-fallback",
		Some(Path::Replay) => "replay",
		Some(Path::Canceled) => "canceled",
	};
	// These spellings are the documented wire format, independent of Debug.
	let refusal = match observed.refusal {
		None => "None",
		Some(ColumnRefusal::Unsupported) => "Some(Unsupported)",
		Some(ColumnRefusal::EntryAuthority) => "Some(EntryAuthority)",
		Some(ColumnRefusal::Type) => "Some(Type)",
		Some(ColumnRefusal::Bounds) => "Some(Bounds)",
		Some(ColumnRefusal::Memory) => "Some(Memory)",
		Some(ColumnRefusal::Fuel) => "Some(Fuel)",
		Some(ColumnRefusal::Canceled) => "Some(Canceled)",
		Some(ColumnRefusal::Arithmetic) => "Some(Arithmetic)",
	};
	format!(
		"path={path}; completed={}; rows={}; valid_rows={}; attempts={}; replays={}; refusal={refusal}",
		observed.completed, observed.rows, observed.valid_rows, observed.attempts, observed.replays
	)
}

pub(crate) fn install(module: &mut Module) -> Result<(), String> {
	module.ty::<ColumnKernel>().map_err(crate::err)?;
	module
		.function("column_kernel", column_kernel)
		.build()
		.map_err(crate::err)?;
	module.function_meta(explain).map_err(crate::err)?;
	module
		.function("map", map)
		.build_associated::<ColumnKernel>()
		.map_err(crate::err)?;
	module
		.function("map_reusing", map_reusing)
		.build_associated::<ColumnKernel>()
		.map_err(crate::err)?;
	module.function_meta(last_path).map_err(crate::err)?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use polars::prelude::{NamedFrom, Series as PolarsSeries};

	fn handle(source: &str) -> ColumnKernel {
		ColumnKernel(
			Kernel::prepare(
				source,
				"map",
				&[ColumnType::I64],
				ColumnType::I64,
				Limits::default(),
			)
			.unwrap(),
		)
	}
	fn args(series: &PolarsSeries) -> Value {
		rune::to_value(vec![Series(series.clone())]).unwrap()
	}

	#[test]
	fn finite_work_is_committed_once_and_scopes_restore() {
		let mut k = handle("pub fn map(x) { x + 1 }");
		let input = PolarsSeries::new("x".into(), [Some(1i64), None, Some(3)]);
		let work = k.0.row_charge().unwrap() * 2;
		budget::with(17, || {
			budget::with(work, || {
				let output = map(&mut k, args(&input)).unwrap().unwrap();
				assert_eq!(
					output.0.i64().unwrap().iter().collect::<Vec<_>>(),
					[Some(2), None, Some(4)]
				);
				assert_eq!(budget::remaining(), Some(0));
				assert_eq!((k.0.last_path().attempts, k.0.last_path().replays), (1, 0));
			})
			.call();
			assert_eq!(budget::remaining(), Some(17));
		})
		.call();
	}

	#[test]
	fn every_fuel_stop_matches_scalar_remaining_without_double_debit() {
		let source = "pub fn map(x) { x * 2 + 1 }";
		let mut k = handle(source);
		let mut reference = handle(source);
		let input = PolarsSeries::new("x".into(), [1i64, 2, 3]);
		let total = k.0.row_charge().unwrap() * input.len();
		for work in 0..total {
			let error = match crate::column::apply(
				&mut reference.0,
				&[&input],
				Options {
					work,
					executor_limits: ExecutorLimits::default(),
					mode: Mode::Reference,
					adapter_bytes: 0,
				},
				|| false,
			) {
				Err(super::super::Error::Scalar(error)) => error,
				_ => panic!("reference must exhaust"),
			};
			budget::with(work, || {
				let result = map(&mut k, args(&input));
				assert!(matches!(result, Err(ref e) if e.is_budget_exhausted()));
				assert_eq!(budget::remaining(), Some(error.remaining_work));
				assert!(!k.0.last_path().completed);
				assert_eq!(k.0.last_path().replays, 1);
			})
			.call();
		}
	}

	#[test]
	fn stock_fallback_debits_only_its_actual_vm_work() {
		let source = "pub fn map(x) { if x < 0 { -x } else { x } }";
		let mut k = handle(source);
		let mut reference = handle(source);
		let input = PolarsSeries::new("x".into(), [-2i64, 3]);
		let options = Options {
			work: 1000,
			executor_limits: ExecutorLimits::default(),
			mode: Mode::Reference,
			adapter_bytes: 0,
		};
		let expected =
			crate::column::apply(&mut reference.0, &[&input], options, || false).unwrap();
		budget::with(1000, || {
			let output = map(&mut k, args(&input)).unwrap().unwrap();
			assert_eq!(
				output.0.i64().unwrap().iter().collect::<Vec<_>>(),
				[Some(2), Some(3)]
			);
			assert_eq!(budget::remaining(), Some(expected.remaining_work));
			assert_eq!((k.0.last_path().attempts, k.0.last_path().replays), (0, 0));
		})
		.call();
	}

	#[test]
	fn acquired_or_unlimited_budget_cannot_start_script_work() {
		let mut k = handle("pub fn map(x) { x + 1 }");
		let input = PolarsSeries::new("x".into(), [1i64]);
		budget::with(usize::MAX, || {
			assert!(matches!(map(&mut k, args(&input)), Ok(Err(message)) if message == "column kernel map requires a finite script budget; none is installed"));
			assert_eq!(budget::remaining(), None);
			assert_eq!(k.0.last_path().attempts, 0);
		})
		.call();
		budget::with(1000, || {
			let guard = budget::acquire();
			assert!(matches!(map(&mut k, args(&input)), Ok(Err(message)) if message == "column kernel map requires a finite script budget; none is installed"));
			assert_eq!(budget::remaining(), None);
			assert_eq!(k.0.last_path().attempts, 0);
			drop(guard);
			assert_eq!(budget::remaining(), Some(1000));
		})
		.call();
	}
	fn script_vm(source: &str) -> rnx::rune::Vm {
		use rnx::rune::{self, Context, Module, Source, Sources, Vm, sync::Arc};
		let mut module = Module::with_crate("polars").unwrap();
		crate::build(&mut module).unwrap();
		let mut context = Context::with_default_modules().unwrap();
		context.install(module).unwrap();
		let mut sources = Sources::new();
		sources.insert(Source::memory(source).unwrap()).unwrap();
		let unit = rune::prepare(&mut sources)
			.with_context(&context)
			.build()
			.unwrap();
		Vm::new(
			Arc::try_new(context.runtime().unwrap()).unwrap(),
			Arc::try_new(unit).unwrap(),
		)
	}

	#[test]
	fn script_map_returns_owned_series_and_resets_path_on_bad_inputs() {
		use crate::generated::types::W_polars_core__series__Series as Wrapped;
		use rnx::rune;
		let mut vm = script_vm(
			r#"
			pub fn main(input) {
				let k = polars::column_kernel("pub fn map(x) { x + 1 }", "map", ["i64"], "i64").unwrap();
				let alias = k;
				let output = k.map([input]).unwrap();
				let observed = alias.last_path();
				match k.map([]) {
					Err(_) => (output, observed, alias.last_path()),
					Ok(_) => panic("invalid arity accepted"),
				}
			}
		"#,
		);
		let input = PolarsSeries::new("x".into(), [Some(1i64), None, Some(3)]);
		let alias = input.clone();
		let value = rune::runtime::budget::with(100000, || vm.call(["main"], (Wrapped(input),)))
			.call()
			.unwrap();
		let (output, observed, reset): (rune::Value, String, String) =
			rune::from_value(value).unwrap();
		assert_eq!(
			output
				.borrow_ref::<Wrapped>()
				.unwrap()
				.0
				.i64()
				.unwrap()
				.iter()
				.collect::<Vec<_>>(),
			[Some(2), None, Some(4)]
		);
		assert_eq!(
			alias.i64().unwrap().iter().collect::<Vec<_>>(),
			[Some(1), None, Some(3)]
		);
		assert!(
			observed.contains("completed=true; rows=3; valid_rows=2; attempts=1; replays=0"),
			"{observed}"
		);
		assert_eq!(
			reset,
			"path=none; completed=false; rows=0; valid_rows=0; attempts=0; replays=0; refusal=None"
		);
	}

	#[test]
	fn script_cannot_catch_large_column_fuel_exhaustion() {
		use crate::generated::types::W_polars_core__series__Series as Wrapped;
		use rnx::rune;
		for body in [
			"match k.map([input]) { Ok(_) => 99, Err(_) => 77 }",
			"k.map([input])?; Ok(99)",
		] {
			for scalar in [
				"pub fn map(x) { x + 1 }",
				"pub fn map(x) { if x > 0 { x } else { -x } }",
			] {
				let source = format!(
					"pub fn main(input) {{ let k = polars::column_kernel({scalar:?}, \"map\", [\"i64\"], \"i64\").unwrap(); {body} }}"
				);
				let mut vm = script_vm(&source);
				let input = PolarsSeries::new("x".into(), vec![1i64; 10000]);
				let result =
					rune::runtime::budget::with(200, || vm.call(["main"], (Wrapped(input),)))
						.call();
				let error = result.unwrap_err();
				assert!(error.is_budget_exhausted(), "{error:?}");
				assert!(!error.stacktrace().is_empty());
			}
		}
	}

	#[test]
	fn script_reuse_input_alias_fails_without_writes() {
		let y = PolarsSeries::new("y".into(), &[10.0f64, 20.0, 30.0]);
		let mut vm = script_vm(
			r#"pub fn run(y) {
		let k = polars::column_kernel("pub fn map(x) { x + 1.0 }", "map", ["f64"], "f64").unwrap();
		k.map_reusing([y], y)
	}"#,
		);
		let value = rnx::rune::to_value(crate::generated::types::W_polars_core__series__Series(
			y.clone(),
		))
		.unwrap();
		let result = rnx::rune::runtime::budget::with(100000, || vm.call(["run"], (value,))).call();
		match result {
			Err(_) => {}
			Ok(value) => {
				let result: Result<rnx::rune::Value, String> =
					rnx::rune::from_value(value).unwrap();
				assert!(result.unwrap_err().contains("expected Series inputs"));
			}
		}
		assert_eq!(y.f64().unwrap().cont_slice().unwrap(), &[10.0, 20.0, 30.0]);
	}

	#[test]
	fn script_reuse_clone_leaves_original_unchanged() {
		let mut vm = script_vm(
			r#"pub fn run(x, y) {
		let k = polars::column_kernel("pub fn map(x) { x + 1.0 }", "map", ["f64"], "f64").unwrap();
		k.map_reusing([x], y.clone()).unwrap()
	}"#,
		);
		let x = PolarsSeries::new("x".into(), &[1.0f64, 2.0, 3.0]);
		let y = PolarsSeries::new("y".into(), &[10.0f64, 20.0, 30.0]);
		let pointer = y.f64().unwrap().cont_slice().unwrap().as_ptr();
		use crate::generated::types::W_polars_core__series__Series as Wrapper;
		let result = rnx::rune::runtime::budget::with(100000, || {
			vm.call(["run"], (Wrapper(x), Wrapper(y.clone())))
		})
		.call()
		.unwrap();
		let result: Wrapper = rnx::rune::from_value(result).unwrap();
		assert_ne!(
			pointer,
			result.0.f64().unwrap().cont_slice().unwrap().as_ptr()
		);
		assert_eq!(
			result.0.f64().unwrap().cont_slice().unwrap(),
			&[2.0, 3.0, 4.0]
		);
		assert_eq!(y.f64().unwrap().cont_slice().unwrap(), &[10.0, 20.0, 30.0]);
	}
}
