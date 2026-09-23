//! Record 0102: `downcast_as_array()` on the fourteen wrappers copies the single array, compared with direct Polars; a multi-chunk receiver keeps Polars's assertion panic; the one array is bounded.
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
    fn bytes(v) { let s = "<"; let j = 0; for b in v { if j > 0 { s = s + " "; } s = s + `${b}`; j = j + 1; } s + ">" }
    fn b(v) { `${v}` }
    fn q(v) { `"${v}"` }
    fn show(r, cell) { match r { Ok(inner) => { let s = `${inner.len()}[`; let j = 0; for x in inner { if j > 0 { s = s + ","; } s = s + match x { Some(v) => cell(v), None => "n" }; j = j + 1; } s + "]" }, Err(e) => `${e.kind()}: ${e.message()}` } }
"#;
fn bytes(v: &[u8]) -> String { format!("<{}>", v.iter().map(|b| b.to_string()).collect::<Vec<_>>().join(" ")) }
macro_rules! rust_show {
    ($a:expr, $cell:expr) => {{ let v: Vec<String> = $a.iter().map(|x| x.map_or("n".to_string(), $cell)).collect(); format!("{}[{}]", v.len(), v.join(",")) }};
}
fn int<N: Into<i64> + Copy>(v: &N) -> String { (*v).into().to_string() }
fn flt<N: Into<f64> + Copy>(v: &N) -> String { format!("{:?}", (*v).into()) }

/// One chunk holding [1, 2, 3, 1, null, 3], cast to the type.
fn rechunked(dtype: p::DataType) -> p::Series {
    let mut s = p::Series::new("x".into(), [1i64, 2, 3]);
    s.append(&p::Series::new("x".into(), [Some(1i64), None, Some(3)])).unwrap();
    s.rechunk().cast(&dtype).unwrap()
}
fn recv(dtype: &str, acc: &str) -> String {
    format!("{{ let s = fx::series_i64(); s.append(fx::series_nulls()).unwrap(); s.rechunk().cast(polars::DataType::{dtype}()).unwrap().{acc}().unwrap() }}")
}
fn script(recv: &str, cell: &str) -> String {
    run(&format!(r#"{SHOW}
        pub fn main() {{
            let r = {recv};
            let e = r.limit(0).unwrap();
            let kept = {{ let t = {recv}; t.downcast_as_array() }};
            `${{show(r.downcast_as_array(), {cell})}} ${{show(e.downcast_as_array(), {cell})}} ${{show(kept, {cell})}}`
        }}
    "#))
}
macro_rules! direct {
    ($r:expr, $cell:expr) => {{
        let r = $r;
        assert_eq!(r.chunks().len(), 1);
        let a = rust_show!(r.downcast_as_array(), $cell);
        format!("{a} {} {a}", rust_show!(r.limit(0).downcast_as_array(), $cell))
    }};
}

#[test]
fn all_fourteen_owners_copy_their_single_array() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let checks: Vec<(&str, String, String)> = vec![
        ("Int8", script(&recv("Int8", "i8"), "b"), direct!(rechunked(p::DataType::Int8).i8().unwrap().clone(), |v: &i8| int(v))),
        ("Int16", script(&recv("Int16", "i16"), "b"), direct!(rechunked(p::DataType::Int16).i16().unwrap().clone(), |v: &i16| int(v))),
        ("Int32", script(&recv("Int32", "i32"), "b"), direct!(rechunked(p::DataType::Int32).i32().unwrap().clone(), |v: &i32| int(v))),
        ("Int64", script(&recv("Int64", "i64"), "b"), direct!(rechunked(p::DataType::Int64).i64().unwrap().clone(), |v: &i64| int(v))),
        ("UInt8", script(&recv("UInt8", "u8"), "b"), direct!(rechunked(p::DataType::UInt8).u8().unwrap().clone(), |v: &u8| int(v))),
        ("UInt16", script(&recv("UInt16", "u16"), "b"), direct!(rechunked(p::DataType::UInt16).u16().unwrap().clone(), |v: &u16| int(v))),
        ("IdxCa", script(&recv("UInt32", "idx"), "b"), direct!(rechunked(p::DataType::UInt32).idx().unwrap().clone(), |v: &u32| int(v))),
        ("UInt64", script(&recv("UInt64", "u64"), "b"), direct!(rechunked(p::DataType::UInt64).u64().unwrap().clone(), |v: &u64| i64::try_from(*v).unwrap().to_string())),
        ("Float32", script(&recv("Float32", "f32"), "b"), direct!(rechunked(p::DataType::Float32).f32().unwrap().clone(), |v: &f32| flt(v))),
        ("Float64", script(&recv("Float64", "f64"), "b"), direct!(rechunked(p::DataType::Float64).f64().unwrap().clone(), |v: &f64| flt(v))),
        ("Boolean", script(&recv("Boolean", "bool"), "b"), direct!(rechunked(p::DataType::Boolean).bool().unwrap().clone(), |v: bool| v.to_string())),
        ("String", script("fx::series_str_mixed().rechunk().str().unwrap()", "q"), direct!(values::series_str_mixed().rechunk().str().unwrap().clone(), |v: &str| format!("\"{v}\""))),
        ("Binary", script("fx::series_binary_mixed().rechunk().binary().unwrap()", "bytes"), direct!(values::series_binary_mixed().rechunk().binary().unwrap().clone(), bytes)),
        ("BinaryOffset", script("fx::series_binary_offset_mixed().rechunk().binary_offset().unwrap()", "bytes"), direct!(values::series_binary_offset_mixed().rechunk().binary_offset().unwrap().clone(), bytes)),
    ];
    for (name, got, want) in &checks {
        assert_eq!(got, want, "{name}");
    }
    assert_eq!(checks[3].1, "6[1,2,3,1,n,3] 0[] 6[1,2,3,1,n,3]");
    assert_eq!(checks[11].1, "4[\"é日本\",\"\",n,\"z\"] 0[] 4[\"é日本\",\"\",n,\"z\"]");
    assert_eq!(checks[13].1, "4[<195 40>,<>,n,<0 0 255>] 0[] 4[<195 40>,<>,n,<0 0 255>]");
}

#[test]
fn extremes_floats_and_u64_read_back() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let got = run(&format!(r#"{SHOW}
        pub fn main() {{
            let i = polars::Int8Chunked::from_vec("x", [-128, 127, 0]).unwrap();
            let f = polars::Float32Chunked::from_vec("x", [0.1, -0.0]).unwrap();
            let d = polars::Float64Chunked::from_vec("x", [-0.0, 0.0 / 0.0, 1.0 / 0.0, -1.0 / 0.0]).unwrap();
            let u = fx::series_u64_boundary().u64().unwrap();
            let failed = show(u.downcast_as_array(), b);
            let after = match u.get(0) {{ Ok(Some(v)) => v, _ => 0 }};
            `${{show(i.downcast_as_array(), b)}} ${{show(f.downcast_as_array(), b)}} ${{show(d.downcast_as_array(), b)}} | ${{failed}} | ${{after}}`
        }}
    "#));
    assert_eq!(got, format!("3[-128,127,0] 2[0.10000000149011612,-0.0] 4[-0.0,NaN,inf,-inf] | ConversionError: downcast_as_array: 9223372036854775808 does not fit a script integer | {}", i64::MAX));
}

#[test]
fn a_multi_chunk_receiver_keeps_the_polars_panic() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let message = |e: Box<dyn std::any::Any + Send>| e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
    // direct Rust: two chunks
    let two = { let mut s = p::Series::new("x".into(), [1i64, 2, 3]); s.append(&p::Series::new("x".into(), [4i64])).unwrap(); s.i64().unwrap().clone() };
    assert_eq!(two.chunks().len(), 2);
    let rust = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| { let _ = two.downcast_as_array(); })).map_err(message).unwrap_err();
    // the script: the same receiver shape
    let script = std::panic::catch_unwind(|| run(r#"
        pub fn main() {
            let s = fx::series_i64();
            s.append(fx::series_nulls()).unwrap();
            let r = s.i64().unwrap();
            `${r.downcast_as_array().unwrap().len()}`
        }
    "#)).map_err(message).unwrap_err();
    for m in [&rust, &script] {
        assert!(m.contains("assertion `left == right` failed") && m.contains("left: 2") && m.contains("right: 1"), "Polars's one-chunk assertion, not chunk zero or a MaterializeLimit: {m}");
    }
    // the safe public constructor given no chunks: record what it builds
    let zero = p::Int64Chunked::from_chunk_iter("x".into(), std::iter::empty::<polars_arrow::array::PrimitiveArray<i64>>());
    let zero_chunks = zero.chunks().len();
    let zero_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| zero.downcast_as_array().len())).map_err(message);
    if zero_chunks == 0 {
        assert!(zero_result.unwrap_err().contains("left: 0"), "zero chunks trip the same assertion");
    } else {
        assert_eq!(zero_result.unwrap(), 0, "the safe constructor gives one empty chunk, which is not the panic path");
    }
}

#[test]
fn the_single_array_is_bounded() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    // [1,2,3,1,null,3] in one chunk: 1 + 6 = 7 slots; binary [c3 28, "", null, 00 00 ff]: 1 + 4 + 5 = 10, no value above 3 bytes
    let got = run(&format!(r#"{SHOW}
        pub fn main() {{
            let s = fx::series_i64();
            s.append(fx::series_nulls()).unwrap();
            let r = s.rechunk().i64().unwrap();
            let bin = fx::series_binary_mixed().rechunk().binary().unwrap();
            let e = r.limit(0).unwrap();
            let out = [];
            polars::set_materialize_limit(6); out.push(show(r.downcast_as_array(), b));
            polars::set_materialize_limit(7); out.push(show(r.downcast_as_array(), b));
            polars::set_materialize_limit(9); out.push(show(bin.downcast_as_array(), bytes));
            polars::set_materialize_limit(10); out.push(show(bin.downcast_as_array(), bytes));
            polars::set_materialize_limit(1); out.push(show(e.downcast_as_array(), b));
            polars::set_materialize_limit(0);
            out.iter().fold("", |acc, x| acc + x + "\n")
        }}
    "#));
    assert_eq!(got.lines().collect::<Vec<_>>(), [
        "MaterializeLimit: downcast_as_array: 1 chunks and 6 cells (7 slots), more than the bound of 6",
        "6[1,2,3,1,n,3]",
        "MaterializeLimit: downcast_as_array: 1 chunks, 4 cells and 5 bytes (10 slots), more than the bound of 9",
        "4[<195 40>,<>,n,<0 0 255>]",
        "0[]",
    ], "{got}");
}
