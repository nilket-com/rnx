//! Record 0110: trait methods dispatched to receivers proven from the trait's impl records (recovered by trait identity). A list, a binary, a generic chunked trait on two dtypes, and a frame join trait match direct Polars; a method whose trait has no proven wrapped receiver stays refused with its reason.
#![cfg(all(feature = "generated", feature = "test-support"))]
use rnx::rune::{self, Context, Module, Source, Sources, Vm};
use std::sync::Arc;

fn run(script: &str) -> rune::Value {
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
	vm.call(["main"], ()).unwrap()
}

use polars::prelude as p;
use polars_core::chunked_array::ops::ChunkUnique;
use polars_ops::prelude::*;
use rnx_polars::generated::fixtures::{self as fx, values};

/// Values of an integer array in order, `n` for a null, joined by commas: the same text the script builds.
const SHOW: &str = r#"fn show(ca) { let s = []; let i = 0; while i < ca.len().unwrap() { s.push(match ca.get(i).unwrap() { Some(v) => `${v}`, None => "n" }); i = i + 1; } s.iter().fold("", |a, b| if a == "" { b } else { a + "," + b }) }"#;
fn show<T: p::PolarsNumericType>(ca: &p::ChunkedArray<T>) -> String
where
	T::Native: std::fmt::Display,
{
	ca.iter()
		.map(|v| v.map_or("n".to_string(), |v| v.to_string()))
		.collect::<Vec<_>>()
		.join(",")
}
fn text(v: rune::Value) -> String {
	rune::from_value::<String>(v).unwrap()
}

#[test]
fn list_lengths_match_direct_polars() {
	let got = text(run(&format!(
		"{SHOW} pub fn main() {{ show(fx::series_list().list()?.lst_lengths()) }}"
	)));
	assert_eq!(
		got,
		show(&values::series_list().list().unwrap().lst_lengths())
	);
}

#[test]
fn binary_size_bytes_matches_direct_polars() {
	let got = text(run(&format!(
		"{SHOW} pub fn main() {{ show(fx::series_binary().binary()?.size_bytes()) }}"
	)));
	assert_eq!(
		got,
		show(&values::series_binary().binary().unwrap().size_bytes())
	);
}

#[test]
fn a_generic_chunked_trait_dispatches_per_dtype() {
	// `ChunkUnique` is implemented for several ChunkedArray instantiations; each alias wrapper gets its own binding
	let got = rune::from_value::<(i64, i64)>(run(
		"pub fn main() { (fx::series_str().str()?.n_unique()?, fx::series_bool().bool()?.n_unique()?) }",
	))
	.unwrap();
	let s = values::series_str().str().unwrap().n_unique().unwrap() as i64;
	let b = values::series_bool().bool().unwrap().n_unique().unwrap() as i64;
	assert_eq!(got, (s, b));
}

#[test]
fn a_frame_join_trait_matches_direct_polars() {
	let v = run(r#"pub fn main() { fx::df().inner_join(fx::df(), ["x"], ["x"]) }"#);
	let v = match rune::from_value::<Result<rune::Value, rune::Value>>(v.clone()) {
		Ok(Ok(w)) => w,
		Ok(Err(e)) => panic!("{}", rnx_polars::oracle::rune_error_kind(&e).unwrap()),
		Err(_) => v,
	};
	let direct = values::df()
		.inner_join(&values::df(), ["x"], ["x"])
		.unwrap();
	assert_eq!(
		fx::show_dataframe(&v).unwrap().to_text(),
		rnx_polars::oracle::frame_repr(&direct).to_text()
	);
}

#[test]
fn unproven_receivers_stay_refused_with_a_reason() {
	let surface: serde_json::Value = serde_json::from_str(
		&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/surface.json")).unwrap(),
	)
	.unwrap();
	let refused: Vec<&serde_json::Value> = surface["entries"]
		.as_array()
		.unwrap()
		.iter()
		.filter(|e| {
			e["status"] == "unsupported"
				&& e["reason"]
					.as_str()
					.is_some_and(|r| r.starts_with("no wrapped implementor"))
		})
		.collect();
	assert!(!refused.is_empty(), "the negative class exists");
	// a blanket-only trait is never given a receiver
	assert!(
		refused
			.iter()
			.any(|e| e["reason"].as_str().unwrap().contains("blanket")),
		"a blanket-only refusal is kept"
	);
	// nothing bound is labelled by a bare generic base: every binding has a concrete receiver
	for e in surface["entries"]
		.as_array()
		.unwrap()
		.iter()
		.filter(|e| e["status"] == "generated")
	{
		for b in e["bindings"].as_array().into_iter().flatten() {
			assert!(
				b["receiver"].as_str() != Some("polars_core::chunked_array::ChunkedArray"),
				"{}",
				e["canonical_path"]
			);
		}
	}
}
