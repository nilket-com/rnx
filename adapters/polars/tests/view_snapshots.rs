//! Record 0104: `downcast_chunks()` on the fourteen wrappers, read by index into owned nested vectors, compared with direct Polars `len()`/`get(i)` and bounded as a whole.
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
    fn show(r, cell) { match r { Ok(outer) => { let s = `${outer.len()}[`; let i = 0; for inner in outer { if i > 0 { s = s + "|"; } let j = 0; for x in inner { if j > 0 { s = s + ","; } s = s + match x { Some(v) => cell(v), None => "n" }; j = j + 1; } i = i + 1; } s + "]" }, Err(e) => `${e.kind()}: ${e.message()}` } }
"#;
fn bytes(v: &[u8]) -> String { format!("<{}>", v.iter().map(|b| b.to_string()).collect::<Vec<_>>().join(" ")) }
/// The script's rendering of a direct `downcast_chunks` view, read by index.
macro_rules! rust_show {
    ($ca:expr, $cell:expr) => {{
        let ca = &$ca;
        let view = ca.downcast_chunks();
        let parts: Vec<String> = (0..view.len()).map(|i| view.get(i).unwrap().iter().map(|x| x.map_or("n".to_string(), $cell)).collect::<Vec<_>>().join(",")).collect();
        assert!(view.get(view.len()).is_none(), "past the end the view has no chunk");
        format!("{}[{}]", parts.len(), parts.join("|"))
    }};
}
fn int<N: Into<i64> + Copy>(v: &N) -> String { (*v).into().to_string() }
fn flt<N: Into<f64> + Copy>(v: &N) -> String { format!("{:?}", (*v).into()) }
fn two_chunks(dtype: p::DataType) -> p::Series {
    let mut s = p::Series::new("x".into(), [1i64, 2, 3]);
    s.append(&p::Series::new("x".into(), [Some(1i64), None, Some(3)])).unwrap();
    s.cast(&dtype).unwrap()
}
fn recv(dtype: &str, acc: &str) -> String {
    format!("{{ let s = fx::series_i64(); s.append(fx::series_nulls()).unwrap(); s.cast(polars::DataType::{dtype}()).unwrap().{acc}().unwrap() }}")
}
fn script(recv: &str, cell: &str) -> String {
    run(&format!(r#"{SHOW}
        pub fn main() {{
            let r = {recv};
            let e = r.limit(0).unwrap();
            let kept = {{ let t = {recv}; t.downcast_chunks() }};
            `${{show(r.downcast_chunks(), {cell})}} ${{show(e.downcast_chunks(), {cell})}} ${{show(kept, {cell})}} ${{show(r.downcast_chunks(), {cell})}}`
        }}
    "#))
}
macro_rules! direct {
    ($r:expr, $cell:expr) => {{
        let r = $r;
        let a = rust_show!(r, $cell);
        format!("{a} {} {a} {a}", rust_show!(r.limit(0), $cell))
    }};
}

#[test]
fn all_fourteen_owners_read_their_chunks_by_index() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let checks: Vec<(&str, String, String)> = vec![
        ("Int8", script(&recv("Int8", "i8"), "b"), direct!(two_chunks(p::DataType::Int8).i8().unwrap().clone(), |v: &i8| int(v))),
        ("Int16", script(&recv("Int16", "i16"), "b"), direct!(two_chunks(p::DataType::Int16).i16().unwrap().clone(), |v: &i16| int(v))),
        ("Int32", script(&recv("Int32", "i32"), "b"), direct!(two_chunks(p::DataType::Int32).i32().unwrap().clone(), |v: &i32| int(v))),
        ("Int64", script(&recv("Int64", "i64"), "b"), direct!(two_chunks(p::DataType::Int64).i64().unwrap().clone(), |v: &i64| int(v))),
        ("UInt8", script(&recv("UInt8", "u8"), "b"), direct!(two_chunks(p::DataType::UInt8).u8().unwrap().clone(), |v: &u8| int(v))),
        ("UInt16", script(&recv("UInt16", "u16"), "b"), direct!(two_chunks(p::DataType::UInt16).u16().unwrap().clone(), |v: &u16| int(v))),
        ("IdxCa", script(&recv("UInt32", "idx"), "b"), direct!(two_chunks(p::DataType::UInt32).idx().unwrap().clone(), |v: &u32| int(v))),
        ("UInt64", script(&recv("UInt64", "u64"), "b"), direct!(two_chunks(p::DataType::UInt64).u64().unwrap().clone(), |v: &u64| i64::try_from(*v).unwrap().to_string())),
        ("Float32", script(&recv("Float32", "f32"), "b"), direct!(two_chunks(p::DataType::Float32).f32().unwrap().clone(), |v: &f32| flt(v))),
        ("Float64", script(&recv("Float64", "f64"), "b"), direct!(two_chunks(p::DataType::Float64).f64().unwrap().clone(), |v: &f64| flt(v))),
        ("Boolean", script(&recv("Boolean", "bool"), "b"), direct!(two_chunks(p::DataType::Boolean).bool().unwrap().clone(), |v: bool| v.to_string())),
        ("String", script("fx::series_str_mixed().str().unwrap()", "q"), direct!(values::series_str_mixed().str().unwrap().clone(), |v: &str| format!("\"{v}\""))),
        ("Binary", script("fx::series_binary_mixed().binary().unwrap()", "bytes"), direct!(values::series_binary_mixed().binary().unwrap().clone(), bytes)),
        ("BinaryOffset", script("fx::series_binary_offset_mixed().binary_offset().unwrap()", "bytes"), direct!(values::series_binary_offset_mixed().binary_offset().unwrap().clone(), bytes)),
    ];
    for (name, got, want) in &checks {
        assert_eq!(got, want, "{name}");
    }
    assert_eq!(checks[3].1, "2[1,2,3|1,n,3] 1[] 2[1,2,3|1,n,3] 2[1,2,3|1,n,3]", "chunk order, the null in the second chunk; an empty receiver is one empty chunk");
    assert_eq!(checks[11].1, "2[\"é日本\",\"\"|n,\"z\"] 1[] 2[\"é日本\",\"\"|n,\"z\"] 2[\"é日本\",\"\"|n,\"z\"]");
    assert_eq!(checks[13].1, "2[<195 40>,<>|n,<0 0 255>] 1[] 2[<195 40>,<>|n,<0 0 255>] 2[<195 40>,<>|n,<0 0 255>]");
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
            let failed = show(u.downcast_chunks(), b);
            let after = match u.get(0) {{ Ok(Some(v)) => v, _ => 0 }};
            `${{show(i.downcast_chunks(), b)}} ${{show(f.downcast_chunks(), b)}} ${{show(d.downcast_chunks(), b)}} | ${{failed}} | ${{after}}`
        }}
    "#));
    assert_eq!(got, format!("1[-128,127,0] 1[0.10000000149011612,-0.0] 1[-0.0,NaN,inf,-inf] | ConversionError: downcast_chunks: 9223372036854775808 does not fit a script integer | {}", i64::MAX));
}

#[test]
fn the_whole_view_is_bounded() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    // two chunks of 1 + 3 = 4 slots each: each fits a bound of 7, both need 8;
    // strings: 2 + 4 + 9 = 15 slots
    let got = run(&format!(r#"{SHOW}
        pub fn main() {{
            let s = fx::series_i64();
            s.append(fx::series_nulls()).unwrap();
            let r = s.i64().unwrap();
            let t = fx::series_str_mixed().str().unwrap();
            let out = [];
            polars::set_materialize_limit(7); out.push(show(r.downcast_chunks(), b)); out.push(match r.downcast_get(1) {{ Ok(Some(v)) => `one chunk fits: ${{v.len()}} cells`, _ => "no" }});
            polars::set_materialize_limit(8); out.push(show(r.downcast_chunks(), b));
            polars::set_materialize_limit(14); out.push(show(t.downcast_chunks(), q));
            polars::set_materialize_limit(15); out.push(show(t.downcast_chunks(), q));
            polars::set_materialize_limit(0);
            out.iter().fold("", |acc, x| acc + x + "\n")
        }}
    "#));
    assert_eq!(got.lines().collect::<Vec<_>>(), [
        "MaterializeLimit: downcast_chunks: 2 chunks and 6 cells (8 slots), more than the bound of 7",
        "one chunk fits: 3 cells",
        "2[1,2,3|1,n,3]",
        "MaterializeLimit: downcast_chunks: 2 chunks, 4 cells and 9 bytes (15 slots), more than the bound of 14",
        "2[\"é日本\",\"\"|n,\"z\"]",
    ].iter().map(|s| *s).collect::<Vec<_>>(), "{got}");
}
