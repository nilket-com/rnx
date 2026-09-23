//! Record 0089: `arg_min_numeric` and `arg_max_numeric` on the integer
//! wrappers, each scenario compared with the same call made directly in Rust.
#![cfg(all(feature = "generated", feature = "test-support"))]
use polars::chunked_array::arg_min_max::{arg_max_numeric, arg_min_numeric};
use polars::prelude as p;
use rnx::rune::{self, Context, Module, Source, Sources, Vm};
use std::sync::Arc;

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

fn show(v: Option<usize>) -> String { v.map_or("none".into(), |i| i.to_string()) }
fn both(ca: &p::Int64Chunked) -> String { format!("{}/{}", show(arg_min_numeric(ca)), show(arg_max_numeric(ca))) }

#[test]
fn integer_extrema_match_polars_across_shapes_and_sorted_flags() {
    use p::*;
    let nulls = Series::new("x".into(), [Some(1i64), None, Some(3)]);
    let mut two = nulls.clone();
    two.append(&Series::new("x".into(), [1i64, 2, 3])).unwrap();
    let dup = Int64Chunked::from_vec("x".into(), vec![2, 5, 1, 5, 1]);
    let mut asc = Int64Chunked::from_vec("x".into(), vec![3, 1, 2]);
    asc.set_sorted_flag(polars::series::IsSorted::Ascending);
    let mut desc = Int64Chunked::from_vec("x".into(), vec![3, 1, 2]);
    desc.set_sorted_flag(polars::series::IsSorted::Descending);
    let empty = Int64Chunked::from_vec("x".into(), vec![]);
    let all_null = Series::new("x".into(), [None::<i64>, None]).i64().unwrap().clone();
    let expected = [both(nulls.i64().unwrap()), both(two.i64().unwrap()), both(&dup), both(&asc), both(&desc), both(&empty), both(&all_null)].join(" ");
    let got = run(r#"
        fn both(ca) {
            let lo = match polars::Int64Chunked::arg_min_numeric(ca).unwrap() { Some(i) => `${i}`, None => "none" };
            let hi = match polars::Int64Chunked::arg_max_numeric(ca).unwrap() { Some(i) => `${i}`, None => "none" };
            `${lo}/${hi}`
        }
        pub fn main() {
            let nulls = fx::series_nulls().i64().unwrap();
            let s = fx::series_nulls();
            s.append(fx::series_i64()).unwrap();
            let two = s.i64().unwrap();
            let dup = polars::Int64Chunked::from_vec("x", [2, 5, 1, 5, 1]).unwrap();
            let asc = polars::Int64Chunked::from_vec("x", [3, 1, 2]).unwrap();
            asc.set_sorted_flag(polars::IsSorted::Ascending());
            let desc = polars::Int64Chunked::from_vec("x", [3, 1, 2]).unwrap();
            desc.set_sorted_flag(polars::IsSorted::Descending());
            let empty = polars::Int64Chunked::from_vec("x", []).unwrap();
            let all_null = polars::Int64Chunked::from_vec_validity("x", [7, 8], Some([false, false])).unwrap();
            let out = `${both(nulls)} ${both(two)} ${both(dup)} ${both(asc)} ${both(desc)} ${both(empty)} ${both(all_null)}`;
            let kept = nulls.len() == 3 && nulls.null_count() == 1 && two.len() == 6;
            `${out}|${kept}`
        }
    "#);
    assert_eq!(got, format!("{expected}|true"));
    // the cases the plan names, spelled out: empty and all-null give none, a
    // wrongly asserted sorted flag decides the answer as it does in Polars
    assert!(expected.ends_with("none/none none/none"), "{expected}");
    assert!(expected.contains(" 0/2 2/0 "), "sorted flags answer from the ends: {expected}");
}

#[test]
fn every_integer_wrapper_has_both_extrema() {
    let got = run(r#"
        fn pair(lo, hi) { `${lo.unwrap().unwrap()}${hi.unwrap().unwrap()}` }
        pub fn main() {
            [
                pair(polars::Int8Chunked::arg_min_numeric(fx::series_i8().i8().unwrap()), polars::Int8Chunked::arg_max_numeric(fx::series_i8().i8().unwrap())),
                pair(polars::Int16Chunked::arg_min_numeric(fx::series_i16().i16().unwrap()), polars::Int16Chunked::arg_max_numeric(fx::series_i16().i16().unwrap())),
                pair(polars::Int32Chunked::arg_min_numeric(fx::series_i32().i32().unwrap()), polars::Int32Chunked::arg_max_numeric(fx::series_i32().i32().unwrap())),
                pair(polars::UInt8Chunked::arg_min_numeric(fx::series_u8().u8().unwrap()), polars::UInt8Chunked::arg_max_numeric(fx::series_u8().u8().unwrap())),
                pair(polars::UInt16Chunked::arg_min_numeric(fx::series_u16().u16().unwrap()), polars::UInt16Chunked::arg_max_numeric(fx::series_u16().u16().unwrap())),
                pair(polars::IdxCa::arg_min_numeric(fx::series_u32().u32().unwrap()), polars::IdxCa::arg_max_numeric(fx::series_u32().u32().unwrap())),
                pair(polars::UInt64Chunked::arg_min_numeric(fx::series_u64().u64().unwrap()), polars::UInt64Chunked::arg_max_numeric(fx::series_u64().u64().unwrap())),
            ].iter().fold("", |a, b| a + b + " ")
        }
    "#);
    assert_eq!(got, "02 02 02 02 02 02 02 ");
}
