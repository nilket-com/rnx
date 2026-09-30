//! Record 0120: the concrete Arrow arrays a script reaches, end to end. A
//! script takes a Series chunk as `polars::arrow::ArrayRef`, downcasts it to
//! its exact concrete array (a wrong downcast is a `ConversionError` naming
//! the dtype), reads it through the generated methods, and gives it back;
//! every value equals the same calls in Rust. Index and range arguments are
//! `OutOfBounds` before any Polars call. Wide integers read back checked
//! (never wrapped, never a partial vector), binary16 reads back exactly,
//! and every copy is bounded before it starts.
#![cfg(all(feature = "generated", feature = "test-support"))]
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

/// Each call's outcome as a string: `ok <value>` or `err <kind>`.
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
	match rune::from_value::<Result<Vec<String>, rune::Value>>(v).unwrap() {
		Ok(v) => v,
		Err(e) => panic!(
			"{script}: setup failed: {:?}",
			rnx_polars::oracle::rune_error_kind(&e)
		),
	}
}

/// Whether this build is the v2 pin (its catalogue has the v2-only natives).
fn v2() -> bool {
	let has = |k: &str| {
		rnx_polars::generated::catalogue::CATALOGUE
			.iter()
			.any(|(p, _)| *p == k)
	};
	let v2 = has("polars::arrow::ArrayRef::as_float16_array");
	assert_eq!(
		v2,
		has("polars::DataType::Decimal"),
		"pin detection disagrees"
	);
	v2
}

fn chunk(series: &str, level: &str) -> String {
	format!("{series}.to_arrow(0, polars::CompatLevel::{level}())?")
}

const I64S: &str = "polars::Series::from_iter_option_i64([Some(1), None, Some(3)])?";
const STRS: &str = "polars::Series::from_iter_option_str([Some(\"a\"), None, Some(\"ccc\")])?";

#[test]
fn primitive_read_guard_and_round_trip() {
	let a = format!("{}.as_int64_array()?", chunk(I64S, "newest"));
	assert_eq!(
		outcomes(
			&a,
			&[
				"a.len()",
				"a.value(0)",
				"a.value(2)",
				"a.value(3)",
				"a.values().map(|v| `${v[0]},${v[2]}`)",
				"a.validity().map(|v| match v { Some(b) => `${b[0]},${b[1]},${b[2]}`, None => \"none\" })",
				"a.sliced(1, 2).and_then(|x| x.value(1))",
				"a.sliced(3, 0).and_then(|x| x.len())",
				"a.sliced(4, 0).and_then(|x| x.len())",
				"a.sliced(2, 2).and_then(|x| x.len())",
				"polars::Series::from_arrow(\"x\", a.boxed()).and_then(|s| s.len())",
			]
		),
		[
			"ok 3",
			"ok 1",
			"ok 3",
			"err OutOfBounds",
			"ok 1,3",
			"ok true,false,true",
			"ok 3",
			"ok 0",
			"err OutOfBounds",
			"err OutOfBounds",
			"ok 3",
		]
	);
}

#[test]
fn wrong_downcast_names_the_dtype() {
	let a = chunk(STRS, "newest");
	assert_eq!(
		outcomes(
			&a,
			&[
				"a.as_large_string_array().and_then(|x| x.len())",
				"a.as_int64_array().and_then(|x| x.len())",
				"a.as_large_list_array().and_then(|x| x.len())",
			]
		),
		[
			"err ConversionError",
			"err ConversionError",
			"err ConversionError"
		]
	);
	// the message names the actual Arrow dtype
	let v = run(&format!(
		"pub fn main() {{ let a = {a}; match a.as_int64_array() {{ Ok(_) => Ok(\"accepted\"), Err(e) => Ok(e.message()) }} }}"
	))
	.unwrap();
	let msg = rune::from_value::<Result<String, rune::Value>>(v)
		.unwrap()
		.unwrap();
	assert!(
		msg.contains("Utf8View") && msg.contains("as_int64_array"),
		"{msg}"
	);
}

#[test]
fn large_string_and_binary_at_the_oldest_level() {
	let s = format!("{}.as_large_string_array()?", chunk(STRS, "oldest"));
	let none = "match o { Some(s) => s, None => \"None\" }";
	assert_eq!(
		outcomes(
			&s,
			&[
				"a.value(2)",
				"a.value(3)",
				&format!("a.get(1).map(|o| {none})"),
				&format!("a.get(0).map(|o| {none})"),
				"a.get(3).map(|o| 0)",
				"a.offsets().map(|v| `${v[0]},${v[1]},${v[2]},${v[3]}`)",
				"a.values().map(|v| v.len())",
				"polars::Series::from_arrow(\"x\", a.boxed()).and_then(|s| s.len())",
			]
		),
		[
			"ok ccc",
			"err OutOfBounds",
			"ok None",
			"ok a",
			"err OutOfBounds",
			"ok 0,1,1,4",
			"ok 4",
			"ok 3",
		]
	);
	let b = format!(
		"{}.as_large_binary_array()?",
		chunk(
			&format!("{STRS}.cast(polars::DataType::Binary())?"),
			"oldest"
		)
	);
	assert_eq!(
		outcomes(
			&b,
			&[
				"a.value(2).map(|v| v.len())",
				"a.value(3).map(|v| v.len())",
				"a.values().map(|v| v.len())",
			]
		),
		["ok 3", "err OutOfBounds", "ok 4"]
	);
	// the newest level gives view arrays, read by the support readers
	assert_eq!(
		outcomes(
			&chunk(
				&format!("{STRS}.cast(polars::DataType::Binary())?"),
				"newest"
			),
			&[
				"a.values_binary().map(|v| v.len())",
				"a.as_large_binary_array().map(|x| 0)"
			]
		),
		["ok 3", "err ConversionError"]
	);
}

#[test]
fn list_children_are_owned_arrays() {
	let l = format!(
		"{}.as_large_list_array()?",
		chunk(
			&format!("{I64S}.implode()?.into_column().as_materialized_series()"),
			"newest"
		)
	);
	assert_eq!(
		outcomes(
			&l,
			&[
				"a.len()",
				"a.value(0).and_then(|c| c.as_int64_array()).and_then(|c| c.value(2))",
				"a.value(1).map(|c| 0)",
				"a.offsets().map(|v| `${v[0]},${v[1]}`)",
			]
		),
		["ok 1", "ok 3", "err OutOfBounds", "ok 0,3"]
	);
}

#[test]
fn copies_are_bounded_by_bytes_not_only_elements() {
	// two strings, four payload bytes: an element-count bound of 3 would
	// admit the copy; the byte buffer is copied element by element, so the
	// bound counts bytes and refuses it before any copy
	let s = format!("{}.as_large_string_array()?", chunk(STRS, "oldest"));
	let v = run(&format!(
		"pub fn main() {{ let a = {s}; polars::set_materialize_limit(3); let r = a.values().map(|v| v.len()); let o = a.offsets().map(|v| v.len()); polars::set_materialize_limit(0); Ok((match r {{ Ok(_) => \"accepted\", Err(e) => e.kind() }}, match o {{ Ok(_) => \"accepted\", Err(e) => e.kind() }})) }}"
	))
	.unwrap();
	let (bytes, offsets) = rune::from_value::<Result<(String, String), rune::Value>>(v)
		.unwrap()
		.unwrap();
	assert_eq!(bytes, "MaterializeLimit");
	// four offsets exceed three too
	assert_eq!(offsets, "MaterializeLimit");
}

#[test]
fn every_native_value_and_validity() {
	let mut natives = vec![
		("i8", "as_int8_array"),
		("i16", "as_int16_array"),
		("i32", "as_int32_array"),
		("i64", "as_int64_array"),
		("u8", "as_uint8_array"),
		("u16", "as_uint16_array"),
		("u32", "as_uint32_array"),
		("u64", "as_uint64_array"),
		("f32", "as_float32_array"),
		("f64", "as_float64_array"),
	];
	let mut casts = vec![];
	if v2() {
		casts = vec![
			("Int128", "as_int128_array"),
			("UInt128", "as_uint128_array"),
			("Float16", "as_float16_array"),
		];
	}
	let check = |series: String, downcast: &str| {
		let a = format!("{}.{downcast}()?", chunk(&series, "newest"));
		assert_eq!(
			outcomes(
				&a,
				&[
					"a.value(0)",
					"a.value(2)",
					"a.validity().map(|v| match v { Some(b) => `${b[0]},${b[1]},${b[2]}`, None => \"none\" })",
					"a.value(3)",
				]
			)
			.into_iter()
			.map(|s| s.replace(".0", ""))
			.collect::<Vec<_>>(),
			["ok 1", "ok 3", "ok true,false,true", "err OutOfBounds"],
			"{downcast}"
		);
	};
	for (n, d) in natives.drain(..) {
		let lit = if n.starts_with('f') {
			"1.0), None, Some(3.0"
		} else {
			"1), None, Some(3"
		};
		check(
			format!("polars::Series::from_iter_option_{n}([Some({lit})])?"),
			d,
		);
	}
	for (dt, d) in casts {
		check(format!("{I64S}.cast(polars::DataType::{dt}())?"), d);
	}
}

#[test]
fn wide_integers_are_checked_never_wrapped() {
	// the values come from strict string casts, exact at every width
	let wide = |dtype: &str, downcast: &str, vals: &[&str]| -> Vec<String> {
		let lits: Vec<String> = vals.iter().map(|v| format!("Some(\"{v}\")")).collect();
		let a = format!(
			"{}.{downcast}()?",
			chunk(
				&format!(
					"polars::Series::from_iter_option_str([{}])?.strict_cast(polars::DataType::{dtype}())?",
					lits.join(", ")
				),
				"newest"
			)
		);
		let calls: Vec<String> = (0..vals.len())
			.map(|i| format!("a.value({i})"))
			.chain(std::iter::once("a.values().map(|v| v.len())".into()))
			.collect();
		let refs: Vec<&str> = calls.iter().map(|s| s.as_str()).collect();
		outcomes(&a, &refs)
	};
	assert_eq!(
		wide(
			"UInt64",
			"as_uint64_array",
			&["9223372036854775807", "9223372036854775808", "1"]
		),
		[
			"ok 9223372036854775807",
			"err ConversionError",
			"ok 1",
			"err ConversionError"
		],
		"u64: one out-of-range element fails the whole vector"
	);
	if v2() {
		assert_eq!(
			wide(
				"Int128",
				"as_int128_array",
				&[
					"9223372036854775807",
					"9223372036854775808",
					"-9223372036854775809",
					"-9223372036854775808"
				]
			),
			[
				"ok 9223372036854775807",
				"err ConversionError",
				"err ConversionError",
				"ok -9223372036854775808",
				"err ConversionError"
			]
		);
		assert_eq!(
			wide(
				"UInt128",
				"as_uint128_array",
				&["340282366920938463463374607431768211455", "5"]
			),
			["err ConversionError", "ok 5", "err ConversionError"]
		);
		let ok = wide("Int128", "as_int128_array", &["1", "-2"]);
		assert_eq!(ok, ["ok 1", "ok -2", "ok 2"]);
	}
}

#[test]
fn floats_keep_special_values() {
	// each special value, read back and compared with Rust's own conversion
	let series = "polars::Series::from_iter_option_f64([Some(0.0), Some(-0.0), Some(1.0 / 0.0), Some(-1.0 / 0.0), Some(0.0 / 0.0), Some(65504.0), Some(0.000000059604644775390625), None])?";
	let reads = [
		"a.value(0).map(|v| `${v}`)",
		"a.value(1).map(|v| `${v}`)",
		"a.value(2).map(|v| `${v}`)",
		"a.value(3).map(|v| `${v}`)",
		"a.value(4).map(|v| `${v}`)",
		"a.value(5).map(|v| `${v}`)",
		"a.value(6).map(|v| `${v}`)",
		"a.validity().map(|v| match v { Some(b) => `${b[7]}`, None => \"none\" })",
		"a.values().map(|v| `${v[1]}`)",
	];
	let expect = |vals: [f64; 7]| -> Vec<String> {
		let mut out: Vec<String> = vals.iter().map(|v| format!("ok {v:?}")).collect();
		out.push("ok false".into());
		out.push(format!("ok {:?}", vals[1]));
		out
	};
	let rust = [
		0.0f64,
		-0.0,
		f64::INFINITY,
		f64::NEG_INFINITY,
		f64::NAN,
		65504.0,
		2f64.powi(-24),
	];
	let f64s = outcomes(
		&format!("{}.as_float64_array()?", chunk(series, "newest")),
		&reads,
	);
	assert_eq!(f64s, expect(rust));
	let f32s = outcomes(
		&format!(
			"{}.as_float32_array()?",
			chunk(
				&format!("{series}.cast(polars::DataType::Float32())?"),
				"newest"
			)
		),
		&reads,
	);
	assert_eq!(f32s, expect(rust.map(|v| v as f32 as f64)));
	if v2() {
		let f16s = outcomes(
			&format!(
				"{}.as_float16_array()?",
				chunk(
					&format!("{series}.cast(polars::DataType::Float16())?"),
					"newest"
				)
			),
			&reads,
		);
		// binary16 holds every one of these exactly
		assert_eq!(f16s, expect(rust));
	}
}

#[test]
fn struct_fields_are_owned_arrays() {
	// record 0125: at 0.55.2 too, now that `json` resolves `dtype-struct`
	// (record 0120 found no StructArray inventory row at 0.55.2)
	assert!(
		rnx_polars::generated::catalogue::CATALOGUE
			.iter()
			.any(|(p, _)| *p == "polars::arrow::ArrayRef::as_struct_array")
	);
	let s = format!(
		"{}.as_struct_array()?",
		chunk(
			&format!(
				"polars::StructChunked::from_columns(\"s\", 3, [polars::Column::from_series({I64S}), polars::Column::from_series({STRS}.with_name(\"y\"))])?.into_column().as_materialized_series()"
			),
			"newest"
		)
	);
	assert_eq!(
		outcomes(
			&s,
			&[
				"a.len()",
				"a.values().map(|v| v.len())",
				"a.values().and_then(|v| v[1].values_str()).map(|v| v.len())",
				"a.sliced(1, 3).map(|x| 0)",
			]
		),
		["ok 3", "ok 2", "ok 3", "err OutOfBounds"]
	);
}

#[test]
fn a_failing_element_names_its_index_and_value() {
	// review of 0120: the whole vector fails, naming the element and its value
	let a = format!(
		"{}.as_uint64_array()?",
		chunk(
			"polars::Series::from_iter_option_str([Some(\"1\"), Some(\"9223372036854775808\"), Some(\"3\")])?.strict_cast(polars::DataType::UInt64())?",
			"newest"
		)
	);
	let v = run(&format!(
		"pub fn main() {{ let a = {a}; match a.values() {{ Ok(_) => Ok(\"accepted\"), Err(e) => Ok(e.message()) }} }}"
	))
	.unwrap();
	let msg = rune::from_value::<Result<String, rune::Value>>(v)
		.unwrap()
		.unwrap();
	assert!(
		msg.contains("9223372036854775808") && msg.contains("(element 1)"),
		"{msg}"
	);
}

#[test]
fn constructors_and_dtype_arguments_are_not_bound() {
	// review of 0120: a dtype argument or a static length is the write side
	// (record 0121); none of these is a script binding at either pin
	let has = |k: &str| {
		rnx_polars::generated::catalogue::CATALOGUE
			.iter()
			.any(|(p, _)| *p == k)
	};
	for owner in [
		"Int64Array",
		"LargeStringArray",
		"LargeBinaryArray",
		"LargeListArray",
		"StructArray",
	] {
		for m in [
			"new_null",
			"new_empty",
			"full_null",
			"to",
			"get_child_type",
			"get_child_field",
			"get_fields",
		] {
			assert!(
				!has(&format!("polars::arrow::{owner}::{m}")),
				"{owner}::{m} is bound"
			);
		}
	}
	// the read side and the dtype-free constructors stay
	assert!(has("polars::arrow::Int64Array::value") && has("polars::arrow::Int64Array::from_vec"));
}
