//! Record 0073: eager frame work runs on the engine thread. Its own test
//! binary, because the engine counters are process-wide and the other
//! integration tests would race them.
#![cfg(feature = "test-support")]
use std::path::PathBuf;
use std::process::Command;

/// The engine counters are process-wide: tests in this binary that call
/// generated bindings in-process take this lock so they cannot race.
static ENGINE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Eager frame work runs on the engine thread, off the caller, including
/// when the caller sits inside an active async runtime.
#[test]
fn eager_frame_work_runs_on_the_engine_thread() {
	let _engine = ENGINE.lock().unwrap_or_else(|e| e.into_inner());
	use rnx::rune::{self, Context, Module, Source, Sources, Vm};
	use std::sync::Arc;
	let mut m = Module::with_crate("polars").unwrap();
	rnx_polars::build(&mut m).unwrap();
	let mut context = Context::with_default_modules().unwrap();
	context.install(m).unwrap();
	let runtime = Arc::new(context.runtime().unwrap());
	let run = |src: &str| -> (i64, i64) {
		let mut sources = Sources::new();
		sources.insert(Source::memory(src).unwrap()).unwrap();
		let unit = rune::prepare(&mut sources).with_context(&context).build().unwrap();
		let mut vm = Vm::new(runtime.clone(), Arc::new(unit));
		rune::from_value::<(i64, i64)>(vm.call(["main"], ()).unwrap()).unwrap()
	};
	// engine_counts() = (started, finished, joined, active, max_active, no_context)
	// the sort alone, isolated: exactly one engine thread
	let script = "pub fn main() { let d = polars::DataFrame::empty(); let opts = polars::SortMultipleOptions::default_(); let before = polars::engine_counts().0; let _ = d.sort_impl([], opts, None); let after = polars::engine_counts().0; (before, after) }";
	let (before, after) = run(script);
	assert_eq!(after - before, 1, "an eager sort must start exactly one engine thread (started {before} -> {after})");
	let script = "pub fn main() { let d = polars::DataFrame::empty(); let before = polars::engine_counts().0; let _ = d.height(); let after = polars::engine_counts().0; (before, after) }";
	let (before, after) = run(script);
	assert_eq!(after - before, 1, "a frame accessor must start exactly one engine thread (started {before} -> {after})");
	// inside a tokio runtime the engine thread still sees no runtime context
	let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
	let (before, after) = rt.block_on(async {
		let script = "pub fn main() { let d = polars::DataFrame::empty(); let before = polars::engine_counts().5; let _ = d.width(); let after = polars::engine_counts().5; (before, after) }";
		run(script)
	});
	assert_eq!(after - before, 1, "a frame call from inside a runtime must run on an engine thread with no runtime context");
}

/// A generated binding runs in a script: an associated constructor, an
/// instance method, and a Polars error surfaced as a `polars::Error`.
#[test]
fn generated_bindings_run() {
	let _engine = ENGINE.lock().unwrap_or_else(|e| e.into_inner());
	use rnx::rune::{self, Context, Module, Source, Sources, Vm};
	use std::sync::Arc;
	let t0 = std::time::Instant::now();
	let mut m = Module::with_crate("polars").unwrap();
	rnx_polars::build(&mut m).unwrap();
	let t1 = std::time::Instant::now();
	let mut context = Context::with_default_modules().unwrap();
	context.install(m).unwrap();
	let t2 = std::time::Instant::now();
	let runtime = Arc::new(context.runtime().unwrap());
	let t3 = std::time::Instant::now();
	eprintln!("module build {} µs, context install {} µs, runtime {} µs", (t1 - t0).as_micros(), (t2 - t1).as_micros(), (t3 - t2).as_micros());
	let run = |src: &str| -> String {
		let mut sources = Sources::new();
		sources.insert(Source::memory(src).unwrap()).unwrap();
		let unit = rune::prepare(&mut sources).with_context(&context).build().unwrap();
		let mut vm = Vm::new(runtime.clone(), Arc::new(unit));
		let out = vm.call(["main"], ()).unwrap();
		rune::from_value::<String>(out).unwrap()
	};
	assert_eq!(run("pub fn main() { let d = polars::DataFrame::empty(); `${d.height().unwrap()} ${d.width()}` }"), "0 0");
	let err = run("pub fn main() { let d = polars::DataFrame::empty(); match d.try_get_column_index(\"x\") { Ok(i) => `ok ${i}`, Err(e) => `${e.kind()}|${e}` } }");
	assert!(err.starts_with("ColumnNotFound|") && err.contains("\"x\""), "{err}");
}

/// The end-to-end script beside this file runs through the adapter binary:
/// generated constructors, a `select_` (renamed: `select` is a Rune
/// keyword) whose vector argument makes it fallible, a `collect` routed
/// through the engine thread, an option struct's setter and getter, a
/// Polars error's `kind()`, and a data-carrying enum constructor.
#[test]
#[cfg(feature = "test-support")]
fn end_to_end_script_runs() {
	let _engine = ENGINE.lock().unwrap_or_else(|e| e.into_inner());
	let out = Command::new(env!("CARGO_BIN_EXE_rnx-polars-fixture"))
		.arg("run")
		.arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/generated_e2e.rn"))
		.stdin(std::process::Stdio::null())
		.output()
		.expect("run the fixture binary");
	let text = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
	assert_eq!(
		text.trim(),
		"height 0 width 0\nexpr col(\"x\").alias(\"y\")\ncollected 1 rows, 1 cols\nopts true\nerror kind ColumnNotFound\ndtype datetime[ms]",
		"{text}"
	);
}
