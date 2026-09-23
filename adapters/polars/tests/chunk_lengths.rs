//! Record 0087: chunk lengths reach the script as an owned vector of
//! integers, one per chunk in order, bounded by the materialize limit.
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

#[test]
fn lengths_follow_the_chunks_and_outlive_the_receiver() {
    let _s = SERIAL.lock().unwrap();
    let result = run(r#"
        fn join(v) { let s = ""; for x in v { s = s + `${x},`; } s }
        pub fn main() {
            let one = fx::series_nulls().i64().unwrap().chunk_lengths().unwrap();
            let s = fx::series_nulls();
            s.append(fx::series_i64()).unwrap();
            s.append(fx::series_nulls().slice(0, 1).unwrap()).unwrap();
            let kept = s.i64().unwrap();
            let lens = kept.chunk_lengths().unwrap();
            let sum = 0; for x in lens { sum = sum + x; }
            let same = lens.len() == s.n_chunks();
            let owned = { let tmp = s.rechunk().i64().unwrap(); tmp.chunk_lengths().unwrap() };
            let empty = fx::series_i64().slice(0, 0).unwrap().i64().unwrap().chunk_lengths().unwrap();
            `${join(one)} ${join(lens)} ${sum} ${kept.len().unwrap()} ${same} ${join(owned)} ${empty.len()}`
        }
    "#);
    assert_eq!(result, "3, 3,3,1, 7 7 true 7, 1");
}

#[test]
fn the_chunk_bound_is_exact_and_the_receiver_survives() {
    let _s = SERIAL.lock().unwrap();
    let result = run(r#"
        pub fn main() {
            let s = fx::series_i64();
            s.append(fx::series_i64()).unwrap();
            let ca = s.i64().unwrap();
            polars::set_materialize_limit(2);
            let at = ca.chunk_lengths().unwrap().len();
            polars::set_materialize_limit(1);
            let over = ca.chunk_lengths();
            polars::set_materialize_limit(0);
            match over {
                Err(e) => `${at} ${e.kind()} ${e.message()} | ${ca.chunk_lengths().unwrap().len()} ${ca.len().unwrap()}`,
                Ok(_) => "unexpected".to_string(),
            }
        }
    "#);
    assert_eq!(result, "2 MaterializeLimit chunk_lengths: 2 items, more than the bound of 1 | 2 6");
}
