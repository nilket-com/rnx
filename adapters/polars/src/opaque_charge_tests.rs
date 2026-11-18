use super::*;
use rune::experimental_core::{
	domain::{CrossOutcome, DomainRoot, ProgramDomain},
	error::ErrorKind,
	heap::Limits,
};
use std::{num::NonZeroUsize, sync::Arc};

#[rune::function(free, path = DataFrame::empty, opaque_factory)]
fn zero_charge_factory() -> DataFrame {
	DataFrame(p::DataFrame::empty())
}

fn context(zero_charge: bool) -> rune::Context {
	let mut context = rune::Context::with_default_modules().unwrap();
	let mut module = rune::Module::with_crate("polars").unwrap();
	if zero_charge {
		module.opaque_type::<DataFrame>(|_| 0, display_fmt).unwrap();
		module.function_meta(zero_charge_factory).unwrap();
		module
			.opaque_result_function("read_csv", read_csv_either, read_csv_checked)
			.unwrap();
	} else {
		build(&mut module).unwrap();
	}
	context.install(module).unwrap();
	context
}

fn domain(context: &rune::Context, bytes: usize) -> ProgramDomain {
	ProgramDomain::new(
		Limits {
			bytes,
			..Limits::default()
		},
		Default::default(),
		context.execution_runtime().unwrap(),
	)
	.unwrap()
}

fn frame(domain: &mut ProgramDomain, context: &rune::Context) -> Result<DomainRoot, ErrorKind> {
	frame_from_program(
		domain,
		context,
		"pub fn main(s) { let frame = polars::DataFrame::empty(); (#{}, frame) }",
	)
}

fn frame_from_program(
	domain: &mut ProgramDomain,
	context: &rune::Context,
	program: &str,
) -> Result<DomainRoot, ErrorKind> {
	let mut sources = rune::Sources::new();
	sources
		.insert(rune::Source::memory(program).unwrap())
		.unwrap();
	let key = domain
		.compile_session(
			context,
			&mut sources,
			&mut rune::Diagnostics::new(),
			1,
			&[],
			&[],
		)
		.unwrap();
	let main = domain.entry(&key, rune::Hash::type_hash(["main"])).unwrap();
	let state = domain.record().unwrap();
	let outcome = {
		let mut invocation = domain.start(&key, main, &[&state], 10_000).unwrap();
		match invocation.resume(NonZeroUsize::new(10_000).unwrap()) {
			CrossOutcome::Complete(root) => Ok(root),
			CrossOutcome::Failed(failure) => Err(failure.failure.error.kind),
			CrossOutcome::Limited => panic!("empty frame creation exhausted the slice"),
		}
	};
	domain.release(state).unwrap();
	outcome
}

#[test]
fn loaded_frame_charges_the_data_and_releases_the_allowance() {
	let nonce = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.unwrap()
		.as_nanos();
	let path =
		std::env::temp_dir().join(format!("rnx-csv-charge-{}-{nonce}.csv", std::process::id()));
	const ROWS: usize = 1_800;
	let data = format!("a\n{}", "42\n".repeat(ROWS));
	std::fs::write(&path, data).unwrap();
	let quoted = serde_json::to_string(path.to_str().unwrap()).unwrap();
	let program = format!(
		"pub fn main(s) {{ let frame = polars::read_csv({quoted}, [(\"a\", \"i64\")])?; (#{{}}, frame) }}"
	);
	let minimum = |context: &rune::Context| {
		let (mut lower, mut upper) = (1, 64 * 1024);
		while lower < upper {
			let mid = lower + (upper - lower) / 2;
			let mut domain = domain(context, mid);
			match frame_from_program(&mut domain, context, &program) {
				Ok(root) => {
					domain.release(root).unwrap();
					upper = mid;
				}
				Err(ErrorKind::StoreCapacity) => lower = mid + 1,
				Err(error) => panic!("CSV charge control failed: {error:?}"),
			}
			domain.close().unwrap();
		}
		assert!(lower < 64 * 1024);
		lower
	};
	let reference = minimum(&context(true));
	let charge = std::mem::size_of::<DataFrame>() + ROWS * std::mem::size_of::<i64>();
	let actual = context(false);
	let required = minimum(&actual);
	// The bounded schema snapshot can set the minimum in the zero-charge
	// control. Its fixed scratch ceiling is independent of the retained frame.
	assert!(reference < charge);
	assert!((charge..=charge + reference).contains(&required));
	let mut tight = domain(&actual, required - 1);
	assert!(matches!(
		frame_from_program(&mut tight, &actual, &program),
		Err(ErrorKind::StoreCapacity)
	));
	tight.collect_programs().unwrap();
	assert_eq!(tight.live_counts(), (0, 0, 0));
	tight.close().unwrap();
	let mut exact = domain(&actual, required);
	for _ in 0..2 {
		let root = frame_from_program(&mut exact, &actual, &program).unwrap();
		assert_eq!(exact.live_counts().1, 1);
		exact.release(root).unwrap();
		exact.collect_programs().unwrap();
		assert_eq!(exact.live_counts(), (0, 0, 0));
	}
	exact.close().unwrap();
	std::fs::remove_file(path).unwrap();
}

#[test]
fn checked_csv_parser_bounded_input_edges() {
	let nonce = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.unwrap()
		.as_nanos();
	let path =
		std::env::temp_dir().join(format!("rnx-csv-edges-{}-{nonce}.csv", std::process::id()));
	let path_str = path.to_str().unwrap();
	let int = p::DataType::Int64;
	let string = p::DataType::String;
	let numeric = Arc::new(p::Schema::from_iter([(
		p::PlSmallStr::from_str("n"),
		int.clone(),
	)]));
	let mixed = Arc::new(p::Schema::from_iter([
		(p::PlSmallStr::from_str("s"), string),
		(p::PlSmallStr::from_str("n"), int.clone()),
	]));
	let columns: Vec<_> = (0..16).map(|i| format!("c{i}")).collect();
	let wide = Arc::new(p::Schema::from_iter(
		columns
			.iter()
			.map(|name| (p::PlSmallStr::from_str(name), int.clone())),
	));

	// Warm the shared parser pool before the focused allocation observation.
	std::fs::write(&path, "n\n42\n").unwrap();
	assert_eq!(
		files::csv_session(path_str, numeric.clone())
			.unwrap()
			.unwrap()
			.height(),
		1
	);
	let wide_csv = format!(
		"{}\n{}",
		columns.join(","),
		"1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1\n".repeat(2_000)
	);
	let cases = [
		("wide", wide, wide_csv, Some(2_000)),
		(
			"quoted",
			mixed.clone(),
			format!("s,n\n\"{}\nend\",1\n", "x".repeat(60_000)),
			Some(1),
		),
		("ragged", mixed, "s,n\na,1,extra\n".into(), None),
		(
			"late-error",
			numeric,
			format!("n\n{}bad\n", "42\n".repeat(2_046)),
			None,
		),
	];
	for (label, schema, csv, height) in cases {
		std::fs::write(&path, csv).unwrap();
		let live = rnx::allocation::live().unwrap();
		rnx::allocation::reset_peak();
		let parsed = files::csv_session(path_str, schema).unwrap();
		let extra = rnx::allocation::peak().saturating_sub(live);
		eprintln!("checked CSV {label}: peak tracked allocation above baseline {extra} bytes");
		match height {
			Some(height) => assert_eq!(parsed.unwrap().height(), height),
			None => assert!(parsed.is_err(), "{label} parsed successfully"),
		}
	}
	std::fs::remove_file(path).unwrap();
}

#[test]
fn real_factory_charges_its_wrapper_and_reuses_the_allowance_after_release() {
	let reference = context(true);
	// The reference uses the same wrapper, call and envelope, with only the
	// descriptor's logical charge zeroed. The minimum includes Store overhead.
	let (mut lower, mut upper) = (1, 4096);
	while lower < upper {
		let mid = lower + (upper - lower) / 2;
		let mut domain = domain(&reference, mid);
		match frame(&mut domain, &reference) {
			Ok(root) => {
				domain.release(root).unwrap();
				upper = mid;
			}
			Err(ErrorKind::StoreCapacity) => lower = mid + 1,
			Err(error) => panic!("zero-charge control failed: {error:?}"),
		}
		domain.close().unwrap();
	}
	assert!(lower < 4096);
	let empty = DataFrame(p::DataFrame::empty());
	assert_eq!(empty.0.estimated_size(), 0);
	let bytes = lower + std::mem::size_of::<DataFrame>();

	let actual = context(false);
	let mut too_small = domain(&actual, bytes - 1);
	assert!(matches!(
		frame(&mut too_small, &actual),
		Err(ErrorKind::StoreCapacity)
	));
	too_small.collect_programs().unwrap();
	assert_eq!(too_small.live_counts(), (0, 0, 0));
	too_small.close().unwrap();

	let mut exact = domain(&actual, bytes);
	for _ in 0..2 {
		let root = frame(&mut exact, &actual).unwrap();
		assert_eq!(exact.live_counts().1, 1);
		exact.release(root).unwrap();
		exact.collect_programs().unwrap();
		assert_eq!(exact.live_counts(), (0, 0, 0));
	}
	exact.close().unwrap();
}
