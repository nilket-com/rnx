//! Record 0126: the eager group-by, through a rebuilt borrow. A script
//! `GroupBy` owns a snapshot of its frame, the key columns, the groups
//! (computed once) and the selection; each call rebuilds Polars' borrowed
//! `GroupBy` for that call only. Every result equals Rust's on a real
//! borrowed `GroupBy` over the same frame: exactly for `group_by_stable`, as
//! a multiset of rows for the hash-ordered `group_by`. The frame stays valid
//! whatever the script does to its own binding afterwards (drop, replace,
//! mutate in place), and `sliced` is refused.
#![cfg(all(feature = "generated", feature = "test-support"))]
use polars::prelude::*;
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

/// The script's `Ok(Vec<String>)`, or its error text.
fn texts(script: &str) -> Vec<String> {
	let v = run(script).unwrap();
	match rune::from_value::<Result<Vec<String>, rune::Value>>(v).unwrap() {
		Ok(v) => v,
		Err(e) => panic!("script error: {e:?}"),
	}
}

const CSV: &str = "region,product,qty,price\nnorth,apple,3,1.25\nsouth,pear,,2.5\nnorth,pear,5,2.5\n,apple,2,1.25\nsouth,apple,7,\nnorth,plum,1,3.1\n,pear,4,2.5\nsouth,plum,,3.1\n";
const SCHEMA: &str =
	r#"[("region", "string"), ("product", "string"), ("qty", "i64"), ("price", "f64")]"#;

fn fixture(name: &str, body: &str) -> (String, DataFrame) {
	let d = std::env::temp_dir().join(format!("rnx-0126-{}", std::process::id()));
	std::fs::create_dir_all(&d).unwrap();
	let p = d.join(name);
	std::fs::write(&p, body).unwrap();
	let schema = Schema::from_iter([
		Field::new("region".into(), DataType::String),
		Field::new("product".into(), DataType::String),
		Field::new("qty".into(), DataType::Int64),
		Field::new("price".into(), DataType::Float64),
	]);
	let df = CsvReadOptions::default()
		.with_has_header(true)
		.with_schema(Some(Arc::new(schema)))
		.try_into_reader_with_file_path(Some(p.clone()))
		.unwrap()
		.finish()
		.unwrap();
	(p.to_str().unwrap().to_owned(), df)
}

fn repr(df: PolarsResult<DataFrame>) -> String {
	frame_repr(&df.unwrap()).to_text()
}

/// A frame's text with its rows as a sorted multiset (the hash-ordered
/// `group_by` promises no row order).
fn unordered(text: &str) -> String {
	match text.split_once("rows=[") {
		Some((head, rows)) => {
			let rows = rows.trim_end_matches(']');
			let mut rs: Vec<&str> = rows.split(" | ").collect();
			rs.sort();
			format!("{head}rows=[{}]", rs.join(" | "))
		}
		None => text.to_owned(),
	}
}

#[test]
fn every_operation_equals_rust_on_a_real_borrowed_group_by() {
	let (path, df) = fixture("sales.csv", CSV);
	// the script: one stable GroupBy, every generated operation
	let got = texts(&format!(
		"pub fn main() {{ let df = polars::read_csv({path:?}, {SCHEMA})?; let gb = df.group_by_stable([\"region\"])?; \
		   let keys = polars::DataFrame::new_infer_height(gb.keys())?; \
		   let s1 = polars::DataFrame::new_infer_height(gb.keys_sliced(Some((1, 1)))?)?; \
		   let s2 = polars::DataFrame::new_infer_height(gb.keys_sliced(Some((-100, 999999)))?)?; \
		   let s3 = polars::DataFrame::new_infer_height(gb.keys_sliced(Some((5, 2)))?)?; \
		   let s4 = polars::DataFrame::new_infer_height(gb.keys_sliced(Some((9223372036854775807, 9223372036854775807)))?)?; \
		   let s5 = polars::DataFrame::new_infer_height(gb.keys_sliced(Some((-9223372036854775807, 3)))?)?; \
		   Ok([polars::oracle_repr(gb.count()?)?, polars::oracle_repr(gb.groups()?)?, polars::oracle_repr(keys)?, \
		       polars::oracle_repr(s1)?, polars::oracle_repr(s2)?, polars::oracle_repr(s3)?, polars::oracle_repr(s4)?, polars::oracle_repr(s5)?, \
		       polars::oracle_repr(gb.select_([\"qty\"])?.count()?)?, format!(\"{{:?}}\", gb.get_groups()), format!(\"{{:?}}\", gb.into_groups())]) }}"
	));
	// Rust: the same operations on a real borrowed GroupBy
	let gb = df.group_by_stable(["region"]).unwrap();
	let keys = |s| repr(DataFrame::new_infer_height(gb.keys_sliced(s)));
	let want = vec![
		repr(gb.count()),
		repr(gb.groups()),
		repr(DataFrame::new_infer_height(gb.keys())),
		keys(Some((1, 1))),
		keys(Some((-100, 999999))),
		keys(Some((5, 2))),
		keys(Some((i64::MAX, i64::MAX as usize))),
		keys(Some((-i64::MAX, 3))),
		repr(gb.clone().select(["qty"]).count()),
		format!("{:?}", gb.get_groups()),
		format!("{:?}", gb.clone().into_groups()),
	];
	assert_eq!(got, want);
}

#[test]
fn hash_ordered_multi_key_null_and_empty_frames_equal_rust() {
	let (path, df) = fixture("sales2.csv", CSV);
	let (empty_path, empty) = fixture("empty.csv", "region,product,qty,price\n");
	let got = texts(&format!(
		"pub fn main() {{ let df = polars::read_csv({path:?}, {SCHEMA})?; let e = polars::read_csv({empty_path:?}, {SCHEMA})?; \
		   Ok([polars::oracle_repr(df.group_by([\"region\"])?.count()?)?, \
		       polars::oracle_repr(df.group_by_stable([\"region\", \"product\"])?.count()?)?, \
		       polars::oracle_repr(df.group_by([\"region\", \"product\"])?.groups()?)?, \
		       polars::oracle_repr(e.group_by_stable([\"region\"])?.count()?)?, \
		       polars::oracle_repr(polars::DataFrame::new_infer_height(e.group_by([\"region\"])?.keys())?)?]) }}"
	));
	let want = vec![
		repr(df.group_by(["region"]).unwrap().count()),
		repr(df.group_by_stable(["region", "product"]).unwrap().count()),
		repr(df.group_by(["region", "product"]).unwrap().groups()),
		repr(empty.group_by_stable(["region"]).unwrap().count()),
		repr(DataFrame::new_infer_height(
			empty.group_by(["region"]).unwrap().keys(),
		)),
	];
	// the null region is a group of its own, as in Rust
	assert!(want[1].contains("Null"), "{}", want[1]);
	for (i, (g, w)) in got.iter().zip(&want).enumerate() {
		if i == 1 || i == 3 {
			assert_eq!(g, w, "case {i}");
		} else {
			assert_eq!(unordered(g), unordered(w), "case {i}");
		}
	}
}

#[test]
fn one_grouping_serves_every_call_of_one_group_by() {
	let (path, _) = fixture("sales3.csv", CSV);
	// the hash-ordered group_by: keys() and two counts are row-aligned
	let got = texts(&format!(
		"pub fn main() {{ let df = polars::read_csv({path:?}, {SCHEMA})?; let out = []; \
		   for i in 0..20 {{ let gb = df.group_by([\"region\", \"product\"])?; \
		     let a = polars::oracle_repr(gb.count()?.select_([\"region\", \"product\"])?)?; \
		     let k = polars::oracle_repr(polars::DataFrame::new_infer_height(gb.keys())?)?; \
		     let b = polars::oracle_repr(gb.count()?.select_([\"region\", \"product\"])?)?; \
		     out.push(`${{a == k && a == b}}`); }} Ok(out) }}"
	));
	assert!(got.iter().all(|x| x == "true"), "{got:?}");
}

#[test]
fn the_frame_stays_valid_whatever_the_script_does_to_its_binding() {
	let (path, df) = fixture("sales4.csv", CSV);
	let (other, _) = fixture("other.csv", "region,product,qty,price\neast,fig,1,1.0\n");
	let want = repr(df.group_by_stable(["region"]).unwrap().count());
	let got = texts(&format!(
		"pub fn main() {{ let df = polars::read_csv({path:?}, {SCHEMA})?; let by = [\"region\"]; \
		   let gb = df.group_by_stable(by)?; \
		   df.drop_in_place(\"qty\")?; df.rename(\"region\", \"r\")?; \
		   let mutated = polars::oracle_repr(gb.count()?)?; \
		   let df = polars::read_csv({other:?}, {SCHEMA})?; \
		   let replaced = polars::oracle_repr(gb.count()?)?; \
		   let gb2 = {{ let tmp = polars::read_csv({path:?}, {SCHEMA})?; tmp.group_by_stable(by)? }}; \
		   let dropped = polars::oracle_repr(gb2.count()?)?; \
		   Ok([mutated, replaced, dropped, `${{by.len()}}`, `${{df.height()?}}`]) }}"
	));
	assert_eq!(
		got,
		vec![want.clone(), want.clone(), want, "1".into(), "1".into()]
	);
}

#[test]
fn an_invalid_selection_is_polars_error_and_the_group_by_stays_usable() {
	let (path, df) = fixture("sales5.csv", CSV);
	let want = repr(df.group_by_stable(["region"]).unwrap().count());
	let got = texts(&format!(
		"pub fn main() {{ let df = polars::read_csv({path:?}, {SCHEMA})?; let gb = df.group_by_stable([\"region\"])?; \
		   let sel = [\"nope\"]; let bad = gb.select_(sel)?.count(); \
		   Ok([`${{bad.is_err()}}`, polars::oracle_repr(gb.count()?)?, `${{sel.len()}}`]) }}"
	));
	assert_eq!(got, vec!["true".to_string(), want, "1".into()]);
	// Rust refuses the same selection
	assert!(
		df.group_by_stable(["region"])
			.unwrap()
			.select(["nope"])
			.count()
			.is_err()
	);
}

#[test]
fn debug_text_is_polars_own() {
	let (path, df) = fixture("sales6.csv", CSV);
	let got = texts(&format!(
		"pub fn main() {{ let df = polars::read_csv({path:?}, {SCHEMA})?; let gb = df.group_by_stable([\"region\"])?; Ok([format!(\"{{:?}}\", gb)]) }}"
	));
	assert_eq!(
		got,
		vec![format!("{:?}", df.group_by_stable(["region"]).unwrap())]
	);
}

#[test]
fn sliced_new_and_script_keys_are_refused() {
	let catalogue = |p: &str| {
		rnx_polars::generated::catalogue::CATALOGUE
			.iter()
			.any(|(k, _)| *k == p)
	};
	assert!(catalogue("polars::GroupBy::count") && catalogue("polars::DataFrame::group_by"));
	for p in [
		"polars::GroupBy::sliced",
		"polars::GroupBy::new",
		"polars::DataFrame::group_by_with_series",
	] {
		assert!(!catalogue(p), "{p} must stay refused");
	}
}
