//! Record 0086: validity masks supplied by the script as `Vec<bool>`,
//! checked against the receiver or values length before Polars sees them.
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

const BITS: &str = r#"
    fn bits(v) { let s = ""; for b in v { s = s + if b { "1" } else { "0" }; } s }
    fn mask(ca) { match ca.rechunk_validity().unwrap() { Some(v) => bits(v), None => "none" } }
"#;

#[test]
fn set_and_with_validity_apply_checked_masks() {
    let _s = SERIAL.lock().unwrap();
    let result = run(&format!("{BITS}{}", r#"
        pub fn main() {
            let ca = fx::series_i64().i64().unwrap();
            let m = [true, false, true];
            ca.set_validity(Some(m)).unwrap();
            let after = `${mask(ca)} ${ca.null_count()} ${ca.get(1).unwrap() == None} ${m.len()}`;
            let short = ca.set_validity(Some([true, false]));
            let long = ca.set_validity(Some([true, true, true, true]));
            let kept = mask(ca);
            ca.set_validity(None).unwrap();
            let cleared = `${mask(ca)} ${ca.null_count()}`;
            let all_false = ca.with_validity(Some([false, false, false])).unwrap();
            let s = fx::series_nulls();
            s.append(fx::series_i64()).unwrap();
            let two = s.i64().unwrap();
            let chunks = two.with_validity(Some([true, true, true, false, true, true])).unwrap();
            let per = chunks.iter_validities().unwrap();
            match (short, long) {
                (Err(a), Err(b)) => `${after} | ${a.kind()} ${a.message()} | ${b.message()} | ${kept} | ${cleared} | ${all_false.null_count()} ${mask(ca)} | ${per.len()} ${chunks.null_count()} ${mask(chunks)} ${mask(two)}`,
                _ => "unexpected".to_string(),
            }
        }
    "#));
    assert_eq!(result, "101 1 true 3 | ShapeMismatch Int64Chunked::set_validity: the mask has 2 bits, expected 3 | Int64Chunked::set_validity: the mask has 4 bits, expected 3 | 101 | none 0 | 3 none | 2 1 111011 101111");
}

#[test]
fn constructors_take_values_masks_and_boolean_bits() {
    let _s = SERIAL.lock().unwrap();
    let result = run(&format!("{BITS}{}", r#"
        pub fn main() {
            let values = [1, 2, 3];
            let ca = polars::Int64Chunked::from_vec_validity("x", values, Some([true, false, true])).unwrap();
            let plain = polars::Int64Chunked::from_vec_validity("x", [4, 5], None).unwrap();
            let bad = polars::Int64Chunked::from_vec_validity("x", values, Some([true]));
            let empty = polars::Int64Chunked::from_vec_validity("x", [], Some([])).unwrap();
            let flags = polars::BooleanChunked::from_bitmap("b", [true, false, false, true]).unwrap();
            let none = polars::BooleanChunked::from_bitmap("b", []).unwrap();
            match bad {
                Err(e) => `${mask(ca)} ${ca.null_count()} ${mask(plain)} ${e.message()} ${empty.len()} ${flags.len()} ${flags.get(0).unwrap() == Some(true)} ${flags.get(1).unwrap() == Some(false)} ${flags.null_count()} ${none.len()} ${values.len()}`,
                Ok(_) => "unexpected".to_string(),
            }
        }
    "#));
    assert_eq!(result, "101 1 none Int64Chunked::from_vec_validity: the mask has 1 bits, expected 3 0 4 true true 0 0 3");
}

#[test]
fn the_mask_bound_is_exact_and_the_receiver_survives_a_refusal() {
    let _s = SERIAL.lock().unwrap();
    let result = run(&format!("{BITS}{}", r#"
        pub fn main() {
            let ca = fx::series_i64().i64().unwrap();
            polars::set_materialize_limit(2);
            let over = ca.set_validity(Some([true, false, true]));
            polars::set_materialize_limit(3);
            let at = ca.set_validity(Some([true, false, true])).is_ok();
            polars::set_materialize_limit(0);
            match over {
                Err(e) => `${e.message()} | ${at} ${mask(ca)} ${ca.len()}`,
                Ok(_) => "unexpected".to_string(),
            }
        }
    "#));
    assert_eq!(result, "Int64Chunked::set_validity: 3 mask bits with 0 already copied, more than the bound of 2 | true 101 3");
}
