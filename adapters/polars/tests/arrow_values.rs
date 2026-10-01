//! Record 0119: the owned Arrow array bridge, end to end. A script takes a
//! Series chunk as `polars::arrow::ArrayRef` (`Series::to_arrow`), inspects
//! it, reads its values through the checked readers, and gives it back
//! (`Series::from_arrow`); every observation equals the same calls in Rust.
//! Out-of-range indices are `OutOfBounds` before any Polars call, a wrong
//! reader is a `ConversionError` naming the dtype, and the array is moved,
//! as in Rust.
#![cfg(all(feature = "generated", feature = "test-support"))]
use rnx::rune::{self, Context, Module, Source, Sources, Vm};
use std::sync::Arc;
// Record 0130: a test here lowers the process-wide test limit, so every
// test in this binary holds this lock.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn run(script: &str) -> Result<rune::Value, String> {
	let mut polars = Module::with_crate("polars").unwrap();
	rnx_polars::build(&mut polars).unwrap();
	let mut fixtures = Module::with_crate("fx").unwrap();
	rnx_polars::generated::fixtures::install(&mut fixtures).unwrap();
	let mut context = Context::with_default_modules().unwrap();
	context.install(polars).unwrap();
	context.install(fixtures).unwrap();
	let runtime = Arc::new(context.runtime().unwrap());
	let mut sources = Sources::new();
	sources.insert(Source::memory(script).unwrap()).unwrap();
	let mut diagnostics = rune::Diagnostics::new();
	let built = rune::prepare(&mut sources)
		.with_context(&context)
		.with_diagnostics(&mut diagnostics)
		.build();
	if built.is_err() {
		panic!("diagnostics: {:?}", diagnostics.diagnostics());
	}
	let mut vm = Vm::new(runtime, Arc::new(built.unwrap()));
	vm.call(["main"], ()).map_err(|e| e.to_string())
}

use polars::prelude::*;

fn kind(v: rune::Value) -> String {
	match rune::from_value::<Result<rune::Value, rune::Value>>(v).unwrap() {
		Err(e) => rnx_polars::oracle::rune_error_kind(&e).unwrap(),
		Ok(_) => "accepted".into(),
	}
}

fn meta(s: &Series) -> (i64, i64, String) {
	let a = s.to_arrow(0, CompatLevel::newest());
	(
		a.len() as i64,
		a.null_count() as i64,
		format!("{:?}", a.dtype()),
	)
}

#[test]
fn numeric_chunk_inspect_read_and_round_trip() {
	let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
	let v = run(
		"pub fn main() { let s = polars::Series::from_iter_option_i64([Some(1), None, Some(3)])?; let a = s.to_arrow(0, polars::CompatLevel::newest())?; let m = (a.len()?, a.null_count()?, a.is_null(1)?, a.dtype_name()); let vals = a.values_i64()?; let wrong = a.values_str(); let back = polars::Series::from_arrow(\"x\", a)?; Ok((m, vals, wrong, back.len()?)) }",
	)
	.unwrap();
	let ((len, nulls, is_null_1, dtype), vals, wrong, back_len) = rune::from_value::<
		Result<((i64, i64, bool, String), Vec<Option<i64>>, rune::Value, i64), rune::Value>,
	>(v)
	.unwrap()
	.unwrap();
	let s = Series::new("x".into(), [Some(1i64), None, Some(3)]);
	assert_eq!((len, nulls, dtype), meta(&s));
	assert!(is_null_1);
	assert_eq!(vals, vec![Some(1), None, Some(3)]);
	assert_eq!(
		kind(wrong),
		"ConversionError",
		"a wrong reader names the dtype"
	);
	assert_eq!(back_len, 3);
}

#[test]
fn string_and_boolean_chunks_read_through_their_readers() {
	let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
	let v = run(
		"pub fn main() { let c = polars::CompatLevel::newest(); let s = polars::Series::from_iter_option_str([Some(\"a\"), None, Some(\"ccc\")])?.to_arrow(0, c)?; let b = polars::Series::from_iter_option_bool([Some(true), None, Some(false)])?.to_arrow(0, c)?; Ok((s.values_str()?, b.values_bool()?, s.values_i64(), s.dtype_name(), b.dtype_name())) }",
	)
	.unwrap();
	let (strs, bools, wrong, sd, bd) = rune::from_value::<
		Result<
			(
				Vec<Option<String>>,
				Vec<Option<bool>>,
				rune::Value,
				String,
				String,
			),
			rune::Value,
		>,
	>(v)
	.unwrap()
	.unwrap();
	assert_eq!(
		strs,
		vec![Some("a".to_string()), None, Some("ccc".to_string())]
	);
	assert_eq!(bools, vec![Some(true), None, Some(false)]);
	assert_eq!(kind(wrong), "ConversionError");
	assert_eq!(
		sd,
		meta(&Series::new("x".into(), [Some("a"), None, Some("ccc")])).2
	);
	assert_eq!(
		bd,
		meta(&Series::new("x".into(), [Some(true), None, Some(false)])).2
	);
}

#[test]
fn an_all_null_chunk_is_null_typed() {
	let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
	let v = run(
		"pub fn main() { let a = polars::Series::new_null(\"n\", 3)?.to_arrow(0, polars::CompatLevel::newest())?; Ok((a.null_count()?, a.dtype_name(), a.values_i64())) }",
	)
	.unwrap();
	let (nulls, dtype, typed) =
		rune::from_value::<Result<(i64, String, rune::Value), rune::Value>>(v)
			.unwrap()
			.unwrap();
	let want = meta(&Series::new_null("n".into(), 3));
	assert_eq!(nulls, want.1);
	assert_eq!(dtype, want.2);
	assert!(dtype.contains("Null"), "{dtype}");
	assert_eq!(
		kind(typed),
		"ConversionError",
		"a typed reader is a checked refusal"
	);
}

#[test]
fn indices_are_guarded_before_the_call() {
	let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
	let v = run(
		"pub fn main() { let c = polars::CompatLevel::newest(); let s = polars::Series::from_iter_option_i64([Some(1), None, Some(3)])?; let chunk = s.to_arrow(5, c); let a = s.to_arrow(0, c)?; Ok((chunk, a.is_null(99), a.sliced(2, 5), a.split_at_boxed(4), a.sliced(99, 0)?.len()?, a.sliced(1, 2)?.values_i64()?)) }",
	)
	.unwrap();
	let (chunk, is_null, sliced, split, empty_len, window) = rune::from_value::<
		Result<
			(
				rune::Value,
				rune::Value,
				rune::Value,
				rune::Value,
				i64,
				Vec<Option<i64>>,
			),
			rune::Value,
		>,
	>(v)
	.unwrap()
	.unwrap();
	for (what, r) in [
		("chunk", chunk),
		("is_null", is_null),
		("sliced", sliced),
		("split_at_boxed", split),
	] {
		assert_eq!(kind(r), "OutOfBounds", "{what}");
	}
	// Arrow's `sliced` returns an empty array for a zero length, unchecked
	assert_eq!(empty_len, 0);
	let a = Series::new("x".into(), [Some(1i64), None, Some(3)]).to_arrow(0, CompatLevel::newest());
	let rust: Vec<Option<i64>> = a
		.sliced(1, 2)
		.as_any()
		.downcast_ref::<polars_arrow::array::PrimitiveArray<i64>>()
		.unwrap()
		.iter()
		.map(|v| v.copied())
		.collect();
	assert_eq!(window, rust);
}

#[test]
fn the_array_is_moved_into_from_arrow() {
	let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
	let err = run(
		"pub fn main() { let a = polars::Series::from_iter_option_i64([Some(1)])?.to_arrow(0, polars::CompatLevel::newest())?; let s = polars::Series::from_arrow(\"x\", a)?; a.len() }",
	)
	.unwrap_err();
	assert!(
		err.contains("Cannot read, value is M"),
		"reusing a moved array is an access error: {err}"
	);
}

/// Review of 0119: the string reader bounds UTF-8 bytes, not only the count.
/// Under a 12-slot budget, two short-count strings whose bytes exceed it are
/// refused, one long string alone is refused, and the same receiver stays
/// usable and reads once the bound allows it.
#[test]
fn values_str_bounds_payload_bytes() {
	let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
	let v = run(
		"pub fn main() { let c = polars::CompatLevel::newest(); let two = polars::Series::from_iter_option_str([Some(\"aaaaaaaa\"), Some(\"bbbbbbbb\")])?.to_arrow(0, c)?; let one = polars::Series::from_iter_option_str([Some(\"xxxxxxxxxxxxxxxxxxxx\")])?.to_arrow(0, c)?; polars::set_materialize_limit(12); let r2 = two.values_str(); let r1 = one.values_str(); let still = two.len()?; polars::set_materialize_limit(0); Ok((r2, r1, still, two.values_str()?)) }",
	)
	.unwrap();
	let (two, one, still, after) = rune::from_value::<
		Result<(rune::Value, rune::Value, i64, Vec<Option<String>>), rune::Value>,
	>(v)
	.unwrap()
	.unwrap();
	assert_eq!(
		kind(two),
		"MaterializeLimit",
		"two strings over the byte budget"
	);
	assert_eq!(
		kind(one),
		"MaterializeLimit",
		"one long string over the byte budget"
	);
	assert_eq!(still, 2, "the receiver is usable after the refusal");
	assert_eq!(
		after,
		vec![Some("aaaaaaaa".to_string()), Some("bbbbbbbb".to_string())]
	);
}
