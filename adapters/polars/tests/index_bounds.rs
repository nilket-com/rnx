//! Record 0091: `convert_and_bound_idx_ca` on the eight integer wrappers,
//! compared with the same direct Rust call; `target_len` is checked below
//! `IdxSize::MAX` before Polars runs.
#![cfg(all(feature = "generated", feature = "test-support"))]
use polars::prelude as p;
use polars::prelude::{convert_and_bound_idx_ca, NamedFrom};
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

/// values and validity: `3` for a valid index 3, `n` for null, `E` for an error
fn show(r: p::PolarsResult<p::IdxCa>) -> String {
    match r {
        Ok(ca) => (0..ca.len()).map(|i| ca.get(i).map_or("n".into(), |v| v.to_string())).collect::<Vec<_>>().join(","),
        Err(_) => "E".into(),
    }
}
const HELPERS: &str = r#"
    fn show(r) { match r { Ok(ca) => { let s = []; let i = 0; while i < ca.len() { s.push(match ca.get(i).unwrap() { Some(v) => `${v}`, None => "n" }); i = i + 1; } s.iter().fold("", |a, b| if a == "" { b } else { a + "," + b }) }, Err(_) => "E" } }
"#;

macro_rules! rust_row {
    ($ca:ty, $n:ty, $signed:expr) => {{
        let vals: Vec<i64> = if $signed { vec![0, 2, 3, -1, -3, -4] } else { vec![0, 2, 3, 5] };
        let ca = <$ca>::from_vec("x".into(), vals.iter().map(|x| *x as $n).collect());
        let nulls = <$ca>::from_vec_validity("x".into(), vec![1 as $n, 9 as $n], Some(polars_arrow::bitmap::Bitmap::from([false, true])));
        [show(convert_and_bound_idx_ca(&ca, 3, true)), show(convert_and_bound_idx_ca(&ca, 3, false)), show(convert_and_bound_idx_ca(&ca, 0, true)), show(convert_and_bound_idx_ca(&nulls, 5, true)), show(convert_and_bound_idx_ca(&nulls, 5, false))].join(" ")
    }};
}

fn rune_row(alias: &str, signed: bool) -> String {
    let vals = if signed { "[0, 2, 3, -1, -3, -4]" } else { "[0, 2, 3, 5]" };
    run(&format!(r#"{HELPERS}
        pub fn main() {{
            let ca = polars::{alias}::from_vec("x", {vals}).unwrap();
            let nulls = polars::{alias}::from_vec_validity("x", [1, 9], Some([false, true])).unwrap();
            let f = |c, n, b| show(polars::{alias}::convert_and_bound_idx_ca(c, n, b));
            let out = `${{f(ca, 3, true)}} ${{f(ca, 3, false)}} ${{f(ca, 0, true)}} ${{f(nulls, 5, true)}} ${{f(nulls, 5, false)}}`;
            if ca.len() > 0 && nulls.null_count() == 1 {{ out }} else {{ "input changed".to_string() }}
        }}
    "#))
}

#[test]
fn every_integer_wrapper_matches_polars_values_and_validity() {
    let rows: Vec<(&str, bool, String)> = vec![
        ("Int8Chunked", true, rust_row!(p::Int8Chunked, i8, true)),
        ("Int16Chunked", true, rust_row!(p::Int16Chunked, i16, true)),
        ("Int32Chunked", true, rust_row!(p::Int32Chunked, i32, true)),
        ("Int64Chunked", true, rust_row!(p::Int64Chunked, i64, true)),
        ("UInt8Chunked", false, rust_row!(p::UInt8Chunked, u8, false)),
        ("UInt16Chunked", false, rust_row!(p::UInt16Chunked, u16, false)),
        ("IdxCa", false, rust_row!(p::IdxCa, u32, false)),
        ("UInt64Chunked", false, rust_row!(p::UInt64Chunked, u64, false)),
    ];
    for (alias, signed, want) in rows {
        assert_eq!(rune_row(alias, signed), want, "{alias}");
    }
    // the shape the comparison relies on: signed indices wrap from the end,
    // out-of-range values are null (or an error without null_on_oob)
    assert_eq!(rust_row!(p::Int64Chunked, i64, true), "0,2,n,2,0,n E n,n,n,n,n,n n,n E");
}

#[test]
fn target_len_is_checked_before_polars_and_extremes_stay_null() {
    let extremes = show(convert_and_bound_idx_ca(p::Series::new("x".into(), [u64::MAX, 4_294_967_297u64, 1]).u64().unwrap(), 2, true));
    let max = p::IdxSize::MAX as i64;
    let got = run(&format!(r#"{HELPERS}
        pub fn main() {{
            let empty = polars::Int8Chunked::from_vec("x", []).unwrap();
            let neg = polars::Int8Chunked::convert_and_bound_idx_ca(empty, -1, true);
            let at = polars::Int8Chunked::convert_and_bound_idx_ca(empty, {max}, false);
            let below = polars::Int8Chunked::convert_and_bound_idx_ca(empty, {below}, false);
            let extremes = show(polars::UInt64Chunked::convert_and_bound_idx_ca(fx::series_u64_extremes().u64().unwrap(), 2, true));
            match (neg, at) {{
                (Err(a), Err(b)) => `${{a.kind()}} ${{b.kind()}} ${{b.message()}} | ${{below.is_ok()}} ${{show(below)}}| ${{extremes}}`,
                _ => "unexpected".to_string(),
            }}
        }}
    "#, below = max - 1));
    assert_eq!(got, format!("ConversionError OutOfBounds convert_and_bound_idx_ca: target_len {max} must be below {max} | true | {extremes}"));
    assert_eq!(extremes, "n,n,1", "u64::MAX and 2^32 + 1 never become valid through the truncating cast");
    let rune_multi = run(&format!(r#"{HELPERS}
        pub fn main() {{
            let base = fx::series_i64().slice(0, 1).unwrap();
            base.append(fx::series_i64().slice(1, 1).unwrap()).unwrap();
            base.append(fx::series_nulls().slice(0, 2).unwrap()).unwrap();
            show(polars::Int64Chunked::convert_and_bound_idx_ca(base.i64().unwrap(), 2, true))
        }}
    "#));
    let rust_multi = {
        let mut b = p::Series::new("x".into(), [1i64]);
        b.append(&p::Series::new("x".into(), [2i64])).unwrap();
        b.append(&p::Series::new("x".into(), [Some(1i64), None])).unwrap();
        show(convert_and_bound_idx_ca(b.i64().unwrap(), 2, true))
    };
    assert_eq!(rune_multi, rust_multi, "a four-chunk receiver with a null");
}
