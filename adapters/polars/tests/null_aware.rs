//! Record 0096: `to_vec_null_aware` on the ten numeric wrappers returns one owned vector of options, compared with the direct Rust `Either`, and is bounded before the Polars call.
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

const SHOW: &str = r#"
    fn show(r) { match r { Ok(v) => v.iter().fold("", |a, x| { let s = match x { Some(y) => `${y}`, None => "n" }; if a == "" { s } else { a + "," + s } }), Err(e) => `${e.kind()}: ${e.message()}` } }
"#;
/// The script's rendering of a Rust `Either` result, with its branch tag:
/// every Left element is `Some`; integers read back through 0093's checked
/// widening. (`Either`'s inherent `either` keeps the crate unnamed.)
macro_rules! show_int {
    ($r:expr) => {{
        let (tag, v) = $r.either(|v| ('L', v.into_iter().map(Some).collect::<Vec<_>>()), |v| ('R', v));
        (tag, v.into_iter().map(|x| x.map_or("n".to_string(), |y| i64::try_from(y).map_or("ConversionError".into(), |y| y.to_string()))).collect::<Vec<_>>().join(","))
    }};
}
/// Floats: widened to `f64`, `Debug` formatting (sign of zero, NaN, inf).
macro_rules! show_float {
    ($r:expr) => {{
        let (tag, v) = $r.either(|v| ('L', v.into_iter().map(Some).collect::<Vec<_>>()), |v| ('R', v));
        (tag, v.into_iter().map(|x| x.map_or("n".to_string(), |y| format!("{:?}", f64::from(y)))).collect::<Vec<_>>().join(","))
    }};
}

const MASK: [bool; 4] = [true, false, true, true];

fn rune_row(alias: &str, vals: &str, dtype: &str, accessor: &str) -> String {
    run(&format!(r#"{SHOW}
        pub fn main() {{
            let a = polars::{alias}::from_vec("x", {vals}).unwrap();
            let b = polars::{alias}::from_vec_validity("x", {vals}, Some([true, false, true, true])).unwrap();
            let e = polars::{alias}::from_vec("x", []).unwrap();
            let s = fx::series_i64();
            s.append(fx::series_nulls()).unwrap();
            let m = s.cast(polars::DataType::{dtype}()).unwrap().{accessor}().unwrap();
            `${{show(a.to_vec_null_aware())}} | ${{show(b.to_vec_null_aware())}} | ${{show(e.to_vec_null_aware())}} | ${{show(m.to_vec_null_aware())}} | ${{m.chunk_lengths().unwrap().len()}} | ${{show(a.to_vec_null_aware())}}`
        }}
    "#))
}
macro_rules! rust_row {
    ($show:ident, $ca:ty, $vals:expr, $dt:expr, $acc:ident) => {{
        let vals = $vals;
        let a = <$ca>::from_vec("x".into(), vals.clone());
        let b = <$ca>::from_vec_validity("x".into(), vals.clone(), Some(polars_arrow::bitmap::Bitmap::from(MASK)));
        let e = <$ca>::from_vec("x".into(), vec![]);
        let mut s = p::Series::new("x".into(), [1i64, 2, 3]);
        s.append(&p::Series::new("x".into(), [Some(1i64), None, Some(3)])).unwrap();
        let s = s.cast(&$dt).unwrap();
        let m = s.$acc().unwrap();
        let rows = [$show!(a.to_vec_null_aware()), $show!(b.to_vec_null_aware()), $show!(e.to_vec_null_aware()), $show!(m.to_vec_null_aware())];
        let tags: String = rows.iter().map(|r| r.0).collect();
        (tags, format!("{} | {} | {} | {} | {} | {}", rows[0].1, rows[1].1, rows[2].1, rows[3].1, m.chunks().len(), rows[0].1))
    }};
}

#[test]
fn every_numeric_wrapper_matches_both_branches() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let checks: Vec<(&str, String, (String, String))> = vec![
        ("Int8Chunked", rune_row("Int8Chunked", "[-128, -1, 0, 127]", "Int8", "i8"), rust_row!(show_int, p::Int8Chunked, vec![i8::MIN, -1, 0, i8::MAX], p::DataType::Int8, i8)),
        ("Int16Chunked", rune_row("Int16Chunked", "[-32768, -1, 0, 32767]", "Int16", "i16"), rust_row!(show_int, p::Int16Chunked, vec![i16::MIN, -1, 0, i16::MAX], p::DataType::Int16, i16)),
        ("Int32Chunked", rune_row("Int32Chunked", "[-2147483648, -1, 0, 2147483647]", "Int32", "i32"), rust_row!(show_int, p::Int32Chunked, vec![i32::MIN, -1, 0, i32::MAX], p::DataType::Int32, i32)),
        ("Int64Chunked", rune_row("Int64Chunked", "[-9223372036854775808, -1, 0, 9223372036854775807]", "Int64", "i64"), rust_row!(show_int, p::Int64Chunked, vec![i64::MIN, -1, 0, i64::MAX], p::DataType::Int64, i64)),
        ("UInt8Chunked", rune_row("UInt8Chunked", "[0, 1, 2, 255]", "UInt8", "u8"), rust_row!(show_int, p::UInt8Chunked, vec![0u8, 1, 2, u8::MAX], p::DataType::UInt8, u8)),
        ("UInt16Chunked", rune_row("UInt16Chunked", "[0, 1, 2, 65535]", "UInt16", "u16"), rust_row!(show_int, p::UInt16Chunked, vec![0u16, 1, 2, u16::MAX], p::DataType::UInt16, u16)),
        ("IdxCa", rune_row("IdxCa", "[0, 1, 2, 4294967295]", "UInt32", "idx"), rust_row!(show_int, p::IdxCa, vec![0u32, 1, 2, u32::MAX], p::DataType::UInt32, idx)),
        ("UInt64Chunked", rune_row("UInt64Chunked", "[0, 1, 2, 9223372036854775807]", "UInt64", "u64"), rust_row!(show_int, p::UInt64Chunked, vec![0u64, 1, 2, i64::MAX as u64], p::DataType::UInt64, u64)),
        ("Float32Chunked", rune_row("Float32Chunked", "[-0.0, 0.0 / 0.0, 1.0 / 0.0, 0.1]", "Float32", "f32"), rust_row!(show_float, p::Float32Chunked, vec![-0.0f32, f32::NAN, f32::INFINITY, 0.1f64 as f32], p::DataType::Float32, f32)),
        ("Float64Chunked", rune_row("Float64Chunked", "[-0.0, 0.0 / 0.0, -1.0 / 0.0, 0.1]", "Float64", "f64"), rust_row!(show_float, p::Float64Chunked, vec![-0.0f64, f64::NAN, f64::NEG_INFINITY, 0.1], p::DataType::Float64, f64)),
    ];
    for (alias, got, (tags, want)) in &checks {
        assert_eq!(got, want, "{alias}");
        assert_eq!(tags, "LRLR", "{alias}: Left for no nulls and empty, Right for a null, including the multi-chunk receiver");
    }
    // spelled out: a null keeps its position, the two chunks merge in order
    assert_eq!(checks[0].1, "-128,-1,0,127 | -128,n,0,127 |  | 1,2,3,1,n,3 | 2 | -128,-1,0,127");
    assert_eq!(checks[8].1, "-0.0,NaN,inf,0.10000000149011612 | -0.0,n,inf,0.10000000149011612 |  | 1.0,2.0,3.0,1.0,n,3.0 | 2 | -0.0,NaN,inf,0.10000000149011612");
}

#[test]
fn a_u64_beyond_a_script_integer_is_an_error_and_the_receiver_survives() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let got = run(&format!(r#"{SHOW}
        pub fn main() {{
            let left = fx::series_u64_extremes().u64().unwrap();
            let right = fx::series_u64_boundary().u64().unwrap();
            let fine = fx::series_u64().u64().unwrap();
            `${{show(left.to_vec_null_aware())}} | ${{show(right.to_vec_null_aware())}} | ${{show(right.to_vec_null_aware())}} | ${{match right.get(0) {{ Ok(Some(v)) => v, _ => 0 }}}} | ${{show(fine.to_vec_null_aware())}}`
        }}
    "#));
    let rust_left = show_int!(p::Series::new("x".into(), [u64::MAX, 4_294_967_297u64, 1]).u64().unwrap().to_vec_null_aware());
    let rust_right = show_int!(p::Series::new("x".into(), [Some(i64::MAX as u64), Some(i64::MAX as u64 + 1), Some(u64::MAX), None]).u64().unwrap().to_vec_null_aware());
    assert_eq!((rust_left.0, rust_right.0), ('L', 'R'));
    assert_eq!(got, format!(
        "ConversionError: to_vec_null_aware: 18446744073709551615 does not fit a script integer | ConversionError: to_vec_null_aware: 9223372036854775808 does not fit a script integer | ConversionError: to_vec_null_aware: 9223372036854775808 does not fit a script integer | {} | 1,2,3",
        i64::MAX
    ));
    // the whole call fails: Rust holds the full values the script cannot
    assert_eq!(rust_left.1, "ConversionError,4294967297,1");
    assert_eq!(rust_right.1, "9223372036854775807,ConversionError,ConversionError,n");
}

#[test]
fn the_result_is_bounded_before_the_polars_call() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let got = run(&format!(r#"{SHOW}
        pub fn main() {{
            let a = polars::Int64Chunked::from_vec("x", [1, 2, 3]).unwrap();
            let b = polars::Float64Chunked::from_vec_validity("x", [1.0, 2.0, 3.0, 4.0], Some([true, false, true, true])).unwrap();
            let s = fx::series_i64();
            s.append(fx::series_nulls()).unwrap();
            let m = s.i64().unwrap();
            let e = polars::UInt8Chunked::from_vec("x", []).unwrap();
            let out = [];
            polars::set_materialize_limit(2); out.push(show(a.to_vec_null_aware()));
            polars::set_materialize_limit(3); out.push(show(a.to_vec_null_aware())); out.push(show(b.to_vec_null_aware()));
            polars::set_materialize_limit(4); out.push(show(b.to_vec_null_aware()));
            polars::set_materialize_limit(5); out.push(show(m.to_vec_null_aware()));
            polars::set_materialize_limit(6); out.push(show(m.to_vec_null_aware()));
            polars::set_materialize_limit(0); out.push(show(e.to_vec_null_aware())); out.push(show(a.to_vec_null_aware()));
            out.iter().fold("", |acc, x| acc + x + "\n")
        }}
    "#));
    let lines: Vec<&str> = got.lines().collect();
    assert_eq!(lines, [
        "MaterializeLimit: to_vec_null_aware: 3 items, more than the bound of 2",
        "1,2,3",
        "MaterializeLimit: to_vec_null_aware: 4 items, more than the bound of 3",
        "1.0,n,3.0,4.0",
        "MaterializeLimit: to_vec_null_aware: 6 items, more than the bound of 5",
        "1,2,3,1,n,3",
        "",
        "1,2,3",
    ], "{got}");
}
