//! Record 0095: `lhs_div` (`lhs / array`) and `lhs_rem` (`lhs % array`) on
//! the ten numeric wrappers with a scalar of the array's own native type,
//! compared value and validity with direct Rust.
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
/// Integers as a script reads them back: through 0093's checked widening.
fn show<T: p::PolarsNumericType>(ca: &p::ChunkedArray<T>) -> String where T::Native: TryInto<i64> {
    (0..ca.len()).map(|i| ca.get(i).map_or("n".into(), |v| v.try_into().map_or("ConversionError".into(), |v: i64| v.to_string()))).collect::<Vec<_>>().join(",")
}
/// Floats as a script prints them: widened to `f64`, `Debug` formatting,
/// which keeps the sign of zero, `NaN` and `inf`.
fn show_float<T: p::PolarsNumericType>(ca: &p::ChunkedArray<T>) -> String where T::Native: Into<f64> {
    (0..ca.len()).map(|i| ca.get(i).map_or("n".into(), |v| format!("{:?}", v.into()))).collect::<Vec<_>>().join(",")
}

const MASK: [bool; 6] = [true, false, true, true, true, true];

/// One type and operation: `lhs op [values]` for four scalars, the same
/// array with its second slot null, an empty array, and the input unchanged
/// afterwards. `u` unwraps the fallible (range-checked) bindings.
fn rune_row(op: &str, alias: &str, vals: &str, lhs: [&str; 4], u: &str) -> String {
    run(&format!(r#"{HELPERS}
        pub fn main() {{
            let ca = polars::{alias}::from_vec("x", {vals}).unwrap();
            let nulls = polars::{alias}::from_vec_validity("x", {vals}, Some([true, false, true, true, true, true])).unwrap();
            let empty = polars::{alias}::from_vec("x", []).unwrap();
            let out = `${{show(ca.{op}({a}){u})}} | ${{show(ca.{op}({b}){u})}} | ${{show(ca.{op}({c}){u})}} | ${{show(ca.{op}({d}){u})}} | ${{show(nulls.{op}({a}){u})}} | ${{empty.{op}({a}){u}.len().unwrap()}}`;
            `${{out}} | ${{show(ca)}}`
        }}
    "#, a = lhs[0], b = lhs[1], c = lhs[2], d = lhs[3]))
}
macro_rules! rust_row {
    ($show:ident, $op:ident, $ca:ty, $vals:expr, $l:expr) => {{
        let vals = $vals;
        let ca = <$ca>::from_vec("x".into(), vals.clone());
        let nulls = <$ca>::from_vec_validity("x".into(), vals.clone(), Some(polars_arrow::bitmap::Bitmap::from(MASK)));
        let empty = <$ca>::from_vec("x".into(), vec![]);
        let l = $l;
        format!("{} | {} | {} | {} | {} | {} | {}", $show(&ca.$op(l[0])), $show(&ca.$op(l[1])), $show(&ca.$op(l[2])), $show(&ca.$op(l[3])), $show(&nulls.$op(l[0])), empty.$op(l[0]).len(), $show(&ca))
    }};
}
macro_rules! both_ops {
    ($checks:ident, $name:literal, $show:ident, $ca:ty, $rvals:literal, $vals:expr, $rl:expr, $l:expr, $u:literal) => {
        $checks.push((concat!($name, " lhs_div"), rune_row("lhs_div", $name, $rvals, $rl, $u), rust_row!($show, lhs_div, $ca, $vals, $l)));
        $checks.push((concat!($name, " lhs_rem"), rune_row("lhs_rem", $name, $rvals, $rl, $u), rust_row!($show, lhs_rem, $ca, $vals, $l)));
    };
}

#[test]
fn every_numeric_wrapper_matches_polars_values_and_validity() {
    let mut checks: Vec<(&str, String, String)> = vec![];
    both_ops!(checks, "Int8Chunked", show, p::Int8Chunked, "[0, 2, -3, -1, 127, -128]", vec![0i8, 2, -3, -1, 127, -128], ["7", "-7", "-128", "0"], [7i8, -7, i8::MIN, 0], ".unwrap()");
    both_ops!(checks, "Int16Chunked", show, p::Int16Chunked, "[0, 2, -3, -1, 32767, -32768]", vec![0i16, 2, -3, -1, i16::MAX, i16::MIN], ["7", "-7", "-32768", "0"], [7i16, -7, i16::MIN, 0], ".unwrap()");
    both_ops!(checks, "Int32Chunked", show, p::Int32Chunked, "[0, 2, -3, -1, 2147483647, -2147483648]", vec![0i32, 2, -3, -1, i32::MAX, i32::MIN], ["7", "-7", "-2147483648", "0"], [7i32, -7, i32::MIN, 0], ".unwrap()");
    both_ops!(checks, "Int64Chunked", show, p::Int64Chunked, "[0, 2, -3, -1, 9223372036854775807, -9223372036854775808]", vec![0i64, 2, -3, -1, i64::MAX, i64::MIN], ["7", "-7", "-9223372036854775808", "0"], [7i64, -7, i64::MIN, 0], "");
    both_ops!(checks, "UInt8Chunked", show, p::UInt8Chunked, "[0, 1, 2, 3, 255, 7]", vec![0u8, 1, 2, 3, 255, 7], ["7", "255", "0", "1"], [7u8, 255, 0, 1], ".unwrap()");
    both_ops!(checks, "UInt16Chunked", show, p::UInt16Chunked, "[0, 1, 2, 3, 65535, 7]", vec![0u16, 1, 2, 3, u16::MAX, 7], ["7", "65535", "0", "1"], [7u16, u16::MAX, 0, 1], ".unwrap()");
    both_ops!(checks, "IdxCa", show, p::IdxCa, "[0, 1, 2, 3, 4294967295, 7]", vec![0u32, 1, 2, 3, u32::MAX, 7], ["7", "4294967295", "0", "1"], [7u32, u32::MAX, 0, 1], ".unwrap()");
    both_ops!(checks, "UInt64Chunked", show, p::UInt64Chunked, "[0, 1, 2, 3, 9223372036854775807, 7]", vec![0u64, 1, 2, 3, i64::MAX as u64, 7], ["7", "9223372036854775807", "0", "1"], [7u64, i64::MAX as u64, 0, 1], ".unwrap()");
    both_ops!(checks, "Float32Chunked", show_float, p::Float32Chunked, "[2.0, -3.0, 0.0, -0.0, 0.0 / 0.0, 1.0 / 0.0]", vec![2.0f32, -3.0, 0.0, -0.0, f32::NAN, f32::INFINITY], ["7.0", "-0.0", "0.0 / 0.0", "-1.0 / 0.0"], [7.0f32, -0.0, f32::NAN, f32::NEG_INFINITY], "");
    both_ops!(checks, "Float64Chunked", show_float, p::Float64Chunked, "[2.0, -3.0, 0.0, -0.0, 0.0 / 0.0, 1.0 / 0.0]", vec![2.0f64, -3.0, 0.0, -0.0, f64::NAN, f64::INFINITY], ["7.0", "-0.0", "0.0 / 0.0", "-1.0 / 0.0"], [7.0f64, -0.0, f64::NAN, f64::NEG_INFINITY], "");
    assert_eq!(checks.len(), 20);
    for (what, got, want) in &checks {
        assert_eq!(got, want, "{what}");
    }
    // what the comparison covers, spelled out: an integer zero denominator
    // is null, MIN / -1 wraps, floor semantics, float zero is IEEE
    let int8 = |op: &str| checks.iter().find(|c| c.0 == format!("Int8Chunked {op}")).unwrap().2.clone();
    assert!(int8("lhs_div").starts_with("n,3,-3,-7,0,-1 | n,-4,2,7,-1,0 | n,-64,42,-128,-2,1 | n,0,0,0,0,0"), "{}", int8("lhs_div"));
    assert!(int8("lhs_rem").starts_with("n,1,-2,0,7,-121 | n,1,-1,0,120,-7 | n,0,-2,0,126,0 | n,0,0,0,0,0"), "{}", int8("lhs_rem"));
    let f64s = |op: &str| checks.iter().find(|c| c.0 == format!("Float64Chunked {op}")).unwrap().2.clone();
    assert!(f64s("lhs_div").starts_with("3.5,-2.3333333333333335,inf,-inf,NaN,0.0 | -0.0,0.0,NaN,NaN,NaN,-0.0"), "{}", f64s("lhs_div"));
    assert!(f64s("lhs_rem").starts_with("1.0,-2.0,NaN,NaN,NaN,NaN"), "{}", f64s("lhs_rem"));
}

#[test]
fn a_multi_chunk_receiver_matches() {
    let got = run(r#"
        fn show(ca) { let s = []; let i = 0; while i < ca.len().unwrap() { s.push(match ca.get(i).unwrap() { Some(v) => `${v}`, None => "n" }); i = i + 1; } s.iter().fold("", |a, b| if a == "" { b } else { a + "," + b }) }
        pub fn main() {
            let s = fx::series_i64();
            s.append(fx::series_nulls()).unwrap();
            let ca = s.i64().unwrap();
            `${ca.chunk_lengths().unwrap().len()} ${show(ca.lhs_div(7))} ${show(ca.lhs_rem(-7))}`
        }
    "#);
    let mut s = p::Series::new("x".into(), [1i64, 2, 3]);
    s.append(&p::Series::new("x".into(), [Some(1i64), None, Some(3)])).unwrap();
    let ca = s.i64().unwrap();
    assert_eq!(got, format!("2 {} {}", show(&ca.lhs_div(7i64)), show(&ca.lhs_rem(-7i64))));
    assert_eq!(got, "2 7,3,2,7,n,2 0,1,2,0,n,2", "floor modulo takes the sign of the divisor");
}

#[test]
fn out_of_range_scalars_are_refused_and_the_receiver_survives() {
    let got = run(&format!(r#"{HELPERS}
        pub fn main() {{
            let i = polars::Int8Chunked::from_vec("x", [1, 0, -1]).unwrap();
            let u = polars::UInt8Chunked::from_vec("x", [1, 0, 2]).unwrap();
            let x = polars::IdxCa::from_vec("x", [1, 0, 2]).unwrap();
            let w = polars::UInt64Chunked::from_vec("x", [1, 0, 2]).unwrap();
            let refused = [i.lhs_div(128), i.lhs_rem(-129), u.lhs_div(-1), u.lhs_rem(256), x.lhs_div(4294967296), w.lhs_rem(-1)];
            let kinds = refused.iter().fold("", |a, r| a + match r {{ Err(e) => e.kind(), Ok(_) => "ok" }} + " ");
            `${{kinds}}| ${{show(i)}} ${{show(u)}} ${{show(x)}} ${{show(w)}} | ${{show(i.lhs_div(-128).unwrap())}}`
        }}
    "#));
    assert_eq!(got, format!("{}| 1,0,-1 1,0,2 1,0,2 1,0,2 | -128,n,-128", "ConversionError ".repeat(6)));
}
