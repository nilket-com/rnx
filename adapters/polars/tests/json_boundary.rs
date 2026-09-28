//! Record 0112: the serde JSON boundary. `to_json`/`from_json` round-trip frame-free values exactly as serde_json does; frames and series, which Polars serializes through an unbounded IPC buffer, have no JSON binding; the byte bound is inclusive and counts UTF-8 bytes in both directions; invalid text and an over-bound text are catchable errors; a refused conversion leaves its receiver usable.
#![cfg(all(feature = "generated", feature = "test-support"))]
use rnx::rune::{self, Context, Module, Source, Sources, Vm};
use std::sync::{Arc, Mutex};

/// The JSON bound is process-wide under test-support: one test at a time.
static SERIAL: Mutex<()> = Mutex::new(());

fn run(script: &str, arg: &str) -> Result<rune::Value, String> {
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
	vm.call(["main"], (arg.to_string(),))
		.map_err(|e| e.to_string())
}

/// `Ok(text)` or `Err(kind)` from a script returning `Result<String, Error>`.
fn text(v: rune::Value) -> Result<String, String> {
	match rune::from_value::<Result<rune::Value, rune::Value>>(v).unwrap() {
		Ok(s) => Ok(rune::from_value::<String>(s).unwrap()),
		Err(e) => Err(rnx_polars::oracle::rune_error_kind(&e).unwrap()),
	}
}

use polars::prelude::*;

/// A frame serializes as IPC bytes (ASCII JSON); a `RowIndex` carries its name as JSON text
/// (a `Field` holds a `DataType`, which can carry a `Series`, so it has no JSON binding).
fn unicode_field() -> polars_io::RowIndex {
	polars_io::RowIndex {
		name: "naïve Zürich 日本".into(),
		offset: 3,
	}
}

#[test]
fn a_value_round_trips_as_serde_json_does() {
	let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
	let field = unicode_field();
	let direct = serde_json::to_string(&field).unwrap();
	let again = text(
		run(
			"pub fn main(s) { polars::RowIndex::from_json(s)?.to_json() }",
			&direct,
		)
		.unwrap(),
	)
	.unwrap();
	assert_eq!(again, direct);
	let opts = SortOptions::default();
	let direct = serde_json::to_string(&opts).unwrap();
	let got = text(
		run(
			"pub fn main(s) { polars::SortOptions::from_json(s)?.to_json() }",
			&direct,
		)
		.unwrap(),
	)
	.unwrap();
	assert_eq!(got, direct);
}

#[test]
fn non_ascii_text_round_trips_and_the_bound_counts_bytes() {
	let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
	let json = serde_json::to_string(&unicode_field()).unwrap();
	assert!(
		json.len() > json.chars().count(),
		"the fixture must contain multi-byte UTF-8"
	);
	let n = json.len() as i64;
	// exactly the bound in bytes: both parse and write succeed
	let at = format!(
		"pub fn main(s) {{ polars::set_json_limit({n}); let r = polars::RowIndex::from_json(s)?.to_json(); polars::set_json_limit(0); r }}"
	);
	assert_eq!(text(run(&at, &json).unwrap()), Ok(json.clone()));
	// one byte less: the text is refused before any parse
	let under = format!(
		"pub fn main(s) {{ polars::set_json_limit({}); let r = polars::RowIndex::from_json(s); polars::set_json_limit(0); r.map(|_| \"parsed\") }}",
		n - 1
	);
	assert_eq!(text(run(&under, &json).unwrap()), Err("JsonLimit".into()));
	// the writer refuses the byte that would cross the bound
	let write_under = format!(
		"pub fn main(s) {{ let f = polars::RowIndex::from_json(s)?; polars::set_json_limit({}); let r = f.to_json(); polars::set_json_limit(0); r }}",
		n - 1
	);
	assert_eq!(
		text(run(&write_under, &json).unwrap()),
		Err("JsonLimit".into())
	);
}

#[test]
fn invalid_json_is_a_catchable_error() {
	let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
	for bad in ["", "{", "not json", "[1, 2]"] {
		let got = text(
			run(
				"pub fn main(s) { polars::RowIndex::from_json(s).map(|_| \"parsed\") }",
				bad,
			)
			.unwrap(),
		);
		assert_eq!(got, Err("Json".into()), "{bad:?}");
		assert!(
			serde_json::from_str::<polars_io::RowIndex>(bad).is_err(),
			"{bad:?}"
		);
	}
}

#[test]
fn a_refused_conversion_leaves_the_receiver_usable() {
	let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
	let json = serde_json::to_string(&unicode_field()).unwrap();
	let got = text(run(
		"pub fn main(s) { let f = polars::RowIndex::from_json(s)?; polars::set_json_limit(8); let refused = f.to_json(); polars::set_json_limit(0); match refused { Ok(_) => Ok(\"not refused\"), Err(e) => f.to_json() } }",
		&json,
	).unwrap())
	.unwrap();
	assert_eq!(got, json);
}

#[test]
fn frames_and_series_have_no_json_binding() {
	// review of 0112: their Serialize encodes IPC into an unbounded buffer first
	let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
	for script in [
		"pub fn main(_s) { fx::df().to_json() }",
		"pub fn main(_s) { fx::series().to_json() }",
	] {
		let e = run(script, "").unwrap_err();
		assert!(e.contains("Missing instance function"), "{script}: {e}");
	}
}
