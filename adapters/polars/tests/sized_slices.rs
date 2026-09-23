//! Record 0097: `limit`, `head` and `tail` on all sixteen ChunkedArray families, compared with direct Polars on multi-chunk, empty and edge inputs.
#![cfg(all(feature = "generated", feature = "test-support"))]
use rnx::rune::{self, Context, Module, Source, Sources, Vm};
use std::sync::Arc;
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn run(script: &str) -> rune::Value {
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
    vm.call(["main"], ()).unwrap()
}

use polars::prelude as p;
use polars::prelude::{IntoSeries, NamedFrom};
use rnx_polars::generated::fixtures::{self as fx, values};

fn text(s: &p::Series) -> String { rnx_polars::oracle::series_repr(s).to_text() }

/// Each script result is a `Result` of a wrapper; shown through the
/// fixtures' helper (which renders the Polars value as a series), or the
/// error kind.
fn shown(v: rune::Value, show: fn(&rune::Value) -> Result<rnx_polars::oracle::Repr, String>) -> String {
    match rune::from_value::<Result<rune::Value, rune::Value>>(v).unwrap() {
        Ok(w) => show(&w).unwrap().to_text(),
        Err(e) => rnx_polars::oracle::rune_error_kind(&e).unwrap(),
    }
}

/// The operations, in the order the script applies them: on the receiver
/// `r` of length `n`, then on an empty one, then a result that outlives a
/// dropped receiver, then three negative lengths, then `n - 1`.
const OPS: &str = "[r.limit(0), r.limit(1), r.limit(n), r.limit(n + 5), r.head(None), r.head(Some(0)), r.head(Some(1)), r.head(Some(n + 5)), r.tail(None), r.tail(Some(0)), r.tail(Some(1)), r.tail(Some(n + 5)), e.head(Some(2)), e.tail(None), e.limit(3), kept, r.limit(-1), r.head(Some(-1)), r.tail(Some(-5)), r.head(Some(n - 1))]";

/// `recv` builds the receiver; `fresh` builds a separate one that is
/// dropped before its `tail(2)` result is shown.
fn rune_rows(recv: &str, fresh: &str, show: fn(&rune::Value) -> Result<rnx_polars::oracle::Repr, String>) -> Vec<String> {
    let v = run(&format!(r#"
        pub fn main() {{
            let r = {recv};
            let n = r.len().unwrap();
            let e = r.limit(0).unwrap();
            let kept = {{ let tr = {fresh}; tr.tail(Some(2)) }};
            {OPS}
        }}
    "#));
    rune::from_value::<Vec<rune::Value>>(v).unwrap().into_iter().map(|x| shown(x, show)).collect()
}

macro_rules! rust_rows {
    ($recv:expr, $fresh:expr) => {{
        let r = $recv;
        let n = r.len();
        let e = r.limit(0);
        let kept = { let tr = $fresh; tr.tail(Some(2)) };
        let t = |ca: p::ChunkedArray<_>| text(&ca.into_series());
        let mut out = vec![t(r.limit(0)), t(r.limit(1)), t(r.limit(n)), t(r.limit(n + 5)), t(r.head(None)), t(r.head(Some(0))), t(r.head(Some(1))), t(r.head(Some(n + 5))), t(r.tail(None)), t(r.tail(Some(0))), t(r.tail(Some(1))), t(r.tail(Some(n + 5))), t(e.head(Some(2))), t(e.tail(None)), t(e.limit(3)), t(kept)];
        out.extend(["ConversionError".to_string(), "ConversionError".into(), "ConversionError".into()]);
        out.push(t(r.head(Some(n - 1))));
        out
    }};
}

/// A two-chunk receiver: the fixture series appended to itself.
macro_rules! family {
    ($checks:ident, $name:literal, $fixture:ident, $acc:ident, $show:path) => {{
        let f = concat!("fx::", stringify!($fixture), "()");
        let recv = format!("{{ let s = {f}; s.append({f}).unwrap(); s.{}().unwrap() }}", stringify!($acc));
        let fresh = format!("{f}.{}().unwrap()", stringify!($acc));
        let rust = rust_rows!({ let mut s = values::$fixture(); s.append(&values::$fixture()).unwrap(); let r = s.$acc().unwrap().clone(); assert_eq!(r.chunks().len(), 2, "a two-chunk receiver"); r }, values::$fixture().$acc().unwrap().clone());
        $checks.push(($name, rune_rows(&recv, &fresh, $show), rust));
    }};
}

#[test]
fn all_sixteen_families_match_polars() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut checks: Vec<(&str, Vec<String>, Vec<String>)> = vec![];
    family!(checks, "BooleanChunked", series_bool, bool, fx::show_w_polars_core__datatypes__booleanchunked);
    family!(checks, "StringChunked", series_str, str, fx::show_w_polars_core__datatypes__stringchunked);
    family!(checks, "BinaryChunked", series_binary, binary, fx::show_w_polars_core__datatypes__binarychunked);
    family!(checks, "BinaryOffsetChunked", series_binary_offset, binary_offset, fx::show_w_polars_core__datatypes__binaryoffsetchunked);
    family!(checks, "Int8Chunked", series_i8, i8, fx::show_w_polars_core__datatypes__int8chunked);
    family!(checks, "Int16Chunked", series_i16, i16, fx::show_w_polars_core__datatypes__int16chunked);
    family!(checks, "Int32Chunked", series_i32, i32, fx::show_w_polars_core__datatypes__int32chunked);
    family!(checks, "Int64Chunked", series_i64, i64, fx::show_w_polars_core__datatypes__int64chunked);
    family!(checks, "UInt8Chunked", series_u8, u8, fx::show_w_polars_core__datatypes__uint8chunked);
    family!(checks, "UInt16Chunked", series_u16, u16, fx::show_w_polars_core__datatypes__uint16chunked);
    family!(checks, "IdxCa", series_u32, idx, fx::show_w_polars_core__datatypes__aliases__idxca);
    family!(checks, "UInt64Chunked", series_u64, u64, fx::show_w_polars_core__datatypes__uint64chunked);
    family!(checks, "Float32Chunked", series_f32, f32, fx::show_w_polars_core__datatypes__float32chunked);
    family!(checks, "Float64Chunked", series_f64, f64, fx::show_w_polars_core__datatypes__float64chunked);
    family!(checks, "ListChunked", series_list, list, fx::show_w_polars_core__datatypes__listchunked);
    family!(checks, "StructChunked", series_struct, struct_, fx::show_w_polars_core__chunked_array__struct___structchunked);
    assert_eq!(checks.len(), 16);
    for (name, got, want) in &checks {
        assert_eq!(got.len(), 20, "{name}");
        for (i, (g, w)) in got.iter().zip(want).enumerate() {
            assert_eq!(g, w, "{name} op {i}");
        }
    }
}

#[test]
fn nulls_extremes_and_floats() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    // [1, 2, 3] ++ [1, null, 3]: the null sits in the second chunk
    let recv = "{ let s = fx::series_i64(); s.append(fx::series_nulls()).unwrap(); s.i64().unwrap() }";
    let build = || { let mut s = p::Series::new("x".into(), [1i64, 2, 3]); s.append(&p::Series::new("x".into(), [Some(1i64), None, Some(3)])).unwrap(); s.i64().unwrap().clone() };
    let want = rust_rows!(build(), build());
    assert_eq!(rune_rows(recv, recv, fx::show_w_polars_core__datatypes__int64chunked), want);
    assert!(want[8].contains("Null"), "tail(None) keeps the second chunk's null: {}", want[8]);
    // signed extremes
    let i8s = "polars::Int8Chunked::from_vec(\"x\", [-128, 127, 0, -1]).unwrap()";
    let want = rust_rows!(p::Int8Chunked::from_vec("x".into(), vec![i8::MIN, i8::MAX, 0, -1]), p::Int8Chunked::from_vec("x".into(), vec![i8::MIN, i8::MAX, 0, -1]));
    assert_eq!(rune_rows(i8s, i8s, fx::show_w_polars_core__datatypes__int8chunked), want);
    // float sign of zero, NaN and infinities
    let f64s = "polars::Float64Chunked::from_vec(\"x\", [-0.0, 0.0 / 0.0, 1.0 / 0.0, -1.0 / 0.0]).unwrap()";
    let fv = || p::Float64Chunked::from_vec("x".into(), vec![-0.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY]);
    let want = rust_rows!(fv(), fv());
    assert_eq!(rune_rows(f64s, f64s, fx::show_w_polars_core__datatypes__float64chunked), want);
    let f32s = "polars::Float32Chunked::from_vec(\"x\", [-0.0, 0.0 / 0.0, 1.0 / 0.0, 0.1]).unwrap()";
    let gv = || p::Float32Chunked::from_vec("x".into(), vec![-0.0f32, f32::NAN, f32::INFINITY, 0.1f64 as f32]);
    let want = rust_rows!(gv(), gv());
    assert_eq!(rune_rows(f32s, f32s, fx::show_w_polars_core__datatypes__float32chunked), want);
    // a UInt64 above i64::MAX stays a Polars value: slicing never reads it back
    let u = "fx::series_u64_boundary().u64().unwrap()";
    let uv = || values::series_u64_boundary().u64().unwrap().clone();
    let want = rust_rows!(uv(), uv());
    assert_eq!(rune_rows(u, u, fx::show_w_polars_core__datatypes__uint64chunked), want);
    assert!(want[3].contains("18446744073709551615"), "{}", want[3]);
}

#[test]
fn negative_lengths_are_refused_and_the_receiver_is_unchanged() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let v = run(r#"
        pub fn main() {
            let r = fx::series_i64().i64().unwrap();
            let refused = [r.limit(-1), r.head(Some(-1)), r.tail(Some(-9223372036854775808))];
            [refused[0], refused[1], refused[2], r.limit(3)]
        }
    "#);
    let out: Vec<String> = rune::from_value::<Vec<rune::Value>>(v).unwrap().into_iter().map(|x| shown(x, fx::show_w_polars_core__datatypes__int64chunked)).collect();
    assert_eq!(out[..3], ["ConversionError", "ConversionError", "ConversionError"]);
    assert_eq!(out[3], text(&values::series_i64()), "the receiver is unchanged");
}
