//! Record 0127: the wide-binding shim against Rune's typed calling
//! convention, at an arity both can take. One function (`Result`) and its
//! `VmResult` twin are registered twice: typed (`#[rune::function]`-style
//! `m.function(..)`) and through a raw shim the generator's own emitter
//! produced. The same scripts call both and compare: a success, each
//! error, a wrong type in each slot, a wrong count, borrowed-argument
//! reuse, owned-argument consumption, and a borrow conflict. The only
//! permitted difference is the typed path's "Bad argument #n" layer, which
//! rune 0.14.2 attaches through a crate-private API; both texts are shown.
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

/// A script's `Ok(String)`, its `Err` as `script error`, or the VM error.
fn outcome(script: &str) -> String {
	match run(script) {
		Ok(v) => match rune::from_value::<Result<String, rune::Value>>(v) {
			Ok(Ok(s)) => format!("ok {s}"),
			Ok(Err(_)) => "script error".into(),
			Err(e) => format!("unexpected {e}"),
		},
		Err(e) => format!("vm error {e}"),
	}
}

fn frame() -> String {
	// review of 0127: a unique file per call, so parallel tests never share one
	static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
	let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
	let d = std::env::temp_dir().join(format!("rnx-0127-{}", std::process::id()));
	std::fs::create_dir_all(&d).unwrap();
	let p = d.join(format!("a-{n}.csv"));
	std::fs::write(&p, "a,b\n1,2\n3,4\n5,6\n").unwrap();
	format!("polars::read_csv({:?})?", p.to_str().unwrap())
}

/// The same body, with the method under its typed and its raw name.
fn both(body: &str) -> (String, String) {
	let f = frame();
	let script = |m: &str| {
		format!(
			"pub fn main() {{ let df = {f}; let other = {f}; {} }}",
			body.replace("METHOD", m)
		)
	};
	(
		outcome(&script("wide_control_typed")),
		outcome(&script("wide_control_raw")),
	)
}

fn both_vm(body: &str) -> (String, String) {
	let f = frame();
	let script = |m: &str| {
		format!(
			"pub fn main() {{ let df = {f}; let other = {f}; {} }}",
			body.replace("METHOD", m)
		)
	};
	(
		outcome(&script("wide_control_vm_typed")),
		outcome(&script("wide_control_vm_raw")),
	)
}

#[test]
fn success_and_both_error_returns_agree() {
	// Result: Ok, and Polars' error (a missing column) as a script error
	assert_eq!(
		both("Ok(`${df.METHOD(\"a\", 2, other)?}`)"),
		("ok 5".into(), "ok 5".into())
	);
	let (t, r) = both("let x = df.METHOD(\"nope\", 2, other); Ok(`${x.is_err()}`)");
	assert_eq!((t.as_str(), r.as_str()), ("ok true", "ok true"));
	// VmResult: Ok, and a VM error that stays a VM error
	assert_eq!(
		both_vm("Ok(`${df.METHOD(\"a\", 2, other)}`)"),
		("ok 6".into(), "ok 6".into())
	);
	let (t, r) = both_vm("Ok(`${df.METHOD(\"a\", -1, other)}`)");
	assert!(
		t.starts_with("vm error") && t.contains("wide control: negative"),
		"{t}"
	);
	assert_eq!(t, r);
}

#[test]
fn a_wrong_type_in_each_slot_is_the_same_error_less_rune_s_index_layer() {
	for (slot, call) in [
		// the receiver, through the qualified form (review of 0127)
		(0, "polars::DataFrame::METHOD(5, \"a\", 2, other)"),
		(1, "df.METHOD(5, 2, other)"),
		(2, "df.METHOD(\"a\", \"x\", other)"),
		(3, "df.METHOD(\"a\", 2, 7)"),
	] {
		let (t, r) = both(&format!("Ok(`${{{call}?}}`)"));
		eprintln!("slot {slot}: typed: {t}\n        raw:   {r}");
		assert!(
			t.starts_with("vm error") && r.starts_with("vm error"),
			"{t} / {r}"
		);
		// rune 0.14.2 renders the typed error without its index layer, so the
		// two texts are identical
		assert_eq!(t, r, "slot {slot}");
	}
}

#[test]
fn a_wrong_count_is_bad_argument_count_both_ways() {
	let (t, r) = both("Ok(`${df.METHOD(\"a\", 2)?}`)");
	eprintln!("count: typed: {t}\n       raw:   {r}");
	assert!(t.contains("Wrong number of arguments 3, expected 4"), "{t}");
	assert_eq!(t, r);
}

#[test]
fn borrowed_arguments_survive_and_owned_ones_are_taken_alike() {
	// the receiver and the string are borrowed: both stay readable
	let (t, r) = both(
		"let s = \"a\"; let n = df.METHOD(s, 1, other)?; Ok(`${n} ${s.len()} ${df.height()?}`)",
	);
	assert_eq!((t.as_str(), r.as_str()), ("ok 4 1 3", "ok 4 1 3"));
	// the frame argument is taken: reading it afterwards is the same VM error
	let (t, r) = both("let n = df.METHOD(\"a\", 1, other)?; Ok(`${other.height()?}`)");
	eprintln!("taken: typed: {t}\n       raw:   {r}");
	assert!(
		t.starts_with("vm error") && t.contains("Cannot read"),
		"{t}"
	);
	assert_eq!(t, r);
}

#[test]
fn a_borrow_conflict_fails_alike() {
	// the same frame as the borrowed receiver and the taken argument
	let (t, r) = both("Ok(`${df.METHOD(\"a\", 1, df)?}`)");
	eprintln!("conflict: typed: {t}\n          raw:   {r}");
	assert!(
		t.starts_with("vm error") && r.starts_with("vm error"),
		"{t} / {r}"
	);
	assert_eq!(t, r);
}
