//! Record 0127: `LazyFrame::pivot` at v2 (the shipped 0.55.2 build has no
//! Polars `pivot` feature, so this runs as a v2 adapter test, copied in by
//! `probes/0127/replay.sh`). The script's pivot equals Rust's
//! `pivot(.., Arc::new(on_columns), ..)` over the same frames; the script's
//! `on_columns` frame and receiver stay usable, the plan keeps its own
//! snapshot, Polars' own validation refuses bad `on_columns` as errors at
//! the same point as in Rust, and the raw shim refuses wrong counts.
#![cfg(all(feature = "generated", feature = "test-support"))]
use polars::prelude::*;
use polars_core::frame::PivotColumnNaming;
use rnx::rune::{self, Context, Module, Source, Sources, Vm};
use rnx_polars::oracle::frame_repr;
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

fn texts(script: &str) -> Vec<String> {
	let v = run(script).unwrap();
	match rune::from_value::<Result<Vec<String>, rune::Value>>(v).unwrap() {
		Ok(v) => v,
		Err(e) => panic!("script error: {e:?}"),
	}
}

const CSV: &str = "region,product,size,qty,price\nnorth,apple,s,3,1.0\nsouth,pear,l,2,2.0\nnorth,pear,s,5,2.5\neast,apple,l,2,1.25\nsouth,apple,s,7,1.5\nnorth,plum,l,1,3.0\n";

fn fixture() -> (String, DataFrame) {
	// review of 0127: a unique file per call, so parallel tests never share one
	static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
	let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
	let d = std::env::temp_dir().join(format!("rnx-0127-pivot-{}", std::process::id()));
	std::fs::create_dir_all(&d).unwrap();
	let p = d.join(format!("sales-{n}.csv"));
	std::fs::write(&p, CSV).unwrap();
	let df = CsvReadOptions::default()
		.with_has_header(true)
		.try_into_reader_with_file_path(Some(p.clone()))
		.unwrap()
		.finish()
		.unwrap();
	(p.to_str().unwrap().to_owned(), df)
}

fn repr(df: PolarsResult<DataFrame>) -> String {
	frame_repr(&df.unwrap()).to_text()
}

/// Rust's pivot over the same frames.
fn rust_pivot(df: &DataFrame, on: &[&str], index: &[&str], values: &[&str], maintain_order: bool, sep: &str, naming: PivotColumnNaming) -> PolarsResult<DataFrame> {
	let on_columns = df.select(on.iter().copied())?.unique_stable(None, UniqueKeepStrategy::First, None)?;
	df.clone()
		.lazy()
		.pivot(cols(on.iter().copied()), Arc::new(on_columns), cols(index.iter().copied()), cols(values.iter().copied()), element().sum(), maintain_order, sep.into(), naming)
		.collect()
}

fn script_pivot(path: &str, on: &[&str], index: &[&str], values: &[&str], maintain_order: bool, sep: &str, naming: &str) -> String {
	let list = |xs: &[&str]| format!("[{}]", xs.iter().map(|x| format!("{x:?}")).collect::<Vec<_>>().join(", "));
	format!(
		"let df = polars::read_csv({path:?})?; \
		 let on_columns = df.select_({on})?.unique_stable(None, polars::UniqueKeepStrategy::First(), None)?; \
		 df.lazy().pivot(polars::cols({on})?, on_columns, polars::cols({index})?, polars::cols({values})?, polars::element().sum(), {maintain_order}, {sep:?}, polars::PivotColumnNaming::{naming}()).collect()?",
		on = list(on),
		index = list(index),
		values = list(values),
	)
}

#[test]
fn pivot_equals_rust_across_its_options() {
	let (path, df) = fixture();
	let cases: Vec<(&[&str], &[&str], &[&str], bool, &str, &str, PivotColumnNaming)> = vec![
		(&["product"], &["region"], &["qty"], true, "_", "Auto", PivotColumnNaming::Auto),
		(&["product"], &["region"], &["qty"], false, "_", "Auto", PivotColumnNaming::Auto),
		(&["product"], &["region"], &["qty"], true, "_", "Combine", PivotColumnNaming::Combine),
		(&["product"], &["region"], &["qty", "price"], true, "-", "Auto", PivotColumnNaming::Auto),
		(&["product", "size"], &["region"], &["qty"], true, "_", "Auto", PivotColumnNaming::Auto),
	];
	for (on, index, values, mo, sep, naming, rnaming) in cases {
		let got = texts(&format!(
			"pub fn main() {{ let r = {{ {} }}; Ok([polars::oracle_repr(r)?]) }}",
			script_pivot(&path, on, index, values, mo, sep, naming)
		));
		let want = repr(rust_pivot(&df, on, index, values, mo, sep, rnaming));
		if mo {
			assert_eq!(got, vec![want], "{on:?} {values:?} {naming}");
		} else {
			// without maintain_order the index rows' order is unspecified
			let sorted = |t: &str| {
				let (h, rows) = t.split_once("rows=[").unwrap();
				let mut r: Vec<&str> = rows.trim_end_matches(']').split(" | ").collect();
				r.sort();
				format!("{h}rows=[{}]", r.join(" | "))
			};
			assert_eq!(sorted(&got[0]), sorted(&want), "{on:?} unordered");
		}
	}
}

#[test]
fn on_columns_and_the_receiver_stay_usable_and_the_plan_keeps_its_snapshot() {
	let (path, df) = fixture();
	let want = repr(rust_pivot(&df, &["product"], &["region"], &["qty"], true, "_", PivotColumnNaming::Auto));
	let got = texts(&format!(
		"pub fn main() {{ let df = polars::read_csv({path:?})?; \
		   let on_columns = df.select_([\"product\"])?.unique_stable(None, polars::UniqueKeepStrategy::First(), None)?; \
		   let lf = df.lazy(); \
		   let plan = lf.pivot(polars::cols([\"product\"])?, on_columns, polars::cols([\"region\"])?, polars::cols([\"qty\"])?, polars::element().sum(), true, \"_\", polars::PivotColumnNaming::Auto()); \
		   let on_height = on_columns.height()?; let lf_height = lf.collect()?.height()?; \
		   let on_columns = df.select_([\"region\"])?; \
		   Ok([`${{on_height}} ${{lf_height}}`, polars::oracle_repr(plan.collect()?)?]) }}"
	));
	assert_eq!(got, vec!["3 6".to_string(), want]);
}

#[test]
fn polars_refuses_bad_on_columns_as_errors_as_in_rust() {
	let (path, df) = fixture();
	let rust = |on: &[&str], on_cols: DataFrame, values: &[&str]| {
		df.clone()
			.lazy()
			.pivot(cols(on.iter().copied()), Arc::new(on_cols), cols(["region"]), cols(values.iter().copied()), element().sum(), true, "_".into(), PivotColumnNaming::Auto)
			.collect()
			.is_err()
	};
	let script = |on: &str, on_cols: &str, values: &str| {
		texts(&format!(
			"pub fn main() {{ let df = polars::read_csv({path:?})?; let oc = {on_cols}; \
			   let r = df.lazy().pivot(polars::cols({on})?, oc, polars::cols([\"region\"])?, polars::cols({values})?, polars::element().sum(), true, \"_\", polars::PivotColumnNaming::Auto()).collect(); \
			   Ok([`${{r.is_err()}}`]) }}"
		))[0]
			.clone()
	};
	let uniq = |c: &[&str]| df.select(c.iter().copied()).unwrap().unique_stable(None, UniqueKeepStrategy::First, None).unwrap();
	// a width mismatch, mismatched names (several on columns), empty values,
	// duplicate on values (a duplicate output name), and an empty on
	let cases: Vec<(&str, &str, &str, bool)> = vec![
		("[\"product\"]", "df.select_([\"product\", \"size\"])?.unique_stable(None, polars::UniqueKeepStrategy::First(), None)?", "[\"qty\"]", rust(&["product"], uniq(&["product", "size"]), &["qty"])),
		("[\"product\", \"size\"]", "df.select_([\"size\", \"product\"])?.unique_stable(None, polars::UniqueKeepStrategy::First(), None)?", "[\"qty\"]", rust(&["product", "size"], uniq(&["size", "product"]), &["qty"])),
		("[\"product\"]", "df.select_([\"product\"])?.unique_stable(None, polars::UniqueKeepStrategy::First(), None)?", "[]", rust(&["product"], uniq(&["product"]), &[])),
		("[\"product\"]", "df.select_([\"product\"])?", "[\"qty\"]", rust(&["product"], df.select(["product"]).unwrap(), &["qty"])),
		// review of 0127: an empty `on`
		("[]", "df.select_([\"product\"])?.unique_stable(None, polars::UniqueKeepStrategy::First(), None)?", "[\"qty\"]", rust(&[], uniq(&["product"]), &["qty"])),
	];
	for (on, oc, values, want) in cases {
		assert!(want, "Rust must refuse {on} {oc} {values}");
		assert_eq!(script(on, oc, values), want.to_string(), "{on} {oc} {values}");
	}
}

#[test]
fn the_shim_refuses_a_wrong_count() {
	let (path, _) = fixture();
	for (args, actual) in [("polars::cols([\"product\"])?, oc, polars::cols([\"region\"])?, polars::cols([\"qty\"])?, polars::element().sum(), true, \"_\"", 8), ("polars::cols([\"product\"])?, oc, polars::cols([\"region\"])?, polars::cols([\"qty\"])?, polars::element().sum(), true, \"_\", polars::PivotColumnNaming::Auto(), 1", 10)] {
		let e = run(&format!(
			"pub fn main() {{ let df = polars::read_csv({path:?})?; let oc = df.select_([\"product\"])?; df.lazy().pivot({args}) }}"
		))
		.unwrap_err();
		assert!(e.contains(&format!("Wrong number of arguments {actual}, expected 9")), "{e}");
	}
}
