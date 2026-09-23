//! Record 0085: validity bitmaps reach the script as owned `Vec<bool>`
//! snapshots; `None` means no mask, distinct from an empty vector.
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
fn masks_nulls_and_chunks_keep_their_shape() {
    let _s = SERIAL.lock().unwrap();
    let result = run(r#"
        fn bits(v) { let s = ""; for b in v { s = s + if b { "1" } else { "0" }; } s }
        pub fn main() {
            let one = fx::series_nulls().i64().unwrap();
            let single = match one.rechunk_validity().unwrap() { Some(v) => bits(v), None => "none" };
            let s = fx::series_nulls();
            s.append(fx::series_i64()).unwrap();
            let two = s.i64().unwrap();
            let per = two.iter_validities().unwrap();
            let first = match per[0] { Some(v) => bits(v), None => "none" };
            let second = match per[1] { Some(_) => "mask", None => "none" };
            let whole = match two.rechunk_validity().unwrap() { Some(v) => bits(v), None => "none" };
            let valid = fx::series_i64().i64().unwrap();
            let unmasked = match valid.rechunk_validity().unwrap() { Some(_) => "mask", None => "none" };
            `${single} ${per.len()} ${first} ${second} ${whole} ${unmasked} ${one.len().unwrap()}`
        }
    "#);
    assert_eq!(result, "101 2 101 none 101111 none 3");
}

#[test]
fn the_bit_bound_is_exact_and_cumulative_across_chunks() {
    let _s = SERIAL.lock().unwrap();
    let result = run(r#"
        pub fn main() {
            let one = fx::series_nulls().i64().unwrap();
            polars::set_materialize_limit(3);
            let at = one.rechunk_validity().is_ok();
            polars::set_materialize_limit(2);
            let below = one.rechunk_validity();
            let s = fx::series_nulls();
            s.append(fx::series_nulls()).unwrap();
            let two = s.i64().unwrap();
            polars::set_materialize_limit(5);
            let second = two.iter_validities();
            polars::set_materialize_limit(6);
            let both = two.iter_validities().unwrap().len();
            polars::set_materialize_limit(0);
            let again = one.rechunk_validity().is_ok();
            match (below, second) {
                (Err(a), Err(b)) => `${at} ${a.kind()} ${a.message()} | ${b.message()} | ${both} ${again} ${two.len().unwrap()}`,
                _ => "unexpected".to_string(),
            }
        }
    "#);
    assert_eq!(result, "true MaterializeLimit rechunk_validity: 3 validity bits with 0 already copied, more than the bound of 2 | iter_validities: 3 validity bits with 3 already copied, more than the bound of 5 | 2 true 6");
}
