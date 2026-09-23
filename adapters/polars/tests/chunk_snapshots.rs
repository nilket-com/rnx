//! Record 0099: `chunks()` on the ten numeric wrappers as owned nested options, compared chunk by chunk with direct Polars, bounded, and checked for u64 read-back.
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

use polars::prelude as p;
use polars::prelude::NamedFrom;
use rnx_polars::generated::fixtures::values;

const SHOW: &str = r#"
    fn cell(x) { match x { Some(v) => `${v}`, None => "n" } }
    fn show(r) { match r { Ok(outer) => { let s = "["; let i = 0; for inner in outer { if i > 0 { s = s + "|"; } let j = 0; for x in inner { if j > 0 { s = s + ","; } s = s + cell(x); j = j + 1; } i = i + 1; } s + "]" }, Err(e) => `${e.kind()}: ${e.message()}` } }
"#;
/// The script's rendering of the chunk list Rust sees directly.
macro_rules! rust_show {
    ($ca:expr, $n:ty, $fmt:expr) => {{
        let ca = $ca;
        let parts: Vec<String> = ca.chunks().iter().map(|a| a.as_any().downcast_ref::<polars_arrow::array::PrimitiveArray<$n>>().unwrap().iter().map(|x| x.map_or("n".to_string(), |v| $fmt(*v))).collect::<Vec<_>>().join(",")).collect();
        format!("[{}]", parts.join("|"))
    }};
}
fn int<N: Into<i64>>(v: N) -> String { v.into().to_string() }
fn flt<N: Into<f64>>(v: N) -> String { format!("{:?}", v.into()) }

fn row(dtype: &str, accessor: &str, alias: &str) -> String {
    run(&format!(r#"{SHOW}
        pub fn main() {{
            let s = fx::series_i64();
            s.append(fx::series_nulls()).unwrap();
            let r = s.cast(polars::DataType::{dtype}()).unwrap().{accessor}().unwrap();
            let one = fx::series_i64().cast(polars::DataType::{dtype}()).unwrap().{accessor}().unwrap();
            let e = polars::{alias}::from_vec("x", []).unwrap();
            let kept = {{ let t = fx::series_i64().cast(polars::DataType::{dtype}()).unwrap().{accessor}().unwrap(); t.chunks() }};
            `${{show(r.chunks())}} ${{show(one.chunks())}} ${{show(e.chunks())}} ${{show(kept)}} ${{show(r.chunks())}}`
        }}
    "#))
}
macro_rules! rust_row {
    ($dt:expr, $acc:ident, $ca:ty, $n:ty, $fmt:expr) => {{
        let mut s = p::Series::new("x".into(), [1i64, 2, 3]);
        s.append(&p::Series::new("x".into(), [Some(1i64), None, Some(3)])).unwrap();
        let s = s.cast(&$dt).unwrap();
        let r = s.$acc().unwrap();
        let one = p::Series::new("x".into(), [1i64, 2, 3]).cast(&$dt).unwrap();
        let e = <$ca>::from_vec("x".into(), vec![]);
        let a = rust_show!(r, $n, $fmt);
        format!("{a} {} {} {} {a}", rust_show!(one.$acc().unwrap(), $n, $fmt), rust_show!(&e, $n, $fmt), rust_show!(one.$acc().unwrap(), $n, $fmt))
    }};
}

#[test]
fn every_numeric_wrapper_keeps_its_chunks() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let checks = [
        ("Int8", row("Int8", "i8", "Int8Chunked"), rust_row!(p::DataType::Int8, i8, p::Int8Chunked, i8, int)),
        ("Int16", row("Int16", "i16", "Int16Chunked"), rust_row!(p::DataType::Int16, i16, p::Int16Chunked, i16, int)),
        ("Int32", row("Int32", "i32", "Int32Chunked"), rust_row!(p::DataType::Int32, i32, p::Int32Chunked, i32, int)),
        ("Int64", row("Int64", "i64", "Int64Chunked"), rust_row!(p::DataType::Int64, i64, p::Int64Chunked, i64, int)),
        ("UInt8", row("UInt8", "u8", "UInt8Chunked"), rust_row!(p::DataType::UInt8, u8, p::UInt8Chunked, u8, int)),
        ("UInt16", row("UInt16", "u16", "UInt16Chunked"), rust_row!(p::DataType::UInt16, u16, p::UInt16Chunked, u16, int)),
        ("UInt32", row("UInt32", "idx", "IdxCa"), rust_row!(p::DataType::UInt32, idx, p::IdxCa, u32, int)),
        ("UInt64", row("UInt64", "u64", "UInt64Chunked"), rust_row!(p::DataType::UInt64, u64, p::UInt64Chunked, u64, |v: u64| i64::try_from(v).unwrap().to_string())),
        ("Float32", row("Float32", "f32", "Float32Chunked"), rust_row!(p::DataType::Float32, f32, p::Float32Chunked, f32, flt)),
        ("Float64", row("Float64", "f64", "Float64Chunked"), rust_row!(p::DataType::Float64, f64, p::Float64Chunked, f64, flt)),
    ];
    for (name, got, want) in &checks {
        assert_eq!(got, want, "{name}");
    }
    // spelled out: two chunks with the null in the second; one chunk; an empty receiver is one empty chunk
    assert_eq!(checks[3].1, "[1,2,3|1,n,3] [1,2,3] [] [1,2,3] [1,2,3|1,n,3]");
    assert_eq!(checks[9].1, "[1.0,2.0,3.0|1.0,n,3.0] [1.0,2.0,3.0] [] [1.0,2.0,3.0] [1.0,2.0,3.0|1.0,n,3.0]");
    assert_eq!(p::Int64Chunked::from_vec("x".into(), vec![]).chunks().len(), 1, "[] above is one empty chunk, not none");
}

#[test]
fn extremes_floats_and_u64_read_back() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let got = run(&format!(r#"{SHOW}
        pub fn main() {{
            let i = polars::Int8Chunked::from_vec("x", [-128, 127, 0]).unwrap();
            let u = polars::UInt64Chunked::from_vec("x", [0, 9223372036854775807]).unwrap();
            let f = polars::Float32Chunked::from_vec("x", [0.1, -0.0]).unwrap();
            let d = polars::Float64Chunked::from_vec("x", [-0.0, 0.0 / 0.0, 1.0 / 0.0, -1.0 / 0.0]).unwrap();
            let b = fx::series_u64_boundary().u64().unwrap();
            let x = fx::series_u64_extremes().u64().unwrap();
            let after = match b.get(0) {{ Ok(Some(v)) => v, _ => 0 }};
            `${{show(i.chunks())}} ${{show(u.chunks())}} ${{show(f.chunks())}} ${{show(d.chunks())}} | ${{show(b.chunks())}} | ${{show(x.chunks())}} | ${{after}} ${{show(b.chunks())}}`
        }}
    "#));
    let want = format!("{} {} {} {} | ConversionError: chunks: 9223372036854775808 does not fit a script integer | ConversionError: chunks: 18446744073709551615 does not fit a script integer | {} ConversionError: chunks: 9223372036854775808 does not fit a script integer",
        rust_show!(p::Int8Chunked::from_vec("x".into(), vec![i8::MIN, i8::MAX, 0]), i8, int),
        rust_show!(p::UInt64Chunked::from_vec("x".into(), vec![0, i64::MAX as u64]), u64, |v: u64| i64::try_from(v).unwrap().to_string()),
        rust_show!(p::Float32Chunked::from_vec("x".into(), vec![0.1f64 as f32, -0.0]), f32, flt),
        rust_show!(p::Float64Chunked::from_vec("x".into(), vec![-0.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY]), f64, flt),
        i64::MAX);
    assert_eq!(got, want);
    assert!(got.starts_with("[-128,127,0] [0,9223372036854775807] [0.10000000149011612,-0.0] [-0.0,NaN,inf,-inf]"), "{got}");
    // the Rust side holds the full values the script cannot
    assert_eq!(values::series_u64_boundary().u64().unwrap().chunks().len(), 1);
}

#[test]
fn the_whole_result_is_bounded_chunks_plus_cells() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    // two chunks and six cells = 8 slots; one empty chunk = 1 slot
    let got = run(&format!(r#"{SHOW}
        pub fn main() {{
            let s = fx::series_i64();
            s.append(fx::series_nulls()).unwrap();
            let r = s.i64().unwrap();
            let e = polars::Int64Chunked::from_vec("x", []).unwrap();
            let out = [];
            polars::set_materialize_limit(7); out.push(show(r.chunks()));
            polars::set_materialize_limit(8); out.push(show(r.chunks()));
            polars::set_materialize_limit(1); out.push(show(e.chunks()));
            polars::set_materialize_limit(0); out.push(show(r.chunks()));
            out.iter().fold("", |acc, x| acc + x + "\n")
        }}
    "#));
    assert_eq!(got.lines().collect::<Vec<_>>(), [
        "MaterializeLimit: chunks: 2 chunks and 6 cells (8 slots), more than the bound of 7",
        "[1,2,3|1,n,3]",
        "[]",
        "[1,2,3|1,n,3]",
    ], "{got}");
}
