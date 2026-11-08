//! ADR 0228: one process-isolated reuse/retention control (no competing tests).
#![deny(warnings, clippy::all)]
use polars::prelude::*;
use polars_arrow::array::PrimitiveArray;
use rnx::column::{ColumnType, ExecutorLimits, Kernel, Limits, Mode, Options};
use rnx_polars::column;

fn options(work: usize) -> Options {
	Options {
		work,
		executor_limits: ExecutorLimits::default(),
		mode: Mode::Auto,
		adapter_bytes: 0,
	}
}
fn floats(values: Vec<f64>) -> Series {
	Series::from_arrow("reuse".into(), Box::new(PrimitiveArray::from_vec(values))).unwrap()
}
fn bits(series: &Series) -> Vec<Option<u64>> {
	match series.dtype() {
		DataType::Float64 => series
			.f64()
			.unwrap()
			.iter()
			.map(|v| v.map(f64::to_bits))
			.collect(),
		DataType::Int64 => series
			.i64()
			.unwrap()
			.iter()
			.map(|v| v.map(|v| v as u64))
			.collect(),
		DataType::Boolean => series
			.bool()
			.unwrap()
			.iter()
			.map(|v| v.map(u64::from))
			.collect(),
		_ => panic!("unsupported output"),
	}
}

#[test]
fn reuse_matrix_failure_and_retention() {
	for (source, types, output, values) in cases() {
		let mut k = Kernel::prepare(source, "map", types, output, Limits::default()).unwrap();
		for layout in 0..4 {
			let n = 1025;
			let inputs: Vec<Series> = types
				.iter()
				.enumerate()
				.map(|(a, ty)| {
					let present = |i: usize| match layout {
						0 => true,
						1 => i < n / 2,
						2 => i.is_multiple_of(2),
						_ => (i * 73 + a * 41 + 19) % 251 < 127,
					};
					let mut s = match ty {
						ColumnType::F64 => Series::new(
							"x".into(),
							(0..n)
								.map(|i| present(i).then_some(values[a] as f64))
								.collect::<Vec<_>>(),
						),
						ColumnType::I64 => Series::new(
							"x".into(),
							(0..n)
								.map(|i| present(i).then_some(values[a]))
								.collect::<Vec<_>>(),
						),
						ColumnType::Bool => Series::new(
							"x".into(),
							(0..n)
								.map(|i| present(i).then_some(values[a] != 0))
								.collect::<Vec<_>>(),
						),
					};
					if layout != 0 {
						let tail = s.slice(37 + a as i64, n - 37 - a);
						s = s.slice(0, 37 + a);
						s.append(&tail).unwrap();
					}
					s
				})
				.collect();
			let refs: Vec<_> = inputs.iter().collect();
			let fresh = column::apply(&mut k, &refs, options(1000000), || false).unwrap();
			let expected = bits(&fresh.series);
			let work = fresh.remaining_work;
			let path = format!("{:?}", k.last_path());
			let shared = fresh.series.clone();
			let sliced = shared.slice(1, n - 1);
			for donor in [
				fresh.series,
				floats(vec![99.0; n]),
				floats(vec![99.0; n * 4]),
				floats(vec![99.0; 1]),
				shared.clone(),
				sliced,
			] {
				let reused =
					column::apply_reusing(&mut k, &refs, options(1000000), donor, || false)
						.unwrap();
				assert_eq!(bits(&reused.series), expected, "{source} layout {layout}");
				assert_eq!(reused.remaining_work, work);
				assert_eq!(format!("{:?}", k.last_path()), path);
			}
		}
	}
	let mut k = Kernel::prepare(
		"pub fn map(x) { x + 1.0 }",
		"map",
		&[ColumnType::F64],
		ColumnType::F64,
		Limits::default(),
	)
	.unwrap();
	let input = floats(vec![
		f64::from_bits(0x7ff8000000000042),
		-0.0,
		f64::INFINITY,
	]);
	let fresh = column::apply(&mut k, &[&input], options(10000), || false).unwrap();
	let reused = column::apply_reusing(
		&mut k,
		&[&input],
		options(10000),
		floats(vec![123.0; 3]),
		|| false,
	)
	.unwrap();
	assert_eq!(bits(&fresh.series), bits(&reused.series));
	let err = column::apply(&mut k, &[&input], options(0), || false)
		.err()
		.expect("expected failure");
	let reused_err = column::apply_reusing(&mut k, &[&input], options(0), reused.series, || false)
		.err()
		.expect("expected failure");
	assert_eq!(err.to_string(), reused_err.to_string());
	assert!(
		column::apply_reusing(&mut k, &[&input], options(10000), fresh.series, || true).is_err()
	);
	let empty = floats(vec![]);
	let zero = column::apply_reusing(
		&mut k,
		&[&empty],
		options(10000),
		floats(vec![1.0; 4096]),
		|| false,
	)
	.unwrap();
	assert!(zero.series.is_empty());
	// Warm metadata/thread initialization before observing process-wide allocation.
	let small = Series::new("masked".into(), (0..65).map(|i| (i % 2 == 0).then_some(1.0)).collect::<Vec<_>>());
	drop(column::apply(&mut k, &[&small], options(10000), || false).unwrap());
	let before = rnx::allocation::live().unwrap();
	let donor = Series::new("masked".into(), (0..1 << 20).map(|i| (i % 2 == 0).then_some(0.0)).collect::<Vec<_>>());
	let peak = rnx::allocation::live().unwrap();
	assert!(peak >= before + 8 * (1 << 20));
	let result = column::apply_reusing(&mut k, &[&small], options(10000), donor, || false).unwrap();
	let after = rnx::allocation::live().unwrap();
	assert!(
		after < before + 256 * 1024,
		"retained peak: before={before}, after={after}"
	);
	assert!(peak > after + 7 * (1 << 20));
	std::thread::spawn(move || drop(result)).join().unwrap();
}

#[allow(clippy::type_complexity)]
fn cases() -> Vec<(
	&'static str,
	&'static [ColumnType],
	ColumnType,
	&'static [i64],
)> {
	vec![
		("pub fn map(x,y) { x - y }", &[ColumnType::F64, ColumnType::F64], ColumnType::F64, &[1, 2]),
		("pub fn map(x,y) { x * y }", &[ColumnType::F64, ColumnType::F64], ColumnType::F64, &[1, 2]),
		(
			"pub fn map(x,y) { x & y }",
			&[ColumnType::Bool, ColumnType::Bool],
			ColumnType::Bool,
			&[1, 0],
		),
		(
			"pub fn map(x,y) { x | y }",
			&[ColumnType::Bool, ColumnType::Bool],
			ColumnType::Bool,
			&[1, 0],
		),
		(
			"pub fn map(a,b,c) { (a & b) | c }",
			&[ColumnType::Bool, ColumnType::Bool, ColumnType::Bool],
			ColumnType::Bool,
			&[1, 0, 1],
		),
		(
			"pub fn map(x,y) { x == y }",
			&[ColumnType::Bool, ColumnType::Bool],
			ColumnType::Bool,
			&[1, 0],
		),
		(
			"pub fn map(x,y) { x ^ y }",
			&[ColumnType::Bool, ColumnType::Bool],
			ColumnType::Bool,
			&[1, 0],
		),
		(
			"pub fn map(x,y) { x * 2.0 + y }",
			&[ColumnType::F64, ColumnType::F64],
			ColumnType::F64,
			&[1, 2],
		),
		(
			"pub fn map(x,y) { x * 3 + y }",
			&[ColumnType::I64, ColumnType::I64],
			ColumnType::I64,
			&[1, 2],
		),
		(
			"pub fn map(x) { x + 1 }",
			&[ColumnType::I64],
			ColumnType::I64,
			&[1],
		),
		(
			"pub fn map(x,y) { x * 2 + y }",
			&[ColumnType::I64, ColumnType::I64],
			ColumnType::I64,
			&[1, 2],
		),
		(
			"pub fn map(x) { x + 1.0 }",
			&[ColumnType::F64],
			ColumnType::F64,
			&[1],
		),
		(
			"pub fn map(x,y) { x + y }",
			&[ColumnType::F64, ColumnType::F64],
			ColumnType::F64,
			&[1, 2],
		),
		(
			"pub fn map(x) { !x }",
			&[ColumnType::Bool],
			ColumnType::Bool,
			&[1],
		),
		(
			"pub fn map(x) { x > 1.0 }",
			&[ColumnType::F64],
			ColumnType::Bool,
			&[2],
		),
		(
			"pub fn map(x) { x <= 1.0 }",
			&[ColumnType::F64],
			ColumnType::Bool,
			&[2],
		),
		(
			"pub fn map(x) { x == 2 }",
			&[ColumnType::I64],
			ColumnType::Bool,
			&[2],
		),
		(
			"pub fn map(x,y) { (x > 1.0) & (y < 2.0) }",
			&[ColumnType::F64, ColumnType::F64],
			ColumnType::Bool,
			&[2, 1],
		),
		(
			"pub fn map(x,y,z) { ((x > 1.0) & (y < 2.0)) | z }",
			&[ColumnType::F64, ColumnType::F64, ColumnType::Bool],
			ColumnType::Bool,
			&[2, 1, 1],
		),
		(
			"pub fn map(x,y) { x * 2.0 + y > 3.0 }",
			&[ColumnType::F64, ColumnType::F64],
			ColumnType::Bool,
			&[2, 1],
		),
		(
			"pub fn map(x,y) { x != y }",
			&[ColumnType::Bool, ColumnType::Bool],
			ColumnType::Bool,
			&[1, 0],
		),
	]
}
