//! Record 0093: u64 and usize read-backs are range-checked on every route; only the proven bounded allowlist keeps a plain int.
#![cfg(all(feature = "generated", feature = "test-support"))]
use rnx::rune::{self, Context, Module, Source, Sources, Vm};
use std::sync::Arc;
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn run(script: &str) -> String {
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
    let built = rune::prepare(&mut sources).with_context(&context).with_diagnostics(&mut diagnostics).build();
    if built.is_err() {
        eprintln!("diagnostics: {:?}", diagnostics.diagnostics());
    }
    let mut vm = Vm::new(runtime, Arc::new(built.unwrap()));
    rune::from_value(vm.call(["main"], ()).unwrap()).unwrap()
}


/// i64::MAX reads back exactly; i64::MAX + 1 and u64::MAX are a typed
/// ConversionError naming the method on the option, iterator, vector and
/// copied-slice routes, and fail a callback (sink and mutating) with the typed
/// callback error; the receiver and a later in-range call are unaffected; a
/// null stays None; the proven `width` is still a plain int while `height`
/// is now a Result.
const PROBE: &str = r#"
    pub fn main() {
        let ca = fx::series_u64_boundary().u64().unwrap();
        let kind = |r| match r { Ok(Some(v)) => `Some(${v})`, Ok(None) => "None", Ok(()) => "ok", Ok(v) => `ok ${v}`, Err(e) => `${e.kind()} ${e.message()}` };
        let g = [kind(ca.get(0)), kind(ca.get(1)), kind(ca.get(2)), kind(ca.get(3))];
        let ends = [kind(ca.first()), kind(ca.last())];
        let routes = [kind(ca.iter().map(|v| v.len())), kind(ca.to_vec().map(|v| v.len())), kind(fx::series_u64_extremes().u64().unwrap().cont_slice().map(|v| v.len()))];
        let small = fx::series_u64().u64().unwrap();
        let fine = [kind(small.cont_slice().map(|v| v[2])), kind(small.iter().map(|v| v.len()))];
        let failed = kind(ca.for_each(|v| ()));
        let mutated = kind(ca.apply_mut(|v| v));
        let reused = kind(small.for_each(|v| ()));
        let m = fx::series_u64().u64().unwrap();
        let applied = kind(m.apply_mut(|v| v + 10));
        let still = [kind(ca.get(0)), kind(ca.get(1)), kind(m.get(2))];
        let df = fx::df();
        `${g[0]} | ${g[1]} | ${g[2]} | ${g[3]} | ${ends[0]} | ${ends[1]} | ${routes[0]} | ${routes[1]} | ${routes[2]} | ${fine[0]} | ${fine[1]} | ${failed} | ${mutated} | ${reused} ${applied} | ${still[0]} ${still[1]} ${still[2]} | ${df.width()} ${df.height().unwrap()}`
    }
"#;

#[test]
fn boundary_values_are_checked_on_every_route() {
    let _serial = SERIAL.lock().unwrap();
    let got = run(PROBE);
    let fit = |m: &str, v: &str| format!("ConversionError {m}: {v} does not fit a script integer");
    let cb = |m: &str| format!("CallbackError callback UInt64Chunked::{m}: UInt64Chunked::{m}: 9223372036854775808 does not fit a script integer");
    let (max, over, top) = ("9223372036854775807", "9223372036854775808", "18446744073709551615");
    let want = [
        format!("Some({max})"), fit("get", over), fit("get", top), "None".into(),
        format!("Some({max})"), "None".into(),
        fit("iter", over), fit("to_vec", over), fit("cont_slice", top),
        "ok 3".into(), "ok 3".into(),
        cb("for_each"), cb("apply_mut"), "ok ok".into(),
        format!("Some({max}) {} Some(13)", fit("get", over)),
        "3 3".into(),
    ].join(" | ");
    assert_eq!(got, want);
}
