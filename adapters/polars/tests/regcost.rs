//! Record 0073 gate 2: where the real module's registration cost goes.
//! Run with `--release --nocapture`; not an assertion, a measurement.
use rnx::rune::{Context, Module};
use std::time::Instant;

fn time(label: &str, fill: impl Fn(&mut Module) -> Result<(), rnx::rune::ContextError>) {
	let mut best = u128::MAX;
	for _ in 0..7 {
		let t0 = Instant::now();
		let mut m = Module::with_crate("polars").unwrap();
		fill(&mut m).unwrap();
		let t1 = Instant::now();
		let mut c = Context::with_default_modules().unwrap();
		c.install(m).unwrap();
		let t2 = Instant::now();
		best = best.min((t2 - t0).as_micros());
		let _ = t1;
	}
	eprintln!("{label}: {best} µs (module build + default context + install)");
}

#[test]
#[ignore]
fn registration_breakdown() {
	time("empty module", |_| Ok(()));
	time("types only", |m| rnx_polars::generated::types::install(m));
	time("types + support", |m| { rnx_polars::generated::types::install(m)?; rnx_polars::generated::support::install(m) });
	// generated functions need the hand-written wrapper types, so their
	// cost is full build() minus the lines above
	time("full build()", |m| rnx_polars::build(m).map(|_| ()).map_err(|_| unreachable!()));
}
