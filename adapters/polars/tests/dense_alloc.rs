//! Record 0129: `to_dense` refuses a bad column list or a non-numeric column
//! before the row-major buffer exists, observed through the allocator's
//! peak. One test, so no parallel test moves the peak.
use rnx::allocation::{peak, reset_peak};
use rnx::rune::{self, Context, Module, Source, Sources, Value, Vm};
use std::sync::Arc;

const ROWS: usize = 1 << 18;
/// A refusal's whole footprint stays far under one f64 column (2 MiB).
const SMALL: usize = 256 << 10;

fn vm() -> Vm {
	let mut polars = Module::with_crate("polars").unwrap();
	rnx_polars::build(&mut polars).unwrap();
	let mut context = Context::with_default_modules().unwrap();
	context
		.install(rnx::interchange::module().unwrap())
		.unwrap();
	context.install(polars).unwrap();
	let runtime = Arc::new(context.runtime().unwrap());
	let mut sources = Sources::new();
	let script = r#"
		pub fn load(path) { polars::read_csv(path, [("a", "i64"), ("s", "string")]) }
		pub fn to_dense(df, cols, dtype) { df.to_dense(cols, dtype) }
	"#;
	sources.insert(Source::memory(script).unwrap()).unwrap();
	let unit = rune::prepare(&mut sources)
		.with_context(&context)
		.build()
		.unwrap();
	Vm::new(runtime, Arc::new(unit))
}

fn result(v: Value) -> Result<Value, String> {
	rune::from_value::<Result<Value, Value>>(v)
		.unwrap()
		.map_err(|e| rune::from_value::<String>(e).unwrap())
}

/// The call's peak above the live bytes when it started, and its outcome.
fn observe(vm: &mut Vm, df: &Value, cols: Value, dtype: &str) -> (usize, Result<Value, String>) {
	reset_peak();
	let base = peak();
	let out = result(vm.call(["to_dense"], (df.clone(), cols, dtype)).unwrap());
	(peak() - base, out)
}

#[test]
fn refusals_come_before_the_buffer() {
	let dir = std::env::temp_dir().join(format!("rnx-0129-alloc-{}", std::process::id()));
	std::fs::create_dir_all(&dir).unwrap();
	let path = dir.join("f.csv");
	let mut csv = String::from("a,s\n");
	for i in 0..ROWS {
		csv.push_str(&format!("{i},x\n"));
	}
	std::fs::write(&path, csv).unwrap();
	let mut vm = vm();
	let df = result(vm.call(["load"], (path.to_str().unwrap(),)).unwrap()).unwrap();
	let list = |v: Vec<String>| rune::to_value(v).unwrap();

	// the control: an accepted export is visible (one f64 column, 2 MiB)
	let (grew, out) = observe(&mut vm, &df, list(vec!["a".into()]), "f64");
	assert!(out.is_ok());
	assert!(grew >= ROWS * 8, "the observation sees the buffer: {grew}");

	// a string column is refused before the 4 MiB buffer for two columns
	let (grew, out) = observe(&mut vm, &df, list(vec!["a".into(), "s".into()]), "f64");
	assert!(out.unwrap_err().contains("\"s\" is str, not numeric"));
	assert!(grew < SMALL, "dtype refusal allocated {grew}");

	// an 8 MiB name is refused before it is copied
	let (grew, out) = observe(
		&mut vm,
		&df,
		list(vec!["a".into(), "n".repeat(8 << 20)]),
		"f64",
	);
	assert!(out.unwrap_err().contains("1 to 256 bytes"));
	assert!(grew < SMALL, "long-name refusal allocated {grew}");

	// a 100,000-entry list is refused from its length, before any entry
	let (grew, out) = observe(&mut vm, &df, list(vec!["a".into(); 100_000]), "f64");
	assert!(out.unwrap_err().contains("100000 columns, at most 4096"));
	assert!(grew < SMALL, "long-list refusal allocated {grew}");

	let _ = std::fs::remove_dir_all(&dir);
}
