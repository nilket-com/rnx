//! Record 0116: generic methods admitted by family rules. A `Schema` (the
//! `Schema<DataType>` alias of the generic `polars_schema::Schema`) behaves as
//! direct Rust does; a listed dtype builder (`ListPrimitiveChunkedBuilder<T>`,
//! per dtype) is reachable from construction to a finished list; a time zone
//! crosses as its IANA name, an unknown name refused (v2 only: the 0.55.2
//! narrow build has no time-zone callables).
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

/// Rule 2: the `Schema` alias's methods, against the same calls in Rust.
#[test]
fn schema_alias_methods_match_rust() {
	let v = run(
		"pub fn main() { let s = polars::Schema::with_capacity(2)?; s.insert(\"a\", polars::DataType::Int64()); s.insert(\"b\", polars::DataType::Float64()); Ok((s.len()?, s.contains(\"b\"), s.contains(\"z\"), s.index_of(\"b\")?, s.index_of(\"z\")?)) }",
	)
	.unwrap();
	let got =
		rune::from_value::<Result<(i64, bool, bool, Option<i64>, Option<i64>), rune::Value>>(v)
			.unwrap()
			.unwrap();
	let mut s = Schema::with_capacity(2);
	s.insert("a".into(), DataType::Int64);
	s.insert("b".into(), DataType::Float64);
	let want = (
		s.len() as i64,
		s.contains("b"),
		s.contains("z"),
		s.index_of("b").map(|i| i as i64),
		s.index_of("z").map(|i| i as i64),
	);
	assert_eq!(got, want);
}

/// Rule 3: a listed builder is reachable from construction to a result, and
/// the finished list equals the one Rust builds with the same calls.
#[test]
fn list_builder_chain_matches_rust() {
	let v = run(
		"pub fn main() { let b = polars::ListInt64ChunkedBuilder::new(\"x\", 3, 4, polars::DataType::Int64())?; b.append_slice([1, 2])?; b.append_null(); b.append_slice([3])?; Ok(b.finish()) }",
	)
	.unwrap();
	let list = rune::from_value::<Result<rune::Value, rune::Value>>(v)
		.unwrap()
		.unwrap();
	let got = rnx_polars::generated::fixtures::show_w_polars_core__datatypes__listchunked(&list)
		.unwrap()
		.to_text();
	let mut b = ListPrimitiveChunkedBuilder::<Int64Type>::new("x".into(), 3, 4, DataType::Int64);
	b.append_slice(&[1, 2]);
	b.append_null();
	b.append_slice(&[3]);
	let want = rnx_polars::oracle::series_repr(&b.finish().into_series()).to_text();
	assert_eq!(got, want);
}

/// Rule 6: a time zone is its IANA name; an unknown name is a
/// ConversionError naming the parameter. Only where the surface has
/// time-zone callables (v2); the 0.55.2 narrow inventory has none.
#[test]
fn time_zone_is_its_iana_name() {
	let surface = std::fs::read_to_string(
		std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("surface.json"),
	)
	.unwrap();
	if !surface.contains("polars_core::datatypes::temporal::time_zone::TimeZone::from_chrono") {
		eprintln!("no time-zone callables in this build's surface: skipped");
		return;
	}
	let v = run(
		"pub fn main() { let tz = polars::TimeZone::from_chrono(\"Europe/Paris\")?; Ok(tz.to_chrono()?) }",
	)
	.unwrap();
	let name = rune::from_value::<Result<String, rune::Value>>(v)
		.unwrap()
		.unwrap();
	assert_eq!(name, "Europe/Paris");
	let bad = run("pub fn main() { polars::TimeZone::from_chrono(\"Nowhere/Nope\") }").unwrap();
	let kind = match rune::from_value::<Result<rune::Value, rune::Value>>(bad).unwrap() {
		Err(e) => rnx_polars::oracle::rune_error_kind(&e).unwrap(),
		Ok(_) => panic!("an unknown time zone was accepted"),
	};
	assert_eq!(kind, "ConversionError");
}
