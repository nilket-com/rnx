//! Record 0205: synchronous borrowed columns through the joined adapter thread.
#![cfg(feature = "generated")]
#![deny(warnings, clippy::all)]
use polars::prelude::*;
use polars_arrow::array::{Array, BooleanArray, PrimitiveArray};
use polars_arrow::bitmap::Bitmap;
use polars_arrow::datatypes::ArrowDataType;
use rnx::column::{ColumnType, ExecutorLimits, Kernel, Limits, Mode, Options, Path};
use rnx_polars::column::{self, Error};

fn options(mode: Mode) -> Options {
	Options {
		work: 100000,
		executor_limits: ExecutorLimits::default(),
		mode,
		adapter_bytes: 0,
	}
}
fn prepare(source: &str, types: &[ColumnType], output: ColumnType) -> Kernel {
	Kernel::prepare(source, "map", types, output, Limits::default()).unwrap()
}

fn script_vm(source: &str) -> rnx::rune::Vm {
	use rnx::rune::{self, Context, Module, Source, Sources, Vm, sync::Arc};
	let mut module = Module::with_crate("polars").unwrap();
	rnx_polars::build(&mut module).unwrap();
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

fn script(source: &str) -> String {
	let mut vm = script_vm(source);
	rnx::rune::runtime::budget::with(100000, || {
		rnx::rune::from_value(vm.call(["main"], ()).unwrap()).unwrap()
	})
	.call()
}

#[test]
fn script_handle_reports_preparation_and_no_unexecuted_path() {
	let observed = script(
		r#"
		pub fn main() {
			let k = polars::column_kernel("pub fn map(x) { x + 1 }", "map", ["i64"], "i64").unwrap();
			let alias = k;
			`${alias.explain()} | ${k.last_path()}`
		}
	"#,
	);
	assert!(observed.contains("inputs=[I64]; output=I64"), "{observed}");
	assert!(
		observed.contains("primitives=portable/runtime-avx2"),
		"{observed}"
	);
	assert_eq!(
		observed.split_once(" | ").unwrap().1,
		"path=none; completed=false; rows=0; valid_rows=0; attempts=0; replays=0; refusal=None"
	);
	let refused = script(
		r#"
		pub fn main() {
			let k = polars::column_kernel("pub fn map(x) { if x < 0 { -x } else { x } }", "map", ["i64"], "i64").unwrap();
			k.explain()
		}
	"#,
	);
	assert!(
		refused.contains("scalar=stock-vm; primitives=none; refusal=Some"),
		"{refused}"
	);
}

#[test]
fn script_signature_errors_are_explicit_and_preparation_runs_no_body() {
	let result = script(
		r#"
		pub fn main() {
			let source = "pub fn map(x) { panic(\"must not execute\") }";
			let k = polars::column_kernel(source, "map", ["i64"], "i64").unwrap();
			let invalid = polars::column_kernel(source, "map", ["u64"], "i64");
			let empty = polars::column_kernel(source, "map", [], "i64");
			let arity = polars::column_kernel(source, "map", ["i64", "i64"], "i64");
			let missing = polars::column_kernel(source, "absent", ["i64"], "i64");
			match (invalid, empty, arity, missing) {
				(Err(a), Err(b), Err(_), Err(_)) => `${a} | ${b} | ${k.last_path()}`,
				_ => "wrongly accepted".to_string(),
			}
		}
	"#,
	);
	assert!(result.contains("expected i64, f64 or bool"), "{result}");
	assert!(result.contains("one to three inputs required"), "{result}");
	assert!(result.contains("path=none; completed=false"), "{result}");
}

#[test]
fn numeric_chunks_and_owners_remain_borrowed_through_join() {
	let mut kernel = prepare(
		"pub fn map(x,y) { x * 2 + y }",
		&[ColumnType::I64; 2],
		ColumnType::I64,
	);
	let mut a = Series::new("a".into(), &[Some(1i64), None, Some(3)]);
	a.append(&Series::new("a".into(), &[Some(4i64), Some(5)]))
		.unwrap();
	let b = Series::new("b".into(), &[8i64, 10, 20, 30, 40, 50, 60]).slice(1, 5);
	let pointers: Vec<_> = a
		.chunks()
		.iter()
		.map(|a| {
			a.as_any()
				.downcast_ref::<PrimitiveArray<i64>>()
				.unwrap()
				.values()
				.as_ptr()
		})
		.collect();
	let alias = a.clone();
	for mode in [Mode::Auto, Mode::Portable, Mode::Reference] {
		let result = column::apply(&mut kernel, &[&a, &b], options(mode), || false).unwrap();
		assert_eq!(
			result.series.i64().unwrap().iter().collect::<Vec<_>>(),
			[Some(12), None, Some(36), Some(48), Some(60)]
		);
		assert_eq!(a.chunks().len(), 2);
		for (array, pointer) in alias.chunks().iter().zip(&pointers) {
			assert_eq!(
				array
					.as_any()
					.downcast_ref::<PrimitiveArray<i64>>()
					.unwrap()
					.values()
					.as_ptr(),
				*pointer
			);
		}
	}
	drop(a);
	assert_eq!(alias.i64().unwrap().get(4), Some(5));
}

#[test]
fn independent_boolean_and_validity_offsets() {
	let mut kernel = prepare(
		"pub fn map(x) { !x }",
		&[ColumnType::Bool],
		ColumnType::Bool,
	);
	for offset in 0..8 {
		for validity_offset in 0..8 {
			let mut values = Bitmap::try_new(vec![0b1011_0010; 8], 64).unwrap();
			let mut validity = Bitmap::try_new(vec![0b1101_1011; 8], 64).unwrap();
			values.slice(offset, 43);
			validity.slice(validity_offset, 43);
			let expected: Vec<_> = values
				.iter()
				.zip(validity.iter())
				.map(|(value, valid)| valid.then_some(!value))
				.collect();
			let array =
				BooleanArray::try_new(ArrowDataType::Boolean, values, Some(validity)).unwrap();
			let input = Series::from_arrow("flags".into(), Box::new(array)).unwrap();
			let result =
				column::apply(&mut kernel, &[&input], options(Mode::Auto), || false).unwrap();
			assert_eq!(
				result.series.bool().unwrap().iter().collect::<Vec<_>>(),
				expected
			);
		}
	}
}

#[test]
fn validation_failure_cannot_keep_a_prior_success() {
	let mut kernel = prepare(
		"pub fn map(x) { x + 1 }",
		&[ColumnType::I64],
		ColumnType::I64,
	);
	let input = Series::new("x".into(), &[1i64, 2, 3]);
	column::apply(&mut kernel, &[&input], options(Mode::Auto), || false).unwrap();
	assert!(kernel.last_path().completed);
	let wrong = Series::new("x".into(), &[1i32, 2, 3]);
	assert!(matches!(
		column::apply(&mut kernel, &[&wrong], options(Mode::Auto), || false),
		Err(Error::Invalid(_))
	));
	assert!(!kernel.last_path().completed);
	assert!(kernel.last_path().path.is_none());
}

#[test]
fn null_payload_overflow_is_skipped_and_real_overflow_replays() {
	let mut kernel = prepare(
		"pub fn map(x) { x + 1 }",
		&[ColumnType::I64],
		ColumnType::I64,
	);
	let array = PrimitiveArray::try_new(
		ArrowDataType::Int64,
		vec![1i64, i64::MAX, 3].into(),
		Some(Bitmap::try_new(vec![5], 3).unwrap()),
	)
	.unwrap();
	let input = Series::from_arrow("x".into(), Box::new(array)).unwrap();
	let result = column::apply(&mut kernel, &[&input], options(Mode::Auto), || false).unwrap();
	assert_eq!(
		result.series.i64().unwrap().iter().collect::<Vec<_>>(),
		[Some(2), None, Some(4)]
	);
	let bad = Series::new("bad".into(), &[1i64, i64::MAX, 3]);
	let err = match column::apply(&mut kernel, &[&bad], options(Mode::Auto), || false) {
		Err(error) => error,
		Ok(_) => panic!("overflow disappeared"),
	};
	let Error::Scalar(error) = err else {
		panic!("scalar failure required")
	};
	assert_eq!(error.row, Some(1));
	assert_eq!(kernel.last_path().path, Some(Path::Replay));
	assert_eq!(kernel.last_path().replays, 1);
}

#[test]
fn explicit_worker_work_does_not_touch_the_callers_tls_scope() {
	let mut kernel = prepare(
		"pub fn map(x) { if x < 0 { -x } else { x } }",
		&[ColumnType::I64],
		ColumnType::I64,
	);
	let input = Series::new("x".into(), &[-1i64, 2, -3]);
	rnx::rune::runtime::budget::with(17, || {
		let result = column::apply(&mut kernel, &[&input], options(Mode::Auto), || false).unwrap();
		assert!(result.remaining_work < 100000);
		assert_eq!(rnx::rune::runtime::budget::remaining(), Some(17));
		assert_eq!(kernel.last_path().attempts, 0);
	})
	.call();
}

#[test]
fn empty_chunks_and_f64_values() {
	let mut kernel = prepare(
		"pub fn map(x) { x + 1.0 }",
		&[ColumnType::F64],
		ColumnType::F64,
	);
	let chunks: Vec<Box<dyn Array>> = vec![
		Box::new(PrimitiveArray::from_vec(Vec::<f64>::new())),
		Box::new(PrimitiveArray::from_vec(vec![1., -0., f64::INFINITY])),
		Box::new(PrimitiveArray::from_vec(Vec::<f64>::new())),
	];
	let input = Series::from_arrow_chunks("x".into(), chunks).unwrap();
	let result = column::apply(&mut kernel, &[&input], options(Mode::Auto), || false).unwrap();
	assert_eq!(
		result
			.series
			.f64()
			.unwrap()
			.into_no_null_iter()
			.collect::<Vec<_>>(),
		[2., 1., f64::INFINITY]
	);
}

#[test]
fn reuse_aliases_are_never_written() {
	let mut k = prepare(
		"pub fn map(x) { x + 1.0 }",
		&[ColumnType::F64],
		ColumnType::F64,
	);
	let x = Series::new("x".into(), &[1.0f64, 2.0, 3.0]);
	let y = Series::new("y".into(), &[10.0f64, 20.0, 30.0]);
	let pointer = y.f64().unwrap().cont_slice().unwrap().as_ptr();
	let out =
		column::apply_reusing(&mut k, &[&x], options(Mode::Auto), y.clone(), || false).unwrap();
	assert_ne!(
		pointer,
		out.series.f64().unwrap().cont_slice().unwrap().as_ptr()
	);
	assert_eq!(y.f64().unwrap().cont_slice().unwrap(), &[10.0, 20.0, 30.0]);
}
