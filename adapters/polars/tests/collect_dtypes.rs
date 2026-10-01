//! Record 0122: a collected frame keeps the dtypes its query produced. A
//! group count is UInt32 and a parsed date is Date, as Polars returns them;
//! record 0058's collect refused both. The hand-written `preview` still
//! refuses a dtype it cannot show, and the hand-written readers keep their
//! four-dtype schema contract.
#![cfg(feature = "generated")]
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

fn sales() -> std::path::PathBuf {
	// record 0127 (review): a unique file per call, so parallel tests never share one
	static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
	let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
	let dir = std::env::temp_dir().join(format!("rnx-0122-collect-{}", std::process::id()));
	std::fs::create_dir_all(&dir).unwrap();
	let p = dir.join(format!("sales-{n}.csv"));
	std::fs::write(&p, "region,qty\nnorth,3\nsouth,\nnorth,5\n").unwrap();
	p
}

#[test]
fn a_count_collects_as_uint32_and_previews() {
	let path = sales();
	let v = run(&format!(
		"pub fn main() {{ let df = polars::read_csv({:?}, [(\"region\", \"string\"), (\"qty\", \"i64\")])?; \
		 let out = df.lazy().group_by([polars::col(\"region\")])?.agg([polars::col(\"qty\").count().alias(\"n\")])?.sort([\"region\"])?.collect()?; \
		 let shown = match out.preview() {{ Ok(t) => t, Err(e) => \"refused\" }}; \
		 let d = out.dtypes(); Ok((`${{d[1]}}`, out.height()?, shown)) }}",
		path.to_str().unwrap()
	))
	.unwrap();
	let got = rune::from_value::<Result<(String, i64, String), rune::Value>>(v)
		.unwrap()
		.unwrap_or_else(|_| panic!("the query failed"));
	assert!(got.0.contains("UInt32") || got.0.contains("u32"), "{got:?}");
	// record 0124: the preview shows every dtype (0122 found it refused)
	assert_eq!(got.1, 2);
	assert!(got.2.contains("\"n\": u32"), "{got:?}");
}

#[test]
fn a_query_with_only_the_four_dtypes_is_unchanged() {
	let path = sales();
	let v = run(&format!(
		"pub fn main() {{ let df = polars::read_csv({:?}, [(\"region\", \"string\"), (\"qty\", \"i64\")])?; \
		 let out = df.lazy().group_by([polars::col(\"region\")])?.agg([polars::col(\"qty\").sum()])?.sort([\"region\"])?.collect()?; \
		 out.preview() }}",
		path.to_str().unwrap()
	))
	.unwrap();
	let text = rune::from_value::<Result<String, rune::Value>>(v)
		.unwrap()
		.unwrap_or_else(|_| panic!("preview failed"));
	assert!(text.contains("north") && text.contains('8'), "{text}");
}
