//! Record 0098: the six float methods on Float32 and Float64, compared with direct Polars value by value, validity included, and bit for bit where the value is a float.
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

use rnx_polars::generated::fixtures::values;
use rnx_polars::generated::support::float_bits as bits;

macro_rules! script {
    ($ty:literal, $acc:literal) => {
        run(&format!(r#"
            pub fn main() {{
                let r = fx::series_{acc}_specials().{acc}().unwrap();
                let nulls = polars::{ty}::from_vec_validity("x", [1.0, 2.0], Some([false, false])).unwrap();
                let empty = polars::{ty}::from_vec("x", []).unwrap();
                let kept = {{ let t = fx::series_{acc}_specials().{acc}().unwrap(); t.to_canonical() }};
                [r.is_nan(), r.is_not_nan(), r.is_finite(), r.is_infinite(), r.none_to_nan(), r.to_canonical(),
                 nulls.is_nan(), nulls.is_not_nan(), nulls.is_finite(), nulls.is_infinite(), nulls.none_to_nan(), nulls.to_canonical(),
                 empty.is_nan(), empty.none_to_nan(), empty.to_canonical(), kept, r]
            }}
        "#, ty = $ty, acc = $acc))
    };
}

/// Direct Polars results in the script's order: masks as bools, arrays as bits.
macro_rules! direct {
    ($ca:ty, $fixture:ident, $acc:ident, $to_bits:expr) => {{
        let r = values::$fixture().$acc().unwrap().clone();
        let nulls = <$ca>::from_vec_validity("x".into(), vec![1.0, 2.0], Some(polars_arrow::bitmap::Bitmap::from([false, false])));
        let empty = <$ca>::from_vec("x".into(), vec![]);
        let m = |b: p::BooleanChunked| format!("{:?}", b.iter().collect::<Vec<_>>());
        let f = |a: $ca| format!("{:?}", a.iter().map(|x| x.map($to_bits)).collect::<Vec<_>>());
        vec![m(r.is_nan()), m(r.is_not_nan()), m(r.is_finite()), m(r.is_infinite()), f(r.none_to_nan()), f(r.to_canonical()),
             m(nulls.is_nan()), m(nulls.is_not_nan()), m(nulls.is_finite()), m(nulls.is_infinite()), f(nulls.none_to_nan()), f(nulls.to_canonical()),
             m(empty.is_nan()), f(empty.none_to_nan()), f(empty.to_canonical()), f(values::$fixture().$acc().unwrap().to_canonical()), f(r.clone())]
    }};
}
use polars::prelude as p;

/// The script's results read back exactly: masks as bools, floats as bits.
fn read(v: rune::Value, floats: fn(&rune::Value) -> Result<String, String>) -> Vec<String> {
    rune::from_value::<Vec<rune::Value>>(v).unwrap().iter().map(|x| bits::bools(x).map(|b| format!("{b:?}")).or_else(|_| floats(x)).unwrap()).collect()
}
fn f64_text(v: &rune::Value) -> Result<String, String> { bits::f64s(v).map(|b| format!("{b:?}")) }
fn f32_text(v: &rune::Value) -> Result<String, String> { bits::f32s(v).map(|b| format!("{b:?}")) }

const T: Option<bool> = Some(true);
const F: Option<bool> = Some(false);
const N: Option<bool> = None;

#[test]
fn float64_matches_polars_bit_for_bit() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let got = read(script!("Float64Chunked", "f64"), f64_text);
    let want = direct!(p::Float64Chunked, series_f64_specials, f64, f64::to_bits);
    assert_eq!(got, want);
    // [1.5, -0.0, 0.0, NaN(+, payload 1) | null, -inf, inf, NaN(-, payload 0x123), null, -2.5] in two chunks
    assert_eq!(values::series_f64_specials().f64().unwrap().chunks().len(), 2);
    assert_eq!(got[0], format!("{:?}", [F, F, F, T, N, F, F, T, N, F]), "is_nan, null stays null");
    assert_eq!(got[1], format!("{:?}", [T, T, T, F, N, T, T, F, N, T]), "is_not_nan is true on infinities");
    assert_eq!(got[2], format!("{:?}", [T, T, T, F, N, F, F, F, N, T]), "is_finite is false on infinities and NaN");
    assert_eq!(got[3], format!("{:?}", [F, F, F, F, N, T, T, F, N, F]), "is_infinite");
    let nan = f64::NAN.to_bits();
    let input = [Some(1.5f64.to_bits()), Some((-0.0f64).to_bits()), Some(0), Some(0x7ff8_0000_0000_0001), None, Some(f64::NEG_INFINITY.to_bits()), Some(f64::INFINITY.to_bits()), Some(0xfff0_0000_0000_0123), None, Some((-2.5f64).to_bits())];
    assert_eq!(got[16], format!("{input:?}"), "the receiver is unchanged, payloads intact");
    let filled: Vec<_> = input.iter().map(|x| Some(x.unwrap_or(nan))).collect();
    assert_eq!(got[4], format!("{filled:?}"), "none_to_nan: nulls become valid NaN, other bits unchanged");
    let canon = [Some(1.5f64.to_bits()), Some(0), Some(0), Some(0x7ff8_0000_0000_0000), None, Some(f64::NEG_INFINITY.to_bits()), Some(f64::INFINITY.to_bits()), Some(0x7ff8_0000_0000_0000), None, Some((-2.5f64).to_bits())];
    assert_eq!(got[5], format!("{canon:?}"), "to_canonical: -0.0 to +0.0, both NaN payloads to the canonical quiet NaN, nulls kept");
    assert_eq!(got[15], got[5], "a result outlives its dropped receiver");
    assert_eq!(got[6], format!("{:?}", [N, N]), "all-null masks stay null");
    assert_eq!(got[10], format!("{:?}", [Some(nan), Some(nan)]));
    assert_eq!(got[11], format!("{:?}", [None::<u64>, None]));
    assert_eq!(got[12], "[]");
}

#[test]
fn float32_matches_polars_bit_for_bit() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let got = read(script!("Float32Chunked", "f32"), f32_text);
    let want = direct!(p::Float32Chunked, series_f32_specials, f32, f32::to_bits);
    assert_eq!(got, want);
    assert_eq!(got[1], format!("{:?}", [T, T, T, F, N, T, T, F, N, T]));
    assert_eq!(got[2], format!("{:?}", [T, T, T, F, N, F, F, F, N, T]));
    let canon = [Some(1.5f32.to_bits()), Some(0), Some(0), Some(0x7fc0_0000), None, Some(f32::NEG_INFINITY.to_bits()), Some(f32::INFINITY.to_bits()), Some(0x7fc0_0000), None, Some((-2.5f32).to_bits())];
    assert_eq!(got[5], format!("{canon:?}"));
    assert!(got[16].contains(&0x7fc0_0001u32.to_string()) && got[16].contains(&0xff80_0123u32.to_string()), "distinct payloads reach the script's receiver: {}", got[16]);
    assert!(got[4].contains(&0xff80_0123u32.to_string()), "none_to_nan keeps a valid NaN's payload: {}", got[4]);
}
