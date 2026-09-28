//! Record 0111: Rust traits at the Rune protocol boundary. A `Hash` + `Eq` type is a working `HashMap` key (HASH with EQ from the Rust `==`); a `PartialEq`-only type is not; `PartialOrd` and `FromStr` match direct Rust; an owned Rune value runs its Rust destructor exactly once, moved or not, which is why `Drop` is a marker.
#![cfg(all(feature = "generated", feature = "test-support"))]
use rnx::rune::{self, Any, Context, Module, Source, Sources, Vm};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

fn run_with(script: &str, extra: Option<Module>) -> Result<rune::Value, String> {
	let mut polars = Module::with_crate("polars").unwrap();
	rnx_polars::build(&mut polars).unwrap();
	let mut fixtures = Module::with_crate("fx").unwrap();
	rnx_polars::generated::fixtures::install(&mut fixtures).unwrap();
	let mut context = Context::with_default_modules().unwrap();
	context.install(polars).unwrap();
	context.install(fixtures).unwrap();
	if let Some(m) = extra {
		context.install(m).unwrap();
	}
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
fn run(script: &str) -> Result<rune::Value, String> {
	run_with(script, None)
}
fn text(v: rune::Value) -> String {
	rune::from_value::<String>(v).unwrap()
}

use polars::prelude::TimeUnit;
use polars_dtype::categorical::CategoricalPhysical;

#[test]
fn a_hash_and_eq_type_is_a_working_map_key() {
	// equal keys meet, distinct keys stay apart, a second insert replaces
	let got = text(run(r#"pub fn main() {
		let m = std::collections::HashMap::new();
		m.insert(polars::TimeUnit::Milliseconds(), 1);
		m.insert(polars::TimeUnit::Nanoseconds(), 2);
		m.insert(polars::TimeUnit::Milliseconds(), 3);
		`${m.len()} ${m.get(polars::TimeUnit::Milliseconds()).unwrap()} ${m.get(polars::TimeUnit::Nanoseconds()).unwrap()} ${m.contains_key(polars::TimeUnit::Microseconds())}`
	}"#).unwrap());
	let mut m = std::collections::HashMap::new();
	m.insert(TimeUnit::Milliseconds, 1);
	m.insert(TimeUnit::Nanoseconds, 2);
	m.insert(TimeUnit::Milliseconds, 3);
	let direct = format!(
		"{} {} {} {}",
		m.len(),
		m[&TimeUnit::Milliseconds],
		m[&TimeUnit::Nanoseconds],
		m.contains_key(&TimeUnit::Microseconds)
	);
	assert_eq!(got, direct);
}

#[test]
fn a_partial_eq_only_type_is_not_a_map_key() {
	// JoinType derives PartialEq and Hash but not Eq at 0.55.2: its Hash is refused, so it cannot key a map
	let e = run(r#"pub fn main() { let m = std::collections::HashMap::new(); m.insert(polars::JoinType::Left(), 1); m.len() }"#).unwrap_err();
	assert!(
		e.contains("Unsupported unary operation `HASH`"),
		"the refusal is the missing HASH, not a later EQ failure: {e}"
	);
	// while `==` (PARTIAL_EQ) still works on it
	assert!(
		rune::from_value::<bool>(
			run(r#"pub fn main() { polars::JoinType::Left() == polars::JoinType::Left() }"#)
				.unwrap()
		)
		.unwrap()
	);
}

#[test]
fn partial_cmp_matches_direct_rust() {
	let got = text(run(r#"pub fn main() {
		let s = [];
		for (a, b) in [(polars::TimeUnit::Milliseconds(), polars::TimeUnit::Nanoseconds()), (polars::TimeUnit::Nanoseconds(), polars::TimeUnit::Milliseconds()), (polars::TimeUnit::Microseconds(), polars::TimeUnit::Microseconds())] {
			s.push(match std::ops::partial_cmp(a, b) { Some(o) => if o == std::cmp::Ordering::Less { "Less" } else if o == std::cmp::Ordering::Equal { "Equal" } else { "Greater" }, None => "None" });
		}
		s.iter().fold("", |a, b| a + b + ",")
	}"#).unwrap());
	let pairs = [
		(TimeUnit::Milliseconds, TimeUnit::Nanoseconds),
		(TimeUnit::Nanoseconds, TimeUnit::Milliseconds),
		(TimeUnit::Microseconds, TimeUnit::Microseconds),
	];
	let direct: String = pairs
		.iter()
		.map(|(a, b)| format!("{:?},", a.partial_cmp(b).unwrap()))
		.collect();
	assert_eq!(got, direct);
}

#[test]
fn parse_matches_from_str_and_fails_catchably() {
	for s in ["u8", "u16", "u32", "u64", "nope"] {
		let got = text(run(&format!(r#"pub fn main() {{ match polars::CategoricalPhysical::parse("{s}") {{ Ok(v) => `ok ${{v == polars::CategoricalPhysical::parse("{s}").unwrap()}}`, Err(e) => "err" }} }}"#)).unwrap());
		let direct = match s.parse::<CategoricalPhysical>() {
			Ok(_) => "ok true",
			Err(_) => "err",
		};
		assert_eq!(got, direct, "{s}");
	}
}

static DROPS: AtomicUsize = AtomicUsize::new(0);
#[derive(Any)]
#[rune(item = ::probe)]
struct Probe(#[allow(dead_code)] u8);
impl Drop for Probe {
	fn drop(&mut self) {
		DROPS.fetch_add(1, Ordering::SeqCst);
	}
}
#[rune::function(path = make)]
fn make() -> Probe {
	Probe(1)
}
#[rune::function(path = consume)]
fn consume(p: Probe) -> u8 {
	p.0
}

#[test]
fn an_owned_value_drops_exactly_once_moved_or_not() {
	// the generated wrappers are plain `#[derive(Any)] struct W(T)`: Rune drops the
	// owned wrapper once, and Rust's drop glue runs T's destructor once
	let module = || {
		let mut m = Module::with_crate("probe").unwrap();
		m.ty::<Probe>().unwrap();
		m.function_meta(make).unwrap();
		m.function_meta(consume).unwrap();
		m
	};
	for (script, want) in [
		("pub fn main() { let p = probe::make(); () }", 1),
		("pub fn main() { let p = probe::make(); let q = p; () }", 1),
		(
			"pub fn main() { let p = probe::make(); probe::consume(p); () }",
			1,
		),
		(
			"pub fn main() { let v = [probe::make(), probe::make()]; () }",
			2,
		),
	] {
		DROPS.store(0, Ordering::SeqCst);
		run_with(script, Some(module())).unwrap();
		assert_eq!(DROPS.load(Ordering::SeqCst), want, "{script}");
	}
}
