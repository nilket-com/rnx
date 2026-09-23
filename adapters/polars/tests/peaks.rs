//! Record 0090: `peak_max_with_start_end` and `peak_min_with_start_end` on the
//! ten numeric wrappers, each scenario compared with the same direct Rust call.
#![cfg(all(feature = "generated", feature = "test-support"))]
use polars::prelude as p;
use polars::prelude::NamedFrom;
use polars::prelude::peaks::{peak_max_with_start_end, peak_min_with_start_end};
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

fn mask(b: &p::BooleanChunked) -> String {
    (0..b.len()).map(|i| b.get(i)).map(|v| match v { Some(true) => '1', Some(false) => '0', None => 'n' }).collect()
}

const HELPERS: &str = r#"
    fn mask(b) { let s = ""; let i = 0; while i < b.len().unwrap() { s = s + match b.get(i).unwrap() { Some(true) => "1", Some(false) => "0", None => "n" }; i = i + 1; } s }
"#;

/// Rune for one type: every scenario, joined with spaces. `lit` spells a
/// script number of that type's kind (`3` or `3.0`).
/// `u` is `.unwrap()` for types whose boundaries are range-checked into a
/// narrower native (the binding is fallible), empty for i64, f32 and f64.
fn rune_script(alias: &str, lit: fn(i64) -> String) -> String {
    let u = if matches!(alias, "Int64Chunked" | "Float32Chunked" | "Float64Chunked") { "" } else { ".unwrap()" };
    let v = |xs: &[i64]| format!("[{}]", xs.iter().map(|x| lit(*x)).collect::<Vec<_>>().join(", "));
    format!(r#"{HELPERS}
        pub fn main() {{
            let dup = polars::{alias}::from_vec("x", {dup}).unwrap();
            let one = polars::{alias}::from_vec("x", {one}).unwrap();
            let none = polars::{alias}::from_vec("x", []).unwrap();
            let nulls = polars::{alias}::from_vec_validity("x", {nv}, Some([true, false, true, true])).unwrap();
            let out = [
                mask(polars::{alias}::peak_max_with_start_end(dup, None, None){u}),
                mask(polars::{alias}::peak_min_with_start_end(dup, None, None){u}),
                mask(polars::{alias}::peak_max_with_start_end(dup, Some({b0}), Some({b5})){u}),
                mask(polars::{alias}::peak_min_with_start_end(dup, Some({b5}), Some({b0})){u}),
                mask(polars::{alias}::peak_max_with_start_end(one, None, None){u}),
                mask(polars::{alias}::peak_max_with_start_end(one, Some({b0}), Some({b0})){u}),
                mask(polars::{alias}::peak_max_with_start_end(none, None, None){u}),
                mask(polars::{alias}::peak_max_with_start_end(nulls, None, None){u}),
                mask(polars::{alias}::peak_min_with_start_end(nulls, Some({b5}), None){u}),
            ];
            let kept = dup.len().unwrap() == 5 && nulls.null_count().unwrap() == 1 && dup.get(1).unwrap() == Some({three});
            out.iter().fold("", |a, b| a + b + " ") + `${{kept}}`
        }}
    "#, dup = v(&[1, 3, 2, 3, 1]), one = v(&[4]), nv = v(&[1, 0, 3, 1]), b0 = lit(0), b5 = lit(5), three = lit(3), u = u)
}

macro_rules! rust_side {
    ($ca:ty, $n:ty) => {{
        let c = |xs: &[i64]| <$ca>::from_vec("x".into(), xs.iter().map(|x| *x as $n).collect());
        let dup = c(&[1, 3, 2, 3, 1]);
        let one = c(&[4]);
        let none = c(&[]);
        let nulls = <$ca>::from_vec_validity("x".into(), [1i64, 0, 3, 1].iter().map(|x| *x as $n).collect(), Some(polars_arrow::bitmap::Bitmap::from([true, false, true, true])));
        let z = 0i64 as $n;
        let f = 5i64 as $n;
        [
            mask(&peak_max_with_start_end(&dup, None, None)),
            mask(&peak_min_with_start_end(&dup, None, None)),
            mask(&peak_max_with_start_end(&dup, Some(z), Some(f))),
            mask(&peak_min_with_start_end(&dup, Some(f), Some(z))),
            mask(&peak_max_with_start_end(&one, None, None)),
            mask(&peak_max_with_start_end(&one, Some(z), Some(z))),
            mask(&peak_max_with_start_end(&none, None, None)),
            mask(&peak_max_with_start_end(&nulls, None, None)),
            mask(&peak_min_with_start_end(&nulls, Some(f), None)),
        ].iter().fold(String::new(), |a, b| a + b + " ") + "true"
    }};
}

#[test]
fn every_numeric_wrapper_matches_polars() {
    let int = |x: i64| x.to_string();
    let float = |x: i64| format!("{x}.0");
    let rows: Vec<(&str, fn(i64) -> String, String)> = vec![
        ("Int8Chunked", int, rust_side!(p::Int8Chunked, i8)),
        ("Int16Chunked", int, rust_side!(p::Int16Chunked, i16)),
        ("Int32Chunked", int, rust_side!(p::Int32Chunked, i32)),
        ("Int64Chunked", int, rust_side!(p::Int64Chunked, i64)),
        ("UInt8Chunked", int, rust_side!(p::UInt8Chunked, u8)),
        ("UInt16Chunked", int, rust_side!(p::UInt16Chunked, u16)),
        ("IdxCa", int, rust_side!(p::IdxCa, u32)),
        ("UInt64Chunked", int, rust_side!(p::UInt64Chunked, u64)),
        ("Float32Chunked", float, rust_side!(p::Float32Chunked, f32)),
        ("Float64Chunked", float, rust_side!(p::Float64Chunked, f64)),
    ];
    for (alias, lit, want) in rows {
        let got = run(&rune_script(alias, lit));
        assert_eq!(got, want, "{alias}");
    }
}

#[test]
fn multi_chunk_boundaries_and_refusals() {
    // two chunks through Series, compared with Rust; then out-of-range and
    // negative boundaries for unsigned and narrow types are refused, and
    // float boundaries keep NaN and infinities
    let mut s = p::Series::new("x".into(), [1i64, 2, 3]);
    s.append(&p::Series::new("x".into(), [Some(1i64), None, Some(3)])).unwrap();
    let want = format!("{} {}", mask(&peak_max_with_start_end(s.i64().unwrap(), None, None)), mask(&peak_min_with_start_end(s.i64().unwrap(), Some(0), Some(9))));
    let got = run(&format!(r#"{HELPERS}
        pub fn main() {{
            let s = fx::series_i64();
            s.append(fx::series_nulls()).unwrap();
            let ca = s.i64().unwrap();
            `${{mask(polars::Int64Chunked::peak_max_with_start_end(ca, None, None))}} ${{mask(polars::Int64Chunked::peak_min_with_start_end(ca, Some(0), Some(9)))}}`
        }}
    "#));
    assert_eq!(got, want);
    let refusals = run(&format!(r#"{HELPERS}
        pub fn main() {{
            let u = polars::UInt8Chunked::from_vec("x", [1, 2]).unwrap();
            let big = polars::UInt8Chunked::peak_max_with_start_end(u, Some(256), None);
            let neg = polars::IdxCa::peak_max_with_start_end(polars::IdxCa::from_vec("x", [1, 2]).unwrap(), None, Some(-1));
            let i8o = polars::Int8Chunked::peak_min_with_start_end(polars::Int8Chunked::from_vec("x", [1]).unwrap(), Some(128), None);
            let f = polars::Float32Chunked::from_vec("x", [1.0, 2.0, 1.0]).unwrap();
            let nan = mask(polars::Float32Chunked::peak_max_with_start_end(f, Some(0.0 / 0.0), Some(1.0 / 0.0)));
            let ninf = mask(polars::Float32Chunked::peak_max_with_start_end(f, Some(-1.0 / 0.0), Some(-1.0 / 0.0)));
            match (big, neg, i8o) {{
                (Err(a), Err(b), Err(c)) => `${{a.kind()}} ${{b.kind()}} ${{c.kind()}} ${{nan}} ${{ninf}} ${{u.len().unwrap()}}`,
                _ => "unexpected".to_string(),
            }}
        }}
    "#));
    let f = p::Float32Chunked::from_vec("x".into(), vec![1.0, 2.0, 1.0]);
    let want = format!("ConversionError ConversionError ConversionError {} {} 2", mask(&peak_max_with_start_end(&f, Some(f32::NAN), Some(f32::INFINITY))), mask(&peak_max_with_start_end(&f, Some(f32::NEG_INFINITY), Some(f32::NEG_INFINITY))));
    assert_eq!(refusals, want);
}
