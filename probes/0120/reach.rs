//! Record 0120 reachability probe: which concrete Arrow arrays a script can
//! obtain. Every route is run twice, as a Rune script through the generated
//! bindings (proving the script reaches it; it reports the Arrow dtype) and
//! as the same Polars calls in Rust (naming the concrete array by downcast);
//! the two dtypes must agree. Prints one JSON row per (item, route, level).
#![cfg(all(feature = "generated", feature = "test-support"))]
use polars::prelude::*;
use polars_arrow::array::*;
use rnx::rune::{self, Context, Module, Source, Sources, Vm};
use std::sync::Arc;

fn run(script: &str) -> Result<Vec<String>, String> {
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
		.build()
		.map_err(|_| format!("compile: {:?}", diagnostics.diagnostics().first()))?;
	let mut vm = Vm::new(runtime, Arc::new(built));
	let v = vm.call(["main"], ()).map_err(|e| format!("vm: {e}"))?;
	match rune::from_value::<Result<Vec<String>, rune::Value>>(v).map_err(|e| e.to_string())? {
		Ok(v) => Ok(v),
		Err(e) => Err(format!(
			"script error: {}",
			rnx_polars::oracle::rune_error_kind(&e).unwrap_or_default()
		)),
	}
}

fn concrete(a: &dyn Array) -> String {
	let any = a.as_any();
	macro_rules! try_ty { ($($t:ty => $n:expr),* $(,)?) => { $(if any.is::<$t>() { return $n.to_string(); })* } }
	try_ty!(
		BooleanArray => "BooleanArray",
		Utf8ViewArray => "Utf8ViewArray",
		BinaryViewArray => "BinaryViewArray",
		Utf8Array<i32> => "Utf8Array<i32>",
		Utf8Array<i64> => "Utf8Array<i64>",
		BinaryArray<i32> => "BinaryArray<i32>",
		BinaryArray<i64> => "BinaryArray<i64>",
		ListArray<i32> => "ListArray<i32>",
		ListArray<i64> => "ListArray<i64>",
		FixedSizeListArray => "FixedSizeListArray",
		FixedSizeBinaryArray => "FixedSizeBinaryArray",
		StructArray => "StructArray",
		NullArray => "NullArray",
		MapArray => "MapArray",
		DictionaryArray<u32> => "DictionaryArray<u32>",
		DictionaryArray<u8> => "DictionaryArray<u8>",
		DictionaryArray<u16> => "DictionaryArray<u16>",
		// every primitive by exact identity, never inferred from the dtype
		PrimitiveArray<i8> => "PrimitiveArray<i8>",
		PrimitiveArray<i16> => "PrimitiveArray<i16>",
		PrimitiveArray<i32> => "PrimitiveArray<i32>",
		PrimitiveArray<i64> => "PrimitiveArray<i64>",
		PrimitiveArray<i128> => "PrimitiveArray<i128>",
		PrimitiveArray<u8> => "PrimitiveArray<u8>",
		PrimitiveArray<u16> => "PrimitiveArray<u16>",
		PrimitiveArray<u32> => "PrimitiveArray<u32>",
		PrimitiveArray<u64> => "PrimitiveArray<u64>",
		PrimitiveArray<u128> => "PrimitiveArray<u128>",
		PrimitiveArray<polars_utils::float16::pf16> => "PrimitiveArray<pf16>",
		PrimitiveArray<f32> => "PrimitiveArray<f32>",
		PrimitiveArray<f64> => "PrimitiveArray<f64>",
	);
	format!("unclassified({:?})", a.dtype())
}

struct Item {
	name: &'static str,
	/// a Rune expression evaluating to a Series
	rune: &'static str,
	rust: fn() -> PolarsResult<Series>,
}

fn i64s() -> Series {
	Series::new("x".into(), [Some(1i64), None, Some(3)])
}
fn strs() -> Series {
	Series::new("x".into(), [Some("a"), None, Some("ccc")])
}

fn items() -> Vec<Item> {
	vec![
		Item { name: "i8", rune: "polars::Series::from_iter_option_i8([Some(1), None])?", rust: || Ok(Series::new("x".into(), [Some(1i8), None])) },
		Item { name: "i16", rune: "polars::Series::from_iter_option_i16([Some(1), None])?", rust: || Ok(Series::new("x".into(), [Some(1i16), None])) },
		Item { name: "i32", rune: "polars::Series::from_iter_option_i32([Some(1), None])?", rust: || Ok(Series::new("x".into(), [Some(1i32), None])) },
		Item { name: "i64", rune: "polars::Series::from_iter_option_i64([Some(1), None, Some(3)])?", rust: || Ok(i64s()) },
		Item { name: "u8", rune: "polars::Series::from_iter_option_u8([Some(1), None])?", rust: || Ok(Series::new("x".into(), [Some(1u8), None])) },
		Item { name: "u16", rune: "polars::Series::from_iter_option_u16([Some(1), None])?", rust: || Ok(Series::new("x".into(), [Some(1u16), None])) },
		Item { name: "u32", rune: "polars::Series::from_iter_option_u32([Some(1), None])?", rust: || Ok(Series::new("x".into(), [Some(1u32), None])) },
		Item { name: "u64", rune: "polars::Series::from_iter_option_u64([Some(1), None])?", rust: || Ok(Series::new("x".into(), [Some(1u64), None])) },
		Item { name: "f32", rune: "polars::Series::from_iter_option_f32([Some(1.5), None])?", rust: || Ok(Series::new("x".into(), [Some(1.5f32), None])) },
		Item { name: "f64", rune: "polars::Series::from_iter_option_f64([Some(1.5), None])?", rust: || Ok(Series::new("x".into(), [Some(1.5f64), None])) },
		Item { name: "bool", rune: "polars::Series::from_iter_option_bool([Some(true), None])?", rust: || Ok(Series::new("x".into(), [Some(true), None])) },
		Item { name: "str", rune: "polars::Series::from_iter_option_str([Some(\"a\"), None, Some(\"ccc\")])?", rust: || Ok(strs()) },
		Item { name: "null", rune: "polars::Series::new_null(\"x\", 2)?", rust: || Ok(Series::new_null("x".into(), 2)) },
		Item { name: "binary", rune: "polars::Series::from_iter_option_str([Some(\"a\"), None, Some(\"ccc\")])?.cast(polars::DataType::Binary())?", rust: || strs().cast(&DataType::Binary) },
		Item { name: "binary_offset", rune: "polars::Series::from_iter_option_str([Some(\"a\"), None, Some(\"ccc\")])?.cast(polars::DataType::Binary())?.cast(polars::DataType::BinaryOffset())?", rust: || strs().cast(&DataType::Binary)?.cast(&DataType::BinaryOffset) },
		Item { name: "date", rune: "polars::Series::from_iter_option_i32([Some(1), None])?.cast(polars::DataType::Date())?", rust: || Series::new("x".into(), [Some(1i32), None]).cast(&DataType::Date) },
		Item { name: "datetime_ms", rune: "polars::Series::from_iter_option_i64([Some(1), None, Some(3)])?.cast(polars::DataType::Datetime(polars::TimeUnit::Milliseconds(), None)?)?", rust: || i64s().cast(&DataType::Datetime(TimeUnit::Milliseconds, None)) },
		Item { name: "duration_ms", rune: "polars::Series::from_iter_option_i64([Some(1), None, Some(3)])?.cast(polars::DataType::Duration(polars::TimeUnit::Milliseconds()))?", rust: || i64s().cast(&DataType::Duration(TimeUnit::Milliseconds)) },
		Item { name: "time", rune: "polars::Series::from_iter_option_i64([Some(1), None, Some(3)])?.cast(polars::DataType::Time())?", rust: || i64s().cast(&DataType::Time) },
		Item { name: "i128", rune: "polars::Series::from_iter_option_i64([Some(1), None, Some(3)])?.cast(polars::DataType::Int128())?", rust: || i64s().cast(&DataType::Int128) },
		Item { name: "u128", rune: "polars::Series::from_iter_option_i64([Some(1), None, Some(3)])?.cast(polars::DataType::UInt128())?", rust: || i64s().cast(&DataType::UInt128) },
		Item { name: "f16", rune: "polars::Series::from_iter_option_i64([Some(1), None, Some(3)])?.cast(polars::DataType::Float16())?", rust: || i64s().cast(&DataType::Float16) },
		Item { name: "decimal", rune: "polars::Series::from_iter_option_i64([Some(1), None, Some(3)])?.cast(polars::DataType::Decimal(10, 2)?)?", rust: || i64s().cast(&DataType::from_arrow_dtype(&polars_arrow::datatypes::ArrowDataType::Decimal(10, 2))) },
		Item { name: "list_i64", rune: "polars::Series::from_iter_option_i64([Some(1), None, Some(3)])?.implode()?.into_column().as_materialized_series()", rust: || Ok(i64s().implode()?.into_series()) },
		Item { name: "list_str", rune: "polars::Series::from_iter_option_str([Some(\"a\"), None, Some(\"ccc\")])?.implode()?.into_column().as_materialized_series()", rust: || Ok(strs().implode()?.into_series()) },
		Item { name: "struct", rune: "polars::StructChunked::from_columns(\"s\", 3, [polars::Column::from_series(polars::Series::from_iter_option_i64([Some(1), None, Some(3)])?), polars::Column::from_series(polars::Series::from_iter_option_str([Some(\"a\"), None, Some(\"ccc\")])?.with_name(\"y\"))])?.into_column().as_materialized_series()", rust: || Ok(StructChunked::from_columns("s".into(), 3, &[Column::from(i64s().with_name("".into())), Column::from(strs().with_name("y".into()))])?.into_series()) },
	]
}

fn rust_routes(s: &Series, level: CompatLevel) -> Vec<(&'static str, Box<dyn Array>)> {
	let mut v = vec![
		("Series::to_arrow", s.to_arrow(0, level)),
		("Column::rechunk_to_arrow", Column::from(s.clone()).rechunk_to_arrow(level)),
		("Series::into_chunks", s.clone().into_chunks().remove(0)),
	];
	if let Ok(l) = s.list() {
		if let Some(inner) = l.get(0) {
			v.push(("ListChunked::get", inner));
		}
	}
	v
}

const RUNE_ROUTES: &str = "let out = []; out.push(mk()?.to_arrow(0, c)?.dtype_name()); out.push(polars::Column::from_series(mk()?).rechunk_to_arrow(c).dtype_name()); out.push(mk()?.into_chunks()[0].dtype_name()); if let Ok(l) = mk()?.list() { if let Some(inner) = l.get(0)? { out.push(inner.dtype_name()); } } Ok(out)";

#[test]
fn reach() {
	let mut rows = Vec::new();
	for item in items() {
		for (lname, lrune, level) in [
			("newest", "polars::CompatLevel::newest()", CompatLevel::newest()),
			("oldest", "polars::CompatLevel::oldest()", CompatLevel::oldest()),
		] {
			let script = format!("fn mk() {{ Ok({}) }} pub fn main() {{ let c = {lrune}; {RUNE_ROUTES} }}", item.rune);
			// an item the script cannot reach is recorded, not mirrored (its
			// Rust constructor may need a feature this pin lacks)
			let script_res = run(&script);
			if let Err(e) = &script_res {
				rows.push(serde_json::json!({"item": item.name, "level": lname, "script": e}));
				continue;
			}
			let rust = (item.rust)().map(|s| rust_routes(&s, level));
			match (&rust, &script_res) {
				(Ok(routes), Ok(names)) => {
					assert_eq!(routes.len(), names.len(), "{} {lname}: route count", item.name);
					for ((route, a), name) in routes.iter().zip(names) {
						let rust_dtype = format!("{:?}", a.dtype());
						assert_eq!(&rust_dtype, name, "{} {lname} {route}: script and Rust disagree", item.name);
						rows.push(serde_json::json!({"item": item.name, "level": lname, "route": route, "arrow_dtype": name, "concrete": concrete(a.as_ref()), "script": "reached"}));
					}
				},
				_ => rows.push(serde_json::json!({"item": item.name, "level": lname, "rust": rust.as_ref().err().map(|e| e.to_string()), "script": script_res.err()})),
			}
		}
	}
	let unclassified: Vec<_> = rows.iter().filter(|r| r["concrete"].as_str().is_some_and(|c| c.starts_with("unclassified"))).collect();
	assert!(unclassified.is_empty(), "no exact identity: {unclassified:?}");
	let out = std::env::var("REACH_OUT").unwrap_or_else(|_| "reach.json".into());
	std::fs::write(&out, serde_json::to_string_pretty(&rows).unwrap()).unwrap();
	println!("reach rows: {} -> {out}", rows.len());
}
