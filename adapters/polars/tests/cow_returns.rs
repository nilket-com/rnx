//! Record 0088: `Cow` results of `rechunk` and the List/Struct
//! `to_physical_repr` reach the script as owned wrappers, from either the
//! borrowed or the owned branch; the receiver is unchanged.
#![cfg(all(feature = "generated", feature = "test-support"))]
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

#[test]
fn rechunk_is_owned_from_both_branches() {
    let result = run(r#"
        fn bits(v) { let s = ""; for b in v { s = s + if b { "1" } else { "0" }; } s }
        pub fn main() {
            let single = { let ca = fx::series_nulls().i64().unwrap(); ca.rechunk() };
            let s = fx::series_nulls();
            s.append(fx::series_nulls()).unwrap();
            let multi = s.i64().unwrap();
            let joined = multi.rechunk();
            let empty = fx::series_i64().slice(0, 0).unwrap().i64().unwrap().rechunk();
            let mask = match joined.rechunk_validity().unwrap() { Some(v) => bits(v), None => "none" };
            `${single.len()} ${single.null_count()} ${single.chunk_lengths().unwrap().len()} | ${joined.len()} ${joined.chunk_lengths().unwrap().len()} ${joined.null_count()} ${mask} ${joined.get(3).unwrap() == Some(1)} | ${multi.chunk_lengths().unwrap().len()} ${multi.len()} | ${empty.len()}`
        }
    "#);
    assert_eq!(result, "3 1 1 | 6 1 2 101101 true | 2 6 | 0");
}

#[test]
fn physical_representations_convert_logical_inners_and_keep_physical_ones() {
    let result = run(r#"
        pub fn main() {
            let physical = fx::series_i64().implode().unwrap();
            let same = physical.to_physical_repr();
            let dates = fx::series_i32().cast(polars::DataType::Date()).unwrap();
            let logical = dates.implode().unwrap();
            let converted = logical.to_physical_repr();
            let inner = |ca| match ca.dtype().inner_dtype() { Some(d) => d.is_temporal(), None => false };
            let st = dates.into_frame().into_struct("s");
            let pst = st.to_physical_repr();
            let field_temporal = |ca| ca.struct_fields().unwrap()[0].dtype().is_temporal();
            `${inner(same)} ${same.len()} | ${inner(logical)} ${inner(converted)} ${converted.len()} | ${field_temporal(st)} ${field_temporal(pst)} ${pst.len()}`
        }
    "#);
    assert_eq!(result, "false 1 | true false 1 | true false 3");
}
