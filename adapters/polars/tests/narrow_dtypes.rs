//! Record 0081: the four narrow integer dtypes are reachable from a script.
//! Each fixture series converts to its `ChunkedArray` alias, reports the
//! matching dtype, and yields its values with the null placement kept.
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
fn narrow_integer_aliases_read_back_with_their_dtype() {
    let result = run(r#"
        pub fn main() {
            let i8 = fx::series_i8().i8().unwrap();
            let i16 = fx::series_i16().i16().unwrap();
            let u8 = fx::series_u8().u8().unwrap();
            let u16 = fx::series_u16().u16().unwrap();
            let integer = i8.dtype().is_integer() && i16.dtype().is_integer() && u8.dtype().is_integer() && u16.dtype().is_integer();
            let signed = i8.dtype().is_signed_integer() && i16.dtype().is_signed_integer() && u8.dtype().is_unsigned_integer() && u16.dtype().is_unsigned_integer();
            let values = i8.get(0).unwrap() == Some(1) && i8.get(2).unwrap() == Some(3)
                && i16.get(1).unwrap() == Some(2) && u8.get(2).unwrap() == Some(3) && u16.get(0).unwrap() == Some(1);
            let lengths = i8.len() == 3 && i16.len() == 3 && u8.len() == 3 && u16.len() == 3;
            // each alias refuses the other widths: the four are distinct script types
            let distinct = fx::series_i8().i16().is_err() && fx::series_i16().i8().is_err() && fx::series_u8().u16().is_err() && fx::series_u16().u8().is_err();
            `${integer} ${signed} ${values} ${lengths} ${distinct}`
        }
    "#);
    assert_eq!(result, "true true true true true", "integer signed values lengths distinct");
}
