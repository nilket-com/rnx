//! Record 0123: a schema displays, and `cast` takes a `DataType`.
//!
//! The schema's text is Polars' own derived `Debug` (Rust gives `Schema` no
//! `Display`); a template, `println!` and `{:?}` all show it. An
//! `impl Into<DataTypeExpr>` parameter takes a `DataTypeExpr` or a
//! `DataType` (converted with Polars' `From<DataType>`), dispatched before
//! the call; any other value is refused as a VM argument-type error (not a
//! script-catchable Result) naming both, and the binding stays infallible. A conversion Polars itself refuses surfaces as
//! Polars' error. Both pins bind `impl Into<DataTypeExpr>` (12 parameters
//! at v2, 6 at 0.55.2); `int_range` exists at v2 only.
#![cfg(all(feature = "generated", feature = "test-support"))]
use polars::prelude::*;
use rnx::rune::{self, Context, Module, Source, Sources, Vm};
use std::sync::Arc;

fn run(script: &str) -> Result<rune::Value, String> {
	let mut polars = Module::with_crate("polars").unwrap();
	rnx_polars::build(&mut polars).unwrap();
	let mut context = Context::with_default_modules().unwrap();
	context.install(polars).unwrap();
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

fn has(path: &str) -> bool {
	rnx_polars::generated::catalogue::CATALOGUE
		.iter()
		.any(|(p, _)| *p == path)
}

/// Both pins take `impl Into<DataTypeExpr>` (and bind the conversion).
fn v2() -> bool {
	has("polars::DataTypeExpr::from_data_type")
}

fn frame() -> DataFrame {
	df!("id" => [1i64, 2, 3], "qty" => [Some(3i64), None, Some(5)], "code" => ["7", "x", "9"])
		.unwrap()
}

const FRAME: &str = "polars::DataFrame::new(3, [polars::Column::from_series(polars::Series::from_iter_option_i64([Some(1), Some(2), Some(3)])?.with_name(\"id\")), polars::Column::from_series(polars::Series::from_iter_option_i64([Some(3), None, Some(5)])?.with_name(\"qty\")), polars::Column::from_series(polars::Series::from_iter_option_str([Some(\"7\"), Some(\"x\"), Some(\"9\")])?.with_name(\"code\"))])?";

fn strings(script: &str) -> Vec<String> {
	let v = run(script).unwrap_or_else(|e| panic!("{script}: {e}"));
	rune::from_value::<Result<Vec<String>, rune::Value>>(v)
		.unwrap()
		.unwrap_or_else(|e| panic!("{script}: {:?}", rnx_polars::oracle::rune_error_kind(&e)))
}

#[test]
fn a_schema_displays_as_polars_shows_it() {
	if !v2() {
		return;
	}
	let rust = format!("{:?}", frame().schema());
	let got = strings(&format!(
		"pub fn main() {{ let df = {FRAME}; let s = df.schema(); Ok([`${{s}}`, format!(\"{{:?}}\", s)]) }}"
	));
	assert_eq!(got, [rust.clone(), rust]);
}

#[test]
fn cast_takes_a_dtype_or_a_dtype_expression() {
	if !v2() {
		return;
	}
	let rust = rnx_polars::oracle::frame_repr(
		&frame()
			.lazy()
			.with_column(col("qty").cast(DataType::Float64))
			.collect()
			.unwrap(),
	)
	.to_text();
	let got = strings(&format!(
		"pub fn main() {{ let df = {FRAME}; \
		 let a = df.lazy().with_column(polars::col(\"qty\").cast(polars::DataType::Float64())).collect()?; \
		 let df = {FRAME}; \
		 let b = df.lazy().with_column(polars::col(\"qty\").cast(polars::DataTypeExpr::from_data_type(polars::DataType::Float64()))).collect()?; \
		 Ok([polars::oracle_repr(a)?, polars::oracle_repr(b)?]) }}"
	));
	assert_eq!(got, [rust.clone(), rust]);
	// the free range function takes a dtype too (at v2: the 0.55.2 build has no int_range)
	if !has("polars::int_range") {
		return;
	}
	let got = strings(
		"pub fn main() { let e = polars::int_range(polars::lit(0)?, polars::lit(3)?, 1, polars::DataType::Int32()); Ok([`${polars::DataFrame::empty().lazy().select_([e.alias(\"r\")])?.collect()?.dtypes()[0]}`]) }",
	);
	assert_eq!(got, ["i32"]);
}

#[test]
fn another_argument_is_refused_before_polars() {
	if !v2() {
		return;
	}
	// the boundary (plan section 2, review of 0123): a VM argument-type error,
	// not a script-catchable Result; its exact message names the operation,
	// the argument, both accepted types and the type found
	let e = run("pub fn main() { polars::col(\"qty\").cast(\"f64\") }").unwrap_err();
	assert!(
		e.contains("cast: `dtype` must be DataTypeExpr or DataType, found ::std::string::String"),
		"{e}"
	);
	let e = run("pub fn main() { polars::col(\"qty\").strict_cast(3) }").unwrap_err();
	assert!(
		e.contains("strict_cast: `dtype` must be DataTypeExpr or DataType, found ::std::i64"),
		"{e}"
	);
	// the script cannot catch it as a Result: a match on the call never runs
	let e = run(
		"pub fn main() { match polars::col(\"qty\").cast(\"f64\") { Ok(_) => \"ok\", Err(_) => \"caught\" } }",
	)
	.unwrap_err();
	assert!(e.contains("must be DataTypeExpr or DataType"), "{e}");
	// and a valid call returns the Expr itself, not a Result: it chains directly
	let got = strings(&format!(
		"pub fn main() {{ let df = {FRAME}; let e = polars::col(\"qty\").cast(polars::DataType::Float64()).alias(\"q\"); Ok([`${{df.lazy().select_([e])?.collect()?.dtypes()[0]}}`]) }}"
	));
	assert_eq!(got, ["f64"]);
	// the value passed is only borrowed: a dtype stays usable afterwards
	let got = strings(&format!(
		"pub fn main() {{ let t = polars::DataType::Float64(); let e = polars::col(\"qty\").cast(t); let e2 = polars::col(\"id\").cast(t); let df = {FRAME}; Ok([`${{df.lazy().select_([e, e2])?.collect()?.dtypes()[1]}}`]) }}"
	));
	assert_eq!(got, ["f64"]);
}

#[test]
fn a_conversion_polars_refuses_is_its_error() {
	if !v2() {
		return;
	}
	let rust = frame()
		.lazy()
		.with_column(col("code").strict_cast(DataType::Int64))
		.collect()
		.unwrap_err();
	// the hand-written `collect` reports Polars' error as its text (not the
	// adapter's typed Error): the message passes through unchanged
	let got = strings(&format!(
		"pub fn main() {{ let df = {FRAME}; match df.lazy().with_column(polars::col(\"code\").strict_cast(polars::DataType::Int64())).collect() {{ Ok(_) => Ok([\"accepted\"]), Err(e) => Ok([e]) }} }}"
	));
	assert_eq!(got, [format!("polars collect: {rust}")]);
}
