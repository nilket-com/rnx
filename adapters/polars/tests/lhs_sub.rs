//! Record 0092: `lhs_sub` (`lhs - array`) on the ten numeric wrappers with a
//! scalar of the array's own native type, compared with direct Rust.
#![cfg(all(feature = "generated", feature = "test-support"))]
use polars::prelude as p;
use polars::prelude::NamedFrom;
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
    match vm.call(["main"], ()) { Ok(v) => rune::from_value(v).unwrap(), Err(e) => panic!("{e}") }
}

const HELPERS: &str = r#"
    fn show(ca) { let s = []; let i = 0; while i < ca.len().unwrap() { s.push(match ca.get(i) { Ok(Some(v)) => `${v}`, Ok(None) => "n", Err(e) => e.kind() }); i = i + 1; } s.iter().fold("", |a, b| if a == "" { b } else { a + "," + b }) }
"#;
fn show<T: p::PolarsNumericType>(ca: &p::ChunkedArray<T>) -> String where T::Native: std::fmt::Display {
    (0..ca.len()).map(|i| ca.get(i).map_or("n".into(), |v| v.to_string())).collect::<Vec<_>>().join(",")
}
/// Floats as a script prints them: widened to `f64`, `Debug` formatting.
fn show_float<T: p::PolarsNumericType>(ca: &p::ChunkedArray<T>) -> String where T::Native: Into<f64> {
    (0..ca.len()).map(|i| ca.get(i).map_or("n".into(), |v| format!("{:?}", v.into()))).collect::<Vec<_>>().join(",")
}
macro_rules! rust_float_row {
    ($ca:ty, $vals:expr, $l:expr) => {{
        let vals = $vals;
        let ca = <$ca>::from_vec("x".into(), vals.clone());
        let nulls = <$ca>::from_vec_validity("x".into(), vals.clone(), Some(polars_arrow::bitmap::Bitmap::from([true, false, true])));
        let empty = <$ca>::from_vec("x".into(), vec![]);
        let l = $l;
        format!("{} {} {} {} {} {}", show_float(&ca.lhs_sub(l[0])), show_float(&ca.lhs_sub(l[1])), show_float(&ca.lhs_sub(l[2])), show_float(&nulls.lhs_sub(l[0])), empty.lhs_sub(l[0]).len(), show_float(&ca))
    }};
}
/// A `UInt64Chunked` as the script reads it back: since record 0093 the
/// generated `get` widens `u64` with a range check, so a value above
/// `i64::MAX` is a `ConversionError` rather than a wrapped negative.
fn show_u64_as_script(ca: &p::UInt64Chunked) -> String {
    (0..ca.len()).map(|i| ca.get(i).map_or("n".into(), |v| i64::try_from(v).map_or("ConversionError".into(), |v| v.to_string()))).collect::<Vec<_>>().join(",")
}

/// One type: `lhs - [values]` for three scalars, a nullable array, an empty
/// array, and the input unchanged afterwards. `u` unwraps the fallible
/// (range-checked) bindings.
fn rune_row(alias: &str, vals: &str, lhs: [&str; 3], u: &str) -> String {
    run(&format!(r#"{HELPERS}
        pub fn main() {{
            let ca = polars::{alias}::from_vec("x", {vals}).unwrap();
            let nulls = polars::{alias}::from_vec_validity("x", {vals}, Some([true, false, true])).unwrap();
            let empty = polars::{alias}::from_vec("x", []).unwrap();
            let out = `${{show(ca.lhs_sub({a}){u})}} ${{show(ca.lhs_sub({b}){u})}} ${{show(ca.lhs_sub({c}){u})}} ${{show(nulls.lhs_sub({a}){u})}} ${{empty.lhs_sub({a}){u}.len().unwrap()}}`;
            `${{out}} ${{show(ca)}}`
        }}
    "#, a = lhs[0], b = lhs[1], c = lhs[2]))
}
macro_rules! rust_row {
    ($ca:ty, $vals:expr, $l:expr) => {{
        let vals = $vals;
        let ca = <$ca>::from_vec("x".into(), vals.clone());
        let nulls = <$ca>::from_vec_validity("x".into(), vals.clone(), Some(polars_arrow::bitmap::Bitmap::from([true, false, true])));
        let empty = <$ca>::from_vec("x".into(), vec![]);
        let l = $l;
        format!("{} {} {} {} {} {}", show(&ca.lhs_sub(l[0])), show(&ca.lhs_sub(l[1])), show(&ca.lhs_sub(l[2])), show(&nulls.lhs_sub(l[0])), empty.lhs_sub(l[0]).len(), show(&ca))
    }};
}

#[test]
fn every_numeric_wrapper_matches_polars_including_wraparound() {
    let checks: Vec<(&str, String, String)> = vec![
        ("Int8Chunked", rune_row("Int8Chunked", "[1, -128, 127]", ["0", "-1", "127"], ".unwrap()"), rust_row!(p::Int8Chunked, vec![1i8, -128, 127], [0i8, -1, 127])),
        ("Int16Chunked", rune_row("Int16Chunked", "[1, -32768, 32767]", ["0", "-1", "32767"], ".unwrap()"), rust_row!(p::Int16Chunked, vec![1i16, i16::MIN, i16::MAX], [0i16, -1, i16::MAX])),
        ("Int32Chunked", rune_row("Int32Chunked", "[1, -2147483648, 2147483647]", ["0", "-1", "2147483647"], ".unwrap()"), rust_row!(p::Int32Chunked, vec![1i32, i32::MIN, i32::MAX], [0i32, -1, i32::MAX])),
        ("Int64Chunked", rune_row("Int64Chunked", "[1, -9223372036854775808, 9223372036854775807]", ["0", "-1", "9223372036854775807"], ""), rust_row!(p::Int64Chunked, vec![1i64, i64::MIN, i64::MAX], [0i64, -1, i64::MAX])),
        ("UInt8Chunked", rune_row("UInt8Chunked", "[1, 0, 255]", ["0", "1", "255"], ".unwrap()"), rust_row!(p::UInt8Chunked, vec![1u8, 0, 255], [0u8, 1, 255])),
        ("UInt16Chunked", rune_row("UInt16Chunked", "[1, 0, 65535]", ["0", "1", "65535"], ".unwrap()"), rust_row!(p::UInt16Chunked, vec![1u16, 0, u16::MAX], [0u16, 1, u16::MAX])),
        ("IdxCa", rune_row("IdxCa", "[1, 0, 4294967295]", ["0", "1", "4294967295"], ".unwrap()"), rust_row!(p::IdxCa, vec![1u32, 0, u32::MAX], [0u32, 1, u32::MAX])),
        ("UInt64Chunked", rune_row("UInt64Chunked", "[1, 0, 9223372036854775807]", ["0", "1", "9223372036854775807"], ".unwrap()"), {
            let v = vec![1u64, 0, i64::MAX as u64];
            let ca = p::UInt64Chunked::from_vec("x".into(), v.clone());
            let nulls = p::UInt64Chunked::from_vec_validity("x".into(), v, Some(polars_arrow::bitmap::Bitmap::from([true, false, true])));
            format!("{} {} {} {} 0 {}", show_u64_as_script(&ca.lhs_sub(0u64)), show_u64_as_script(&ca.lhs_sub(1u64)), show_u64_as_script(&ca.lhs_sub(i64::MAX as u64)), show_u64_as_script(&nulls.lhs_sub(0u64)), show_u64_as_script(&ca))
        }),
        ("Float32Chunked", rune_row("Float32Chunked", "[1.5, -0.0, 3.0]", ["0.0", "0.1", "1.0 / 0.0"], ""), rust_float_row!(p::Float32Chunked, vec![1.5f32, -0.0, 3.0], [0.0f32, 0.1f64 as f32, f32::INFINITY])),
        ("Float64Chunked", rune_row("Float64Chunked", "[1.5, -0.0, 3.0]", ["0.0", "0.1", "0.0 / 0.0"], ""), rust_float_row!(p::Float64Chunked, vec![1.5f64, -0.0, 3.0], [0.0f64, 0.1, f64::NAN])),
    ];
    for (alias, got, want) in checks {
        assert_eq!(got, want, "{alias}");
    }
    // the wraparound the comparison covers, spelled out for Int8 and UInt8
    assert_eq!(rust_row!(p::Int8Chunked, vec![1i8, -128, 127], [0i8, -1, 127]).split(' ').next().unwrap(), "-1,-128,-127");
    assert_eq!(rust_row!(p::UInt8Chunked, vec![1u8, 0, 255], [0u8, 1, 255]).split(' ').next().unwrap(), "255,0,1");
}

#[test]
fn out_of_range_scalars_are_refused_and_the_receiver_survives() {
    let max = {
        let s = p::Series::new("x".into(), [u64::MAX, 4_294_967_297u64, 1]);
        show(&s.u64().unwrap().lhs_sub(5u64))
    };
    let got = run(&format!(r#"{HELPERS}
        pub fn main() {{
            let i = polars::Int8Chunked::from_vec("x", [1, 2]).unwrap();
            let u = polars::UInt8Chunked::from_vec("x", [1, 2]).unwrap();
            let x = polars::IdxCa::from_vec("x", [1, 2]).unwrap();
            let refused = [i.lhs_sub(128), i.lhs_sub(-129), u.lhs_sub(-1), u.lhs_sub(256), x.lhs_sub(-1), x.lhs_sub(4294967296)];
            let kinds = refused.iter().fold("", |a, r| a + match r {{ Err(e) => e.kind(), Ok(_) => "ok" }} + " ");
            let max = show(polars::UInt64Chunked::lhs_sub(fx::series_u64_extremes().u64().unwrap(), 5).unwrap());
            let multi = {{ let s = fx::series_i64(); s.append(fx::series_nulls()).unwrap(); show(s.i64().unwrap().lhs_sub(10)) }};
            `${{kinds}}| ${{show(i)}} ${{show(u)}} ${{show(x)}} | ${{max}} | ${{multi}}`
        }}
    "#));
    let multi = {
        let mut s = p::Series::new("x".into(), [1i64, 2, 3]);
        s.append(&p::Series::new("x".into(), [Some(1i64), None, Some(3)])).unwrap();
        show(&s.i64().unwrap().lhs_sub(10i64))
    };
    let conv = "ConversionError ";
    let max_script = "6,ConversionError,4";
    assert_eq!(got, format!("{}| 1,2 1,2 1,2 | {max_script} | {multi}", conv.repeat(6)));
    assert_eq!(max, "6,18446744069414584324,4", "u64 wraparound from 5 - u64::MAX, as Polars computes it");
    assert_eq!(max_script, "6,ConversionError,4", "record 0093: the script's read-back of 18446744069414584324 is a ConversionError, not a wrapped -4294967292");
}
