//! Record 0117: trait arguments. A generic trait's impls on one receiver are
//! one Rune function when their script kinds are disjoint (a series and a
//! string for `ChunkCompareEq`), and a value of no listed kind is a
//! ConversionError; a trait-argument bound proves through its projection
//! (`ChunkShift` on a numeric array); a listed `PrimitiveChunkedBuilder`
//! reaches `finish` through `ChunkedBuilder`; a bounded blanket impl gives its
//! receivers (`TemporalMethods` on a datetime `Series`, v2 only: the 0.55.2
//! narrow inventory has no `polars_time`). Each against the same calls in Rust.
#![cfg(all(feature = "generated", feature = "test-support"))]
use rnx::rune::{self, Context, Module, Source, Sources, Vm};
use std::sync::Arc;

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
use rnx_polars::generated::fixtures::values;

fn show_bool(v: &rune::Value) -> String {
	rnx_polars::generated::fixtures::show_w_polars_core__datatypes__booleanchunked(v)
		.unwrap()
		.to_text()
}

fn repr(s: &Series) -> String {
	rnx_polars::oracle::series_repr(s).to_text()
}

fn surface_has(path: &str) -> bool {
	std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("surface.json"))
		.unwrap()
		.contains(path)
}

/// Stage B: the two `ChunkCompareEq` arms of `StringChunked` behind one
/// `equal`, dispatched on the argument's kind, each equal to Rust's.
#[test]
fn compare_dispatches_on_the_argument_kind() {
	let v = run(
		"pub fn main() { let s = fx::series_str().str()?; let o = fx::series_str().str()?; Ok((s.equal(o)?, s.equal(\"bb\")?, s.gt(\"a\")?)) }",
	)
	.unwrap();
	let (series_arm, str_arm, gt_arm) =
		rune::from_value::<Result<(rune::Value, rune::Value, rune::Value), rune::Value>>(v)
			.unwrap()
			.unwrap();
	let s = values::series_str();
	let ca = s.str().unwrap();
	let want_series = repr(&ChunkCompareEq::<&StringChunked>::equal(ca, ca).into_series());
	let want_str = repr(&ChunkCompareEq::<&str>::equal(ca, "bb").into_series());
	let want_gt = repr(&ChunkCompareIneq::<&str>::gt(ca, "a").into_series());
	assert_eq!(show_bool(&series_arm), want_series);
	assert_eq!(show_bool(&str_arm), want_str);
	assert_eq!(show_bool(&gt_arm), want_gt);
	// a value of no listed kind is a ConversionError naming the parameter
	let bad = run("pub fn main() { let s = fx::series_str().str()?; s.equal(1) }").unwrap();
	match rune::from_value::<Result<rune::Value, rune::Value>>(bad).unwrap() {
		Err(e) => assert_eq!(
			rnx_polars::oracle::rune_error_kind(&e).unwrap(),
			"ConversionError"
		),
		Ok(_) => panic!("an int was accepted by a series/string dispatch"),
	}
}

/// Stage C: `ChunkShift<T>` on a numeric array needs `ChunkedArray<T>:
/// ChunkFull<T::Physical>`, a trait-argument bound proven through the
/// impl's projection argument; the call equals Rust's.
#[test]
fn numeric_shift_matches_rust() {
	let v = run("pub fn main() { let a = fx::series_i64().i64()?; Ok(a.shift(1)) }").unwrap();
	let got = rune::from_value::<Result<rune::Value, rune::Value>>(v)
		.unwrap()
		.unwrap();
	let got = rnx_polars::generated::fixtures::show_w_polars_core__datatypes__int64chunked(&got)
		.unwrap()
		.to_text();
	let s = values::series_i64();
	let want = repr(&ChunkShift::shift(s.i64().unwrap(), 1).into_series());
	assert_eq!(got, want);
}

/// Stage F: a listed `PrimitiveChunkedBuilder` from `new` through
/// `ChunkedBuilder::append_*` to `finish`, equal to Rust's.
#[test]
fn primitive_builder_chain_matches_rust() {
	let v = run(
		"pub fn main() { let b = polars::Int64ChunkedBuilder::new(\"x\", 4)?; b.append_value(1); b.append_null(); b.append_option(Some(3)); b.append_option(None); Ok(b.finish()) }",
	)
	.unwrap();
	let got = rune::from_value::<Result<rune::Value, rune::Value>>(v)
		.unwrap()
		.unwrap();
	let got = rnx_polars::generated::fixtures::show_w_polars_core__datatypes__int64chunked(&got)
		.unwrap()
		.to_text();
	let mut b = PrimitiveChunkedBuilder::<Int64Type>::new("x".into(), 4);
	b.append_value(1);
	b.append_null();
	b.append_option(Some(3));
	b.append_option(None);
	assert_eq!(got, repr(&b.finish().into_series()));
}

/// Stage D: `impl<T: Sized + AsSeries> TemporalMethods for T` gives
/// `Series` its temporal methods. The fixture is epoch milliseconds 1..3, so
/// Rust's `year` is 1970 in each row, under the input's name (the direct
/// `polars_time` call is not linked in the 0.55.2 build, which has no such
/// binding; the surface check skips it there).
#[test]
fn temporal_methods_on_a_datetime() {
	if !surface_has("polars_time::series::TemporalMethods::year") {
		eprintln!("no polars_time in this build's surface: skipped");
		return;
	}
	let v = run("pub fn main() { let s = fx::series_datetime(); Ok(s.year()?) }").unwrap();
	let got = rune::from_value::<Result<rune::Value, rune::Value>>(v)
		.unwrap()
		.unwrap();
	let got = rnx_polars::generated::fixtures::show_w_polars_core__datatypes__int32chunked(&got)
		.unwrap()
		.to_text();
	let want = Int32Chunked::from_slice("x".into(), &[1970, 1970, 1970]).into_series();
	assert_eq!(got, repr(&want));
}
