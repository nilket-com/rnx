//! Record 0074, labeled diagnostic outside the classifier: for the three
//! lazy joins, compare the binding's collected rows with Rust's as
//! multisets (duplicates kept) together with the schema, and count how
//! many distinct row orders six Rust runs produce. Writes DIAG_OUT.
#![cfg(feature = "test-support")]
use polars::prelude as p;
use polars::prelude::*;
use rnx::rune::{self, Context, Module, Source, Sources, Vm};
use rnx_polars::generated::fixtures::values::*;
use rnx_polars::oracle::{frame_repr, Repr};
use std::sync::Arc;

fn rows(r: &Repr) -> (String, Vec<String>) {
	match r {
		Repr::Frame { head, rows } => (head.clone(), rows.clone()),
		Repr::Seq(_) => (r.to_text(), vec![]),
		Repr::Text(t) => (t.clone(), vec![]),
	}
}

#[test]
fn join_row_multisets() {
	let mut m = Module::with_crate("polars").unwrap();
	rnx_polars::build(&mut m).unwrap();
	let mut f = Module::with_crate("fx").unwrap();
	rnx_polars::generated::fixtures::install(&mut f).unwrap();
	let mut context = Context::with_default_modules().unwrap();
	context.install(m).unwrap();
	context.install(f).unwrap();
	let runtime = Arc::new(context.runtime().unwrap());
	let mut report = Vec::new();
	for name in ["inner_join", "left_join", "full_join"] {
		// binding side: the generated method on the fixture, collected
		let script = format!("pub fn main() {{ let a = fx::lf(); let b = fx::lf(); (a.{name}(b, fx::expr(), fx::expr()), ()) }}");
		let mut sources = Sources::new();
		sources.insert(Source::memory(&script).unwrap()).unwrap();
		let unit = rune::prepare(&mut sources).with_context(&context).build().unwrap();
		let mut vm = Vm::new(runtime.clone(), Arc::new(unit));
		let out = vm.call(["main"], ()).unwrap();
		let (v, _) = rune::from_value::<(rune::Value, rune::Value)>(out).unwrap();
		let binding = rnx_polars::generated::fixtures::show_lazyframe(&v).unwrap();
		// Rust side: the same call, six times
		let rust: Vec<Repr> = (0..6)
			.map(|_| {
				let a = lf();
				let b = lf();
				let joined = match name {
					"inner_join" => a.inner_join(b, expr(), expr()),
					"left_join" => a.left_join(b, expr(), expr()),
					_ => a.full_join(b, expr(), expr()),
				};
				match joined.collect() {
					Ok(d) => frame_repr(&d).prefixed("collected "),
					Err(e) => Repr::Text(format!("collect error: {e}")),
				}
			})
			.collect();
		let (bh, mut br) = rows(&binding);
		let (rh, mut rr) = rows(&rust[0]);
		br.sort();
		rr.sort();
		let mut orders: Vec<Vec<String>> = rust.iter().map(|r| rows(r).1).collect();
		orders.sort();
		orders.dedup();
		report.push(serde_json::json!({
			"name": name,
			"schema_equal": bh == rh,
			"multiset_equal": br == rr,
			"rows_binding": br.len(),
			"rows_rust": rr.len(),
			"distinct_orders": orders.len(),
			"binding": binding.to_text(),
			"rust_first": rust[0].to_text(),
		}));
	}
	let out = serde_json::json!({"run_id": std::env::var("DIAG_RUN_ID").unwrap_or_default(), "joins": report});
	let path = std::env::var("DIAG_OUT").expect("DIAG_OUT");
	std::fs::write(&path, serde_json::to_string_pretty(&out).unwrap() + "\n").unwrap();
	let _ = p::DataType::Int64;
}
