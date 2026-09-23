//! Record 0101: `downcast_get(idx)` on the fourteen wrappers copies only the selected chunk, compared with direct Polars, with None past the last chunk and a bound on the selected chunk alone.
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
    fn show(r, cell) { match r { Ok(Some(inner)) => { let s = `${inner.len()}[`; let j = 0; for x in inner { if j > 0 { s = s + ","; } s = s + match x { Some(v) => cell(v), None => "n" }; j = j + 1; } s + "]" }, Ok(None) => "None", Err(e) => `${e.kind()}: ${e.message()}` } }
"#;
fn bytes(v: &[u8]) -> String { format!("<{}>", v.iter().map(|b| b.to_string()).collect::<Vec<_>>().join(" ")) }
/// The script's rendering of a direct `downcast_get`.
macro_rules! rust_show {
    ($got:expr, $cell:expr) => {{
        match $got {
            None => "None".to_string(),
            Some(a) => { let v: Vec<String> = a.iter().map(|x| x.map_or("n".to_string(), $cell)).collect(); format!("{}[{}]", v.len(), v.join(",")) }
        }
    }};
}
fn script(recv: &str, cell: &str) -> String {
    run(&format!(r#"{SHOW}
        pub fn main() {{
            let r = {recv};
            let e = r.limit(0).unwrap();
            let kept = {{ let t = {recv}; t.downcast_get(1) }};
            `${{show(r.downcast_get(0), {cell})}} ${{show(r.downcast_get(1), {cell})}} ${{show(r.downcast_get(2), {cell})}} ${{show(e.downcast_get(0), {cell})}} ${{show(e.downcast_get(1), {cell})}} ${{show(kept, {cell})}} ${{show(r.downcast_get(-1), {cell})}}`
        }}
    "#))
}
macro_rules! direct {
    ($r:expr, $cell:expr) => {{
        let r = $r;
        let e = r.limit(0);
        format!("{} {} {} {} {} {} ConversionError: idx: -1 is out of range for usize", rust_show!(r.downcast_get(0), $cell), rust_show!(r.downcast_get(1), $cell), rust_show!(r.downcast_get(2), $cell), rust_show!(e.downcast_get(0), $cell), rust_show!(e.downcast_get(1), $cell), rust_show!(r.downcast_get(1), $cell))
    }};
}
fn int<N: Into<i64>>(v: &N) -> String where N: Copy { (*v).into().to_string() }
fn flt<N: Into<f64> + Copy>(v: &N) -> String { format!("{:?}", (*v).into()) }

fn two_chunks(dtype: p::DataType) -> p::Series {
    let mut s = p::Series::new("x".into(), [1i64, 2, 3]);
    s.append(&p::Series::new("x".into(), [Some(1i64), None, Some(3)])).unwrap();
    s.cast(&dtype).unwrap()
}
fn recv(dtype: &str, acc: &str) -> String {
    format!("{{ let s = fx::series_i64(); s.append(fx::series_nulls()).unwrap(); s.cast(polars::DataType::{dtype}()).unwrap().{acc}().unwrap() }}")
}

#[test]
fn all_fourteen_owners_select_one_chunk() {
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
    // chunk index, not row index: index 1 is the whole second chunk; a present empty chunk is 0[], an absent one None
    assert_eq!(checks[3].1, "3[1,2,3] 3[1,n,3] None 0[] None 3[1,n,3] ConversionError: idx: -1 is out of range for usize");
    assert_eq!(checks[11].1, "2[\"é日本\",\"\"] 2[n,\"z\"] None 0[] None 2[n,\"z\"] ConversionError: idx: -1 is out of range for usize");
    assert_eq!(checks[12].1, "2[<195 40>,<>] 2[n,<0 0 255>] None 0[] None 2[n,<0 0 255>] ConversionError: idx: -1 is out of range for usize");
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
            let failed = show(u.downcast_get(0), b);
            let after = match u.get(0) {{ Ok(Some(v)) => v, _ => 0 }};
            `${{show(i.downcast_get(0), b)}} ${{show(f.downcast_get(0), b)}} ${{show(d.downcast_get(0), b)}} | ${{failed}} | ${{after}} ${{show(u.downcast_get(1), b)}}`
        }}
    "#));
    assert_eq!(got, format!("3[-128,127,0] 2[0.10000000149011612,-0.0] 4[-0.0,NaN,inf,-inf] | ConversionError: downcast_get: 9223372036854775808 does not fit a script integer | {} None", i64::MAX));
}

#[test]
fn only_the_selected_chunk_is_bounded() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    // [1,2,3] ++ [1,null,3] costs 8 slots whole; its chunk 1 costs 1 + 3 = 4
    let got = run(&format!(r#"{SHOW}
        pub fn main() {{
            let s = fx::series_i64();
            s.append(fx::series_nulls()).unwrap();
            let r = s.i64().unwrap();
            let t = fx::series_str_mixed().str().unwrap();
            let out = [];
            polars::set_materialize_limit(3); out.push(show(r.downcast_get(1), b));
            polars::set_materialize_limit(4); out.push(show(r.downcast_get(1), b)); out.push(show(r.chunks().map(|v| [v.len()]), b));
            polars::set_materialize_limit(10); out.push(show(t.downcast_get(0), q));
            polars::set_materialize_limit(11); out.push(show(t.downcast_get(0), q));
            polars::set_materialize_limit(1); out.push(show(r.downcast_get(5), b));
            polars::set_materialize_limit(0); out.push(show(r.downcast_get(0), b));
            out.iter().fold("", |acc, x| acc + x + "\n")
        }}
    "#));
    assert_eq!(got.lines().collect::<Vec<_>>(), [
        "MaterializeLimit: downcast_get: 1 chunks and 3 cells (4 slots), more than the bound of 3",
        "3[1,n,3]",
        "MaterializeLimit: chunks: 2 chunks and 6 cells (8 slots), more than the bound of 4",
        "MaterializeLimit: downcast_get: 1 chunks, 2 cells and 8 bytes (11 slots), more than the bound of 10",
        "2[\"é日本\",\"\"]",
        "None",
        "3[1,2,3]",
    ], "{got}");
}
