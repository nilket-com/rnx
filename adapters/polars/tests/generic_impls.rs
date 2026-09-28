//! Record 0113: generic trait impls through their family rules. Operators return a `Result` (`(a + b)?` in a script): faithful where the Rust impl is fallible, and a foreign right-hand side is catchable. They preserve nulls and Polars' own division semantics (frame arithmetic is v2-only, behind `dataframe_arithmetic`, and covered by the v2 oracle), keep their receiver usable and refuse a foreign right-hand side catchably; FromIterator converts script vectors with the existing narrow and null rules; DataFrame indexing clones the column out and panics exactly where Rust's `Index` panics.
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
use rnx_polars::generated::fixtures as fx;

/// A script value that is `Ok(wrapper)` or a wrapper, shown through `show`; an `Err` as its kind.
fn shown(
	v: rune::Value,
	show: fn(&rune::Value) -> Result<rnx_polars::oracle::Repr, String>,
) -> Result<String, String> {
	match rune::from_value::<Result<rune::Value, rune::Value>>(v.clone()) {
		Ok(Ok(w)) => Ok(show(&w).unwrap().to_text()),
		Ok(Err(e)) => Err(rnx_polars::oracle::rune_error_kind(&e).unwrap()),
		Err(_) => Ok(show(&v).unwrap().to_text()),
	}
}
fn series_text(s: &Series) -> String {
	rnx_polars::oracle::series_repr(s).to_text()
}
fn show_series(v: &rune::Value) -> Result<rnx_polars::oracle::Repr, String> {
	fx::show_w_polars_core__series__series(v)
}

#[test]
fn series_operators_preserve_nulls_and_match_rust() {
	let a: Series = [Some(1i64), None, Some(3)].into_iter().collect();
	let b: Series = [Some(10i64), Some(20), None].into_iter().collect();
	let setup = "let a = polars::Series::from_iter_option_i64([Some(1), None, Some(3)])?; let b = polars::Series::from_iter_option_i64([Some(10), Some(20), None])?;";
	let cases: Vec<(&str, Series)> = vec![
		("(a + b)?", (&a + &b).unwrap()),
		("(a - b)?", (&a - &b).unwrap()),
		("(a * b)?", (&a * &b).unwrap()),
		("(a * 2)?", &a * 2i64),
		("(a / 1.5)?", &a / 1.5f64),
		("(a % 2)?", &a % 2i64),
	];
	for (expr, direct) in cases {
		let got = shown(
			run(&format!("pub fn main() {{ {setup} Ok({expr}) }}")).unwrap(),
			show_series,
		)
		.unwrap();
		assert_eq!(got, series_text(&direct), "{expr}");
	}
}

#[test]
fn integer_division_by_zero_is_polars_own_semantics() {
	let a: Series = [Some(4i64), Some(0), None].into_iter().collect();
	let z: Series = [Some(0i64), Some(0), Some(0)].into_iter().collect();
	let got = shown(
		run("pub fn main() { let a = polars::Series::from_iter_option_i64([Some(4), Some(0), None])?; let z = polars::Series::from_iter_option_i64([Some(0), Some(0), Some(0)])?; Ok((a / z)?) }").unwrap(),
		show_series,
	)
	.unwrap();
	assert_eq!(got, series_text(&(&a / &z).unwrap()));
}

#[test]
fn the_receiver_stays_usable_and_a_foreign_rhs_is_refused() {
	let v = run("pub fn main() { let a = polars::Series::from_iter_i64([1, 2])?; let r = a + \"x\"; Ok((r.is_err(), a.len()?)) }").unwrap();
	let (refused, len) = rune::from_value::<Result<(bool, i64), rune::Value>>(v)
		.unwrap()
		.unwrap();
	assert!(refused && len == 2);
}

#[test]
fn from_iter_checks_narrowing_nulls_and_empty() {
	let got = shown(
		run("pub fn main() { polars::Series::from_iter_option_u8([Some(1), None, Some(255)]) }")
			.unwrap(),
		show_series,
	)
	.unwrap();
	let direct: Series = [Some(1u8), None, Some(255)].into_iter().collect();
	assert_eq!(got, series_text(&direct));
	let over = shown(
		run("pub fn main() { polars::Series::from_iter_u8([300]) }").unwrap(),
		show_series,
	);
	assert!(over.is_err(), "300 does not narrow to u8: {over:?}");
	let empty = shown(
		run("pub fn main() { polars::Series::from_iter_f64([]) }").unwrap(),
		show_series,
	)
	.unwrap();
	assert_eq!(
		empty,
		series_text(&Vec::<f64>::new().into_iter().collect::<Series>())
	);
	let strs = shown(
		run("pub fn main() { polars::Series::from_iter_option_str([Some(\"a\"), None]) }").unwrap(),
		show_series,
	)
	.unwrap();
	assert_eq!(
		strs,
		series_text(&[Some("a"), None].into_iter().collect::<Series>())
	);
}

#[test]
fn frame_indexing_clones_the_column_and_panics_like_rust() {
	let df = fx::values::df();
	for (key, direct) in [("\"x\"", df["x"].clone()), ("0", df[0].clone())] {
		let got = shown(
			run(&format!("pub fn main() {{ fx::df()[{key}] }}")).unwrap(),
			fx::show_w_polars_core__frame__column__column,
		)
		.unwrap();
		let want = rnx_polars::oracle::series_repr(direct.as_materialized_series()).to_text();
		assert_eq!(got, want, "{key}");
	}
	// a missing name: Rust's Index panics, and so does the binding (the oracle's both_panic)
	let rust = std::panic::catch_unwind(|| fx::values::df()["missing"].clone());
	let rune = std::panic::catch_unwind(|| run("pub fn main() { fx::df()[\"missing\"] }"));
	assert!(rust.is_err(), "Rust indexing a missing name panics");
	assert!(
		rune.is_err() || rune.as_ref().unwrap().is_err(),
		"the binding does not return a value"
	);
}

/// The materialize bound is process-wide under test-support: the bound test runs alone.
static LIMIT: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn from_iter_input_is_bounded_before_any_copy() {
	// review of 0113: exactly the bound passes, bound + 1 is refused before the
	// script vector is copied, and the source vector stays usable
	let _g = LIMIT.lock().unwrap_or_else(|e| e.into_inner());
	let v = run(
		"pub fn main() { polars::set_materialize_limit(3); let at = [1, 2, 3]; let over = [1, 2, 3, 4]; let a = polars::Series::from_iter_u8(at); let b = polars::Series::from_iter_u8(over); polars::set_materialize_limit(0); let kind = match b { Ok(_) => \"accepted\", Err(e) => e.kind() }; Ok((a?.len()?, kind, over.len(), at.len())) }",
	)
	.unwrap();
	let (n, kind, over_len, at_len) =
		rune::from_value::<Result<(i64, String, i64, i64), rune::Value>>(v)
			.unwrap()
			.unwrap();
	assert_eq!(
		(n, kind.as_str(), over_len, at_len),
		(3, "MaterializeLimit", 4, 3)
	);
}
