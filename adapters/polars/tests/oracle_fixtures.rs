//! Record 0121: the repaired oracle fixtures build what they claim. Each
//! typed source fixture has its intended physical dtype and values, a null
//! in the middle, and Rune and Rust agree on it; the boundary values a
//! width allows (beyond i64 for the 128-bit integers, a subnormal for f16)
//! come from string and float sources on both sides, since an i64 source
//! cannot supply them. The pin fixtures (map, extension, plan, fractions)
//! hold valid values of their types. The typed producers and pin types
//! exist at v2 only.
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

/// The script's strings, each call's outcome `ok <value>` or `err <kind>`.
fn outcomes(setup: &str, calls: &[&str]) -> Vec<String> {
	let body: Vec<String> = calls
		.iter()
		.map(|c| {
			String::from("match ") + c + " { Ok(v) => `ok ${v}`, Err(e) => `err ${e.kind()}` }"
		})
		.collect();
	let script = format!(
		"pub fn main() {{ let a = {setup}; Ok([{}]) }}",
		body.join(", ")
	);
	let v = run(&script).unwrap_or_else(|e| panic!("{script}: {e}"));
	rune::from_value::<Result<Vec<String>, rune::Value>>(v)
		.unwrap()
		.unwrap_or_else(|_| panic!("{script}: setup failed"))
}

fn v2() -> bool {
	rnx_polars::generated::catalogue::CATALOGUE
		.iter()
		.any(|(p, _)| *p == "polars::Series::i128")
}

use polars::prelude::*;
use rnx_polars::generated::fixtures::values;

#[test]
fn typed_sources_have_their_dtype_values_and_a_null() {
	if !v2() {
		// no producer consumes them at 0.55.2; they only need to compile
		return;
	}
	// the chunk the script reads each fixture's values through (a
	// Float16Chunked's `get` is the opaque pf16, so f16 reads via Float64)
	for (fx, dtype, unpack) in [
		("series_i128", DataType::Int128, "i128()"),
		("series_u128", DataType::UInt128, "u128()"),
		(
			"series_f16",
			DataType::Float16,
			"cast(polars::DataType::Float64()).and_then(|s| s.f64())",
		),
	] {
		let rust = match fx {
			"series_i128" => values::series_i128(),
			"series_u128" => values::series_u128(),
			_ => values::series_f16(),
		};
		assert_eq!(rust.dtype(), &dtype, "{fx}");
		assert_eq!((rust.len(), rust.null_count()), (3, 1), "{fx}");
		let first = rust.get(0).unwrap().to_string();
		let last = rust.get(2).unwrap().to_string();
		// the Rune side unpacks the same fixture through its producer
		let got = outcomes(
			&format!("fx::{fx}()"),
			&[
				&format!("a.{unpack}.and_then(|c| c.len())"),
				&format!(
					"a.{unpack}.and_then(|c| c.get(1)).map(|v| match v {{ Some(_) => \"some\", None => \"null\" }})"
				),
				&format!("a.{unpack}.and_then(|c| c.get(0)).map(|v| `${{v.unwrap()}}`)"),
				&format!("a.{unpack}.and_then(|c| c.get(2)).map(|v| `${{v.unwrap()}}`)"),
			],
		);
		assert_eq!(
			got,
			[
				"ok 3".to_string(),
				"ok null".into(),
				format!("ok {first}"),
				format!("ok {last}"),
			],
			"{fx}"
		);
	}
	// decimal: its physical dtype and scale, and the null (by name: the
	// `Decimal` variant does not exist at 0.55.2, where this file compiles too)
	let d = values::series_decimal();
	assert_eq!(format!("{:?}", d.dtype()), "Decimal(10, 2)");
	assert_eq!((d.len(), d.null_count()), (3, 1));
	assert_eq!(
		outcomes(
			"fx::series_decimal()",
			&["a.decimal().and_then(|c| c.len())"]
		),
		["ok 3"]
	);
}

#[test]
fn boundary_values_come_from_string_and_float_sources() {
	if !v2() {
		return;
	}
	// beyond i64: exact in Rust, a checked ConversionError at the script
	// boundary (0093's widen), never wrapped
	let big = "170141183460469231731687303715884105727";
	let s = Series::new("x".into(), [Some(big), None])
		.strict_cast(&DataType::Int128)
		.unwrap();
	assert_eq!(s.get(0).unwrap().to_string(), i128::MAX.to_string());
	let got = outcomes(
		&format!(
			"polars::Series::from_iter_option_str([Some(\"{big}\"), None])?.strict_cast(polars::DataType::Int128())?"
		),
		&[
			"a.i128().and_then(|c| c.get(0))",
			"a.i128().and_then(|c| c.null_count())",
		],
	);
	assert_eq!(got, ["err ConversionError", "ok 1"]);
	let umax = "340282366920938463463374607431768211455";
	let u = Series::new("x".into(), [Some(umax)])
		.strict_cast(&DataType::UInt128)
		.unwrap();
	assert_eq!(u.get(0).unwrap().to_string(), u128::MAX.to_string());
	assert_eq!(
		outcomes(
			&format!(
				"polars::Series::from_iter_option_str([Some(\"{umax}\")])?.strict_cast(polars::DataType::UInt128())?"
			),
			&["a.u128().and_then(|c| c.get(0))"]
		),
		["err ConversionError"]
	);
	// an f16 subnormal from a float source: exact on both sides
	let tiny = 2f64.powi(-24);
	let f = Series::new("x".into(), [Some(tiny), None])
		.cast(&DataType::Float16)
		.unwrap();
	assert_eq!(f.dtype(), &DataType::Float16);
	assert_eq!(f.null_count(), 1);
	let rust_tiny = f
		.cast(&DataType::Float64)
		.unwrap()
		.f64()
		.unwrap()
		.get(0)
		.unwrap();
	assert_eq!(rust_tiny, tiny);
	let got = outcomes(
		"polars::Series::from_iter_option_f64([Some(0.000000059604644775390625), None])?.cast(polars::DataType::Float16())?.cast(polars::DataType::Float64())?",
		&[
			"a.f64().and_then(|c| c.get(0)).map(|v| `${v.unwrap()}`)",
			"a.null_count()",
		],
	);
	assert_eq!(got, [format!("ok {rust_tiny:?}"), "ok 1".into()]);
}

#[test]
fn pin_fixtures_hold_valid_values() {
	if !v2() {
		// the pin types are not wrapped at 0.55.2 (DslPlan is, and its fixture
		// is checked below at both pins)
		return;
	}
	// checked through the script, since the pin types exist at v2 only
	assert_eq!(
		outcomes("fx::map_chunked()", &["Ok(`${a.dtype()}`)", "a.len()",]),
		["ok map[str, i64]", "ok 1"]
	);
	assert_eq!(
		outcomes(
			"fx::extension_chunked()",
			&["Ok(`${a.dtype()}`)", "a.len()"]
		),
		["ok ext[rnx.oracle]", "ok 3"]
	);
	assert_eq!(
		outcomes("fx::extension_type()", &["Ok(`${a}`)"]),
		["ok rnx.oracle"]
	);
	assert_eq!(
		outcomes("fx::fractions()", &["Ok(`${a == fx::fractions()}`)"]),
		["ok true"]
	);
}

#[test]
fn the_plan_fixture_collects_to_its_frame() {
	// DslPlan is wrapped at both pins
	let lf: LazyFrame = values::dsl_plan().into();
	assert_eq!(lf.collect().unwrap().shape(), (3, 1));
	assert_eq!(
		outcomes(
			"fx::dsl_plan()",
			&[
				"polars::LazyFrame::from_logical_plan(a, polars::OptFlags::default_()).collect().and_then(|df| df.height())"
			]
		),
		["ok 3"]
	);
}
