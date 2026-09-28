//! Record 0109: non-Clone values move. A consuming method on an Expr namespace or a builder takes the Rune value, as Rust takes it; the result matches direct Polars, and a later use of the moved value is an access error, never a value. A Clone receiver (`Expr`) is still cloned and stays usable.
#![cfg(all(feature = "generated", feature = "test-support"))]
use rnx::rune::{self, Context, Module, Source, Sources, Vm};
use std::sync::Arc;

fn run(script: &str) -> Result<rune::Value, String> {
	let mut polars = Module::with_crate("polars").unwrap();
	rnx_polars::build(&mut polars).unwrap();
	let mut fixtures = Module::with_crate("fx").unwrap();
	rnx_polars::generated::fixtures::install(&mut fixtures).unwrap();
	let mut context = Context::with_default_modules().unwrap();
	context.install(polars).unwrap();
	context.install(fixtures).unwrap();
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

use polars::prelude as p;
use polars::prelude::*;
use rnx_polars::generated::fixtures::{self as fx, values};

fn lazy_text(v: &rune::Value) -> String {
	fx::show_lazyframe(v).unwrap().to_text()
}
fn frame_text(lf: p::LazyFrame) -> String {
	rnx_polars::oracle::frame_repr(&lf.collect().unwrap())
		.prefixed("collected ")
		.to_text()
}
fn ok(v: rune::Value) -> rune::Value {
	match rune::from_value::<Result<rune::Value, rune::Value>>(v.clone()) {
		Ok(Ok(w)) => w,
		Ok(Err(e)) => panic!(
			"script error: {}",
			rnx_polars::oracle::rune_error_kind(&e).unwrap()
		),
		Err(_) => v,
	}
}

#[test]
fn namespace_methods_match_direct_polars() {
	// the name namespace: moved out of `fx::expr().name()`, applied in a select
	let v = run(r#"pub fn main() { fx::lf().select_([fx::expr().name().suffix("_s"), fx::expr().name().prefix("p_")]) }"#).unwrap();
	let direct = values::lf().select([
		p::col("x").name().suffix("_s"),
		p::col("x").name().prefix("p_"),
	]);
	assert_eq!(lazy_text(&ok(v)), frame_text(direct));
}

#[test]
fn builder_chain_matches_direct_polars() {
	let v = run(r#"pub fn main() { Ok(polars::JoinBuilder::new(fx::lf()).with(fx::lf()).left_on([fx::expr()])?.right_on([fx::expr()])?.how(polars::JoinType::Left()).maintain_order(polars::MaintainOrderJoin::Left()).finish()) }"#).unwrap();
	let direct = values::lf()
		.join_builder()
		.with(values::lf())
		.left_on([p::col("x")])
		.right_on([p::col("x")])
		.how(JoinType::Left)
		.maintain_order(MaintainOrderJoin::Left)
		.finish();
	assert_eq!(lazy_text(&ok(v)), frame_text(direct));
}

#[test]
fn a_moved_namespace_is_an_access_error() {
	let e = run(r#"pub fn main() { let ns = fx::expr().name(); let a = ns.suffix("_a"); let b = ns.suffix("_b"); (a, b) }"#).unwrap_err();
	assert!(
		e.contains("Cannot take"),
		"a moved value is refused by the VM, not read: {e}"
	);
}

#[test]
fn a_moved_builder_is_an_access_error() {
	let e = run(r#"pub fn main() { let b = polars::JoinBuilder::new(fx::lf()).with(fx::lf()).left_on([fx::expr()])?.right_on([fx::expr()])?; let j = b.finish(); let again = b.finish(); Ok((j, again)) }"#).unwrap_err();
	assert!(
		e.contains("Cannot take"),
		"a moved value is refused by the VM, not read: {e}"
	);
	// a builder method that returns the builder hands a fresh value on; only the consumed one is gone
	let v = run(
		r#"pub fn main() { let b = polars::JoinBuilder::new(fx::lf()); let b2 = b.with(fx::lf()); Ok(b2.left_on([fx::expr()])?.right_on([fx::expr()])?.finish()) }"#,
	);
	assert!(v.is_ok(), "{v:?}");
}

#[test]
fn a_clone_receiver_stays_usable() {
	// `Expr::name` consumes an Expr in Rust; Expr is Clone, so the Rune value is cloned and reusable
	let v = run(r#"pub fn main() { let e = fx::expr(); let a = e.name().suffix("_a"); let b = e.name().suffix("_b"); fx::lf().select_([a, b, e]) }"#).unwrap();
	let direct = values::lf().select([
		p::col("x").name().suffix("_a"),
		p::col("x").name().suffix("_b"),
		p::col("x"),
	]);
	assert_eq!(lazy_text(&ok(v)), frame_text(direct));
}
