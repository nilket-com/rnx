//! Record 0100: `chunks()` on the Boolean, String, Binary and BinaryOffset wrappers as owned nested options, compared chunk by chunk and byte for byte with direct Polars, and bounded by chunks + cells + payload bytes.
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
use polars_arrow::array::{BinaryArray, BinaryViewArray, BooleanArray, Utf8ViewArray};
use rnx_polars::generated::fixtures::values;

/// `cell` renders one present value; strings are quoted so an empty one shows.
const SHOW: &str = r#"
    fn bytes(v) { let s = "<"; let j = 0; for b in v { if j > 0 { s = s + " "; } s = s + `${b}`; j = j + 1; } s + ">" }
    fn show(r, cell) { match r { Ok(outer) => { let s = `${outer.len()}[`; let i = 0; for inner in outer { if i > 0 { s = s + "|"; } let j = 0; for x in inner { if j > 0 { s = s + ","; } s = s + match x { Some(v) => cell(v), None => "n" }; j = j + 1; } i = i + 1; } s + "]" }, Err(e) => `${e.kind()}: ${e.message()}` } }
    fn b(v) { `${v}` }
    fn q(v) { `"${v}"` }
"#;
fn bytes(v: &[u8]) -> String { format!("<{}>", v.iter().map(|b| b.to_string()).collect::<Vec<_>>().join(" ")) }
/// The script's rendering of what Rust sees in `chunks()`.
macro_rules! rust_show {
    ($ca:expr, $arr:ty, $cell:expr) => {{
        let parts: Vec<String> = $ca.chunks().iter().map(|a| a.as_any().downcast_ref::<$arr>().unwrap().iter().map(|x| x.map_or("n".to_string(), $cell)).collect::<Vec<_>>().join(",")).collect();
        format!("{}[{}]", parts.len(), parts.join("|"))
    }};
}

fn script(recv: &str, one: &str, empty: &str, cell: &str) -> String {
    run(&format!(r#"{SHOW}
        pub fn main() {{
            let r = {recv};
            let one = {one};
            let e = {empty};
            let kept = {{ let t = {recv}; t.chunks() }};
            `${{show(r.chunks(), {cell})}} ${{show(one.chunks(), {cell})}} ${{show(e.chunks(), {cell})}} ${{show(kept, {cell})}} ${{show(r.chunks(), {cell})}}`
        }}
    "#))
}

#[test]
fn all_four_owners_keep_chunks_nulls_and_payloads() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let q = |v: &str| format!("\"{v}\"");
    // Boolean: [true, false, true] ++ cast([1, null, 3]) = [true, null, true]
    let got = script("{ let s = fx::series_bool(); s.append(fx::series_nulls().cast(polars::DataType::Boolean()).unwrap()).unwrap(); s.bool().unwrap() }", "fx::series_bool().bool().unwrap()", "fx::series_bool().bool().unwrap().limit(0).unwrap()", "b");
    let r = { let mut s = values::series_bool(); s.append(&values::series_nulls().cast(&p::DataType::Boolean).unwrap()).unwrap(); s.bool().unwrap().clone() };
    let one = values::series_bool().bool().unwrap().clone();
    let a = rust_show!(r, BooleanArray, |v: bool| v.to_string());
    assert_eq!(got, format!("{a} {} {} {a} {a}", rust_show!(one, BooleanArray, |v: bool| v.to_string()), rust_show!(one.limit(0), BooleanArray, |v: bool| v.to_string())));
    assert_eq!(a, "2[true,false,true|true,n,true]");
    assert!(got.contains(" 1[] "), "an empty receiver is one empty chunk: {got}");
    // String: [é日本, ""] ++ [null, z]
    let got = script("fx::series_str_mixed().str().unwrap()", "fx::series_str().str().unwrap()", "fx::series_str().str().unwrap().limit(0).unwrap()", "q");
    let r = values::series_str_mixed().str().unwrap().clone();
    let one = values::series_str().str().unwrap().clone();
    let a = rust_show!(r, Utf8ViewArray, q);
    assert_eq!(got, format!("{a} {} {} {} {a}", rust_show!(one, Utf8ViewArray, q), rust_show!(one.limit(0), Utf8ViewArray, q), rust_show!(r, Utf8ViewArray, q)));
    assert_eq!(a, "2[\"é日本\",\"\"|n,\"z\"]");
    // Binary (views): [c3 28, <empty>] ++ [null, 00 00 ff]
    let got = script("fx::series_binary_mixed().binary().unwrap()", "fx::series_binary().binary().unwrap()", "fx::series_binary().binary().unwrap().limit(0).unwrap()", "bytes");
    let r = values::series_binary_mixed().binary().unwrap().clone();
    let one = values::series_binary().binary().unwrap().clone();
    let a = rust_show!(r, BinaryViewArray, bytes);
    assert_eq!(got, format!("{a} {} {} {} {a}", rust_show!(one, BinaryViewArray, bytes), rust_show!(one.limit(0), BinaryViewArray, bytes), rust_show!(r, BinaryViewArray, bytes)));
    assert_eq!(a, "2[<195 40>,<>|n,<0 0 255>]", "raw non-UTF-8, empty and zero bytes");
    // BinaryOffset: the same values
    let got = script("fx::series_binary_offset_mixed().binary_offset().unwrap()", "fx::series_binary_offset().binary_offset().unwrap()", "fx::series_binary_offset().binary_offset().unwrap().limit(0).unwrap()", "bytes");
    let r = values::series_binary_offset_mixed().binary_offset().unwrap().clone();
    let one = values::series_binary_offset().binary_offset().unwrap().clone();
    let a = rust_show!(r, BinaryArray<i64>, bytes);
    assert_eq!(got, format!("{a} {} {} {} {a}", rust_show!(one, BinaryArray<i64>, bytes), rust_show!(one.limit(0), BinaryArray<i64>, bytes), rust_show!(r, BinaryArray<i64>, bytes)));
    assert_eq!(a, "2[<195 40>,<>|n,<0 0 255>]");
}

#[test]
fn the_whole_result_is_bounded_with_payload_bytes() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    // [é日本, ""] ++ [null, z]: 2 chunks + 4 cells + (8 + 0 + 1) bytes = 15 slots
    let got = run(&format!(r#"{SHOW}
        pub fn main() {{
            let r = fx::series_str_mixed().str().unwrap();
            let bin = fx::series_binary_mixed().binary().unwrap();
            let out = [];
            polars::set_materialize_limit(14); out.push(show(r.chunks(), q));
            polars::set_materialize_limit(15); out.push(show(r.chunks(), q));
            // every value is below 12 alone; 2 + 4 + (2 + 0 + 3) = 11 slots in all
            polars::set_materialize_limit(10); out.push(show(bin.chunks(), bytes));
            polars::set_materialize_limit(11); out.push(show(bin.chunks(), bytes));
            polars::set_materialize_limit(0); out.push(show(r.chunks(), q));
            out.iter().fold("", |acc, x| acc + x + "\n")
        }}
    "#));
    assert_eq!(got.lines().collect::<Vec<_>>(), [
        "MaterializeLimit: chunks: 2 chunks, 4 cells and 9 bytes (15 slots), more than the bound of 14",
        "2[\"é日本\",\"\"|n,\"z\"]",
        "MaterializeLimit: chunks: 2 chunks, 4 cells and 5 bytes (11 slots), more than the bound of 10",
        "2[<195 40>,<>|n,<0 0 255>]",
        "2[\"é日本\",\"\"|n,\"z\"]",
    ], "{got}");
}
