//! Record 0082: borrowed slices reach the script as owned, bounded vectors.
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
fn a_copied_slice_outlives_its_source() {
    let _serial = SERIAL.lock().unwrap();
    let result = run(r#"
        pub fn main() {
            let v = { let ca = fx::series_i64().i64().unwrap(); ca.cont_slice().unwrap() };
            let f = { let st = fx::series_struct().struct_().unwrap(); st.struct_fields().unwrap() };
            `${v.len()} ${v[0]} ${v[2]} ${f.len()}`
        }
    "#);
    assert_eq!(result, "3 1 3 3");
}

#[test]
fn direct_slice_bound_is_exact() {
    let _serial = SERIAL.lock().unwrap();
    let result = run(r#"
        pub fn main() {
            let ca = fx::series_i64().i64().unwrap();
            polars::set_materialize_limit(3);
            let at = ca.cont_slice();
            polars::set_materialize_limit(2);
            let over = ca.cont_slice();
            polars::set_materialize_limit(0);
            let after = ca.cont_slice().unwrap();
            match (at, over) {
                (Ok(v), Err(e)) => `${v.len()} ${e.kind()} ${e.message()} | ${after.len()}`,
                _ => "unexpected".to_string(),
            }
        }
    "#);
    assert!(result.starts_with("3 MaterializeLimit cont_slice: 3 slice elements with 0 already copied, more than the bound of 2 | 3"), "{result}");
}

#[test]
fn nested_slices_share_one_cumulative_bound() {
    let _serial = SERIAL.lock().unwrap();
    let result = run(r#"
        pub fn main() {
            let s = fx::series_i64();
            s.append(fx::series_i64()).unwrap();
            let chunks = s.n_chunks();
            let a = s.i64().unwrap();
            let views = a.data_views().unwrap();
            polars::set_materialize_limit(6);
            let at = a.data_views();
            polars::set_materialize_limit(5);
            let over = a.data_views();
            polars::set_materialize_limit(0);
            let contiguous = a.cont_slice();
            match (at, over, contiguous) {
                (Ok(v), Err(e), Err(c)) => `${chunks} ${views.len()} ${views[1].len()} ${v.len()} ${e.kind()} ${e.message()} | ${c.kind()}`,
                _ => "unexpected".to_string(),
            }
        }
    "#);
    assert!(result.starts_with("2 2 3 2 MaterializeLimit data_views: 3 slice elements with 3 already copied, more than the bound of 5 | "), "{result}");
    assert!(!result.ends_with("MaterializeLimit"), "a multi-chunk cont_slice is Polars's own error, not the bound: {result}");
}

#[test]
fn binary_cells_copy_zero_and_non_utf8_bytes_and_empty_slices() {
    let _serial = SERIAL.lock().unwrap();
    let result = run(r#"
        pub fn main() {
            let ca = fx::series_binary().binary().unwrap();
            let raw = ca.get(1).unwrap().unwrap();
            let empty = ca.get(2).unwrap().unwrap();
            let all = ca.iter().unwrap();
            let dense = ca.no_null_iter().unwrap();
            let first = ca.first().unwrap().unwrap();
            let max = ca.max_binary().unwrap().unwrap();
            `${raw.len()} ${raw[0]} ${raw[1]} ${empty.len()} ${all.len()} ${dense.len()} ${dense[2].len()} ${first[0]} ${first[1]} ${max[0]}`
        }
    "#);
    assert_eq!(result, "2 0 255 0 3 3 0 97 98 97");
}

#[test]
fn a_callback_sees_a_byte_snapshot_and_its_failure_keeps_the_receiver() {
    let _serial = SERIAL.lock().unwrap();
    let result = run(r#"
        pub fn main() {
            let ca = fx::series_binary().binary().unwrap();
            let seen = ca.for_each(|b| match b { Some(v) => if v.len() == 2 && v[0] == 0 && v[1] == 255 { panic("saw zero ff") }, None => {} });
            polars::set_materialize_limit(1);
            let bounded = ca.for_each(|b| {});
            polars::set_materialize_limit(0);
            let fine = ca.for_each(|b| {});
            match (seen, bounded, fine) {
                (Err(a), Err(b), Ok(_)) => `${a.kind()} ${a.message()} | ${b.kind()} ${b.message()} | ${ca.len().unwrap()}`,
                _ => "unexpected".to_string(),
            }
        }
    "#);
    assert!(result.starts_with("CallbackError callback BinaryChunked::for_each: call failed: Panicked: saw zero ff | CallbackError callback BinaryChunked::for_each: BinaryChunked::for_each: 2 slice elements with 0 already copied, more than the bound of 1 | 3"), "{result}");
}

#[test]
fn callback_slices_share_one_bound_across_the_whole_call() {
    let _serial = SERIAL.lock().unwrap();
    // the two 2-byte cells fit the bound of 3 one at a time; together they
    // do not, and one call of `for_each` copies both
    let result = run(r#"
        pub fn main() {
            let ca = fx::series_binary().binary().unwrap();
            polars::set_materialize_limit(3);
            let together = ca.for_each(|b| {});
            polars::set_materialize_limit(4);
            let fits = ca.for_each(|b| {});
            polars::set_materialize_limit(0);
            match (together, fits) {
                (Err(e), Ok(_)) => `${e.kind()} ${e.message()} ${ca.len().unwrap()}`,
                _ => "unexpected".to_string(),
            }
        }
    "#);
    assert_eq!(result, "CallbackError callback BinaryChunked::for_each: BinaryChunked::for_each: 2 slice elements with 2 already copied, more than the bound of 3 3");
}
