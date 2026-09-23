//! Record 0106: `layout()` on the fourteen wrappers reports the Polars variant by name with its chunks copied, compared with a direct match on the real `layout()`, and bounded before the call.
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
use polars_core::chunked_array::ChunkedArrayLayout as L;
use rnx_polars::generated::fixtures::values;

const SHOW: &str = r#"
    fn bytes(v) { let s = "<"; let j = 0; for b in v { if j > 0 { s = s + " "; } s = s + `${b}`; j = j + 1; } s + ">" }
    fn b(v) { `${v}` }
    fn q(v) { `"${v}"` }
    fn show(r, cell) { match r { Ok((tag, outer)) => { let s = `${tag} ${outer.len()}[`; let i = 0; for inner in outer { if i > 0 { s = s + "|"; } let j = 0; for x in inner { if j > 0 { s = s + ","; } s = s + match x { Some(v) => cell(v), None => "n" }; j = j + 1; } i = i + 1; } s + "]" }, Err(e) => `${e.kind()}: ${e.message()}` } }
"#;
fn bytes(v: &[u8]) -> String { format!("<{}>", v.iter().map(|b| b.to_string()).collect::<Vec<_>>().join(" ")) }
/// The script's rendering of a direct match on `layout()`.
macro_rules! rust_show {
    ($ca:expr, $cell:expr) => {{
        let ca = &$ca;
        let (tag, parts): (&str, Vec<String>) = match ca.layout() {
            L::SingleNoNull(a) => ("SingleNoNull", vec![a.iter().map(|x| x.map_or("n".to_string(), $cell)).collect::<Vec<_>>().join(",")]),
            L::Single(a) => ("Single", vec![a.iter().map(|x| x.map_or("n".to_string(), $cell)).collect::<Vec<_>>().join(",")]),
            L::MultiNoNull(c) => ("MultiNoNull", c.downcast_iter().map(|a| a.iter().map(|x| x.map_or("n".to_string(), $cell)).collect::<Vec<_>>().join(",")).collect()),
            L::Multi(c) => ("Multi", c.downcast_iter().map(|a| a.iter().map(|x| x.map_or("n".to_string(), $cell)).collect::<Vec<_>>().join(",")).collect()),
        };
        format!("{tag} {}[{}]", parts.len(), parts.join("|"))
    }};
}
fn int<N: Into<i64> + Copy>(v: &N) -> String { (*v).into().to_string() }
fn flt<N: Into<f64> + Copy>(v: &N) -> String { format!("{:?}", (*v).into()) }

/// The four receivers of an owner, as script expressions: one chunk without
/// nulls, one with a null, two without nulls, two with a null in the later one.
fn script(four: [&str; 4], acc: &str, cell: &str) -> String {
    run(&format!(r#"{SHOW}
        pub fn main() {{
            let r1 = {}.{acc}().unwrap();
            let r2 = {}.{acc}().unwrap();
            let r3 = {}.{acc}().unwrap();
            let r4 = {}.{acc}().unwrap();
            let r0 = r1.limit(0).unwrap();
            let kept = {{ let t = {}.{acc}().unwrap(); t.layout() }};
            `${{show(r1.layout(), {cell})}} / ${{show(r2.layout(), {cell})}} / ${{show(r3.layout(), {cell})}} / ${{show(r4.layout(), {cell})}} / ${{show(r0.layout(), {cell})}} / ${{show(kept, {cell})}} / ${{show(r4.layout(), {cell})}}`
        }}
    "#, four[0], four[1], four[2], four[3], four[3]))
}
macro_rules! direct {
    ($four:expr, $acc:ident, $cell:expr) => {{
        let [a, b, c, d]: [p::Series; 4] = $four;
        let (a, b, c, d) = (a.$acc().unwrap().clone(), b.$acc().unwrap().clone(), c.$acc().unwrap().clone(), d.$acc().unwrap().clone());
        let e = a.limit(0);
        let dd = rust_show!(d, $cell);
        format!("{} / {} / {} / {dd} / {} / {dd} / {dd}", rust_show!(a, $cell), rust_show!(b, $cell), rust_show!(c, $cell), rust_show!(e, $cell))
    }};
}
fn cat(a: p::Series, b: p::Series) -> p::Series { let mut a = a; a.append(&b).unwrap(); a }
/// Numeric and Boolean receivers, cast from Int64 fixtures.
fn numeric(dt: &p::DataType) -> [p::Series; 4] {
    let i = || p::Series::new("x".into(), [1i64, 2, 3]);
    let n = || p::Series::new("x".into(), [Some(1i64), None, Some(3)]);
    [i().cast(dt).unwrap(), n().cast(dt).unwrap(), cat(i(), i()).cast(dt).unwrap(), cat(i(), n()).cast(dt).unwrap()]
}
fn numeric_script(dt: &str) -> [String; 4] {
    [format!("fx::series_i64().cast(polars::DataType::{dt}()).unwrap()"), format!("fx::series_nulls().cast(polars::DataType::{dt}()).unwrap()"),
     format!("{{ let s = fx::series_i64(); s.append(fx::series_i64()).unwrap(); s.cast(polars::DataType::{dt}()).unwrap() }}"),
     format!("{{ let s = fx::series_i64(); s.append(fx::series_nulls()).unwrap(); s.cast(polars::DataType::{dt}()).unwrap() }}")]
}
/// Payload receivers from the typed and mixed fixtures.
fn payload(plain: fn() -> p::Series, mixed: fn() -> p::Series) -> [p::Series; 4] {
    [plain(), mixed().rechunk(), cat(plain(), plain()), mixed()]
}
fn payload_script(plain: &str, mixed: &str) -> [String; 4] {
    [format!("fx::{plain}()"), format!("fx::{mixed}().rechunk()"), format!("{{ let s = fx::{plain}(); s.append(fx::{plain}()).unwrap(); s }}"), format!("fx::{mixed}()")]
}
fn refs(v: &[String; 4]) -> [&str; 4] { [&v[0], &v[1], &v[2], &v[3]] }

#[test]
fn all_fourteen_owners_report_their_variant_and_chunks() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let checks: Vec<(&str, String, String)> = vec![
        ("Int8", script(refs(&numeric_script("Int8")), "i8", "b"), direct!(numeric(&p::DataType::Int8), i8, |v: &i8| int(v))),
        ("Int16", script(refs(&numeric_script("Int16")), "i16", "b"), direct!(numeric(&p::DataType::Int16), i16, |v: &i16| int(v))),
        ("Int32", script(refs(&numeric_script("Int32")), "i32", "b"), direct!(numeric(&p::DataType::Int32), i32, |v: &i32| int(v))),
        ("Int64", script(refs(&numeric_script("Int64")), "i64", "b"), direct!(numeric(&p::DataType::Int64), i64, |v: &i64| int(v))),
        ("UInt8", script(refs(&numeric_script("UInt8")), "u8", "b"), direct!(numeric(&p::DataType::UInt8), u8, |v: &u8| int(v))),
        ("UInt16", script(refs(&numeric_script("UInt16")), "u16", "b"), direct!(numeric(&p::DataType::UInt16), u16, |v: &u16| int(v))),
        ("IdxCa", script(refs(&numeric_script("UInt32")), "idx", "b"), direct!(numeric(&p::DataType::UInt32), idx, |v: &u32| int(v))),
        ("UInt64", script(refs(&numeric_script("UInt64")), "u64", "b"), direct!(numeric(&p::DataType::UInt64), u64, |v: &u64| i64::try_from(*v).unwrap().to_string())),
        ("Float32", script(refs(&numeric_script("Float32")), "f32", "b"), direct!(numeric(&p::DataType::Float32), f32, |v: &f32| flt(v))),
        ("Float64", script(refs(&numeric_script("Float64")), "f64", "b"), direct!(numeric(&p::DataType::Float64), f64, |v: &f64| flt(v))),
        ("Boolean", script(refs(&numeric_script("Boolean")), "bool", "b"), direct!(numeric(&p::DataType::Boolean), bool, |v: bool| v.to_string())),
        ("String", script(refs(&payload_script("series_str", "series_str_mixed")), "str", "q"), direct!(payload(values::series_str, values::series_str_mixed), str, |v: &str| format!("\"{v}\""))),
        ("Binary", script(refs(&payload_script("series_binary", "series_binary_mixed")), "binary", "bytes"), direct!(payload(values::series_binary, values::series_binary_mixed), binary, bytes)),
        ("BinaryOffset", script(refs(&payload_script("series_binary_offset", "series_binary_offset_mixed")), "binary_offset", "bytes"), direct!(payload(values::series_binary_offset, values::series_binary_offset_mixed), binary_offset, bytes)),
    ];
    for (name, got, want) in &checks {
        assert_eq!(got, want, "{name}");
    }
    // all four variants, reported as Polars returned them
    assert_eq!(checks[3].1, "SingleNoNull 1[1,2,3] / Single 1[1,n,3] / MultiNoNull 2[1,2,3|1,2,3] / Multi 2[1,2,3|1,n,3] / SingleNoNull 1[] / Multi 2[1,2,3|1,n,3] / Multi 2[1,2,3|1,n,3]");
    assert_eq!(checks[11].1, "SingleNoNull 1[\"a\",\"bb\",\"ccc\"] / Single 1[\"é日本\",\"\",n,\"z\"] / MultiNoNull 2[\"a\",\"bb\",\"ccc\"|\"a\",\"bb\",\"ccc\"] / Multi 2[\"é日本\",\"\"|n,\"z\"] / SingleNoNull 1[] / Multi 2[\"é日本\",\"\"|n,\"z\"] / Multi 2[\"é日本\",\"\"|n,\"z\"]");
    assert!(checks[13].1.starts_with("SingleNoNull 1[<97 98>,<0 255>,<>] / Single 1[<195 40>,<>,n,<0 0 255>] / MultiNoNull 2["), "{}", checks[13].1);
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
            let failed = show(u.layout(), b);
            let after = match u.get(0) {{ Ok(Some(v)) => v, _ => 0 }};
            `${{show(i.layout(), b)}} ${{show(f.layout(), b)}} ${{show(d.layout(), b)}} | ${{failed}} | ${{after}} ${{u.chunk_lengths().unwrap().len()}}`
        }}
    "#));
    assert_eq!(got, format!("SingleNoNull 1[-128,127,0] SingleNoNull 1[0.10000000149011612,-0.0] SingleNoNull 1[-0.0,NaN,inf,-inf] | ConversionError: layout: 9223372036854775808 does not fit a script integer | {} 1", i64::MAX));
}

#[test]
fn the_whole_payload_is_bounded_before_the_call() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    // Multi: 2 chunks + 6 cells = 8; each chunk alone is 4. Single: 1 + 3 = 4. Strings Multi: 2 + 4 + 9 = 15.
    let got = run(&format!(r#"{SHOW}
        pub fn main() {{
            let s = fx::series_i64();
            s.append(fx::series_nulls()).unwrap();
            let m = s.i64().unwrap();
            let one = fx::series_nulls().i64().unwrap();
            let t = fx::series_str_mixed().str().unwrap();
            let out = [];
            polars::set_materialize_limit(7); out.push(show(m.layout(), b));
            polars::set_materialize_limit(8); out.push(show(m.layout(), b));
            polars::set_materialize_limit(3); out.push(show(one.layout(), b));
            polars::set_materialize_limit(4); out.push(show(one.layout(), b));
            polars::set_materialize_limit(14); out.push(show(t.layout(), q));
            polars::set_materialize_limit(15); out.push(show(t.layout(), q));
            polars::set_materialize_limit(0);
            out.iter().fold("", |acc, x| acc + x + "\n")
        }}
    "#));
    assert_eq!(got.lines().collect::<Vec<_>>(), [
        "MaterializeLimit: layout: 2 chunks and 6 cells (8 slots), more than the bound of 7",
        "Multi 2[1,2,3|1,n,3]",
        "MaterializeLimit: layout: 1 chunks and 3 cells (4 slots), more than the bound of 3",
        "Single 1[1,n,3]",
        "MaterializeLimit: layout: 2 chunks, 4 cells and 9 bytes (15 slots), more than the bound of 14",
        "Multi 2[\"é日本\",\"\"|n,\"z\"]",
    ], "{got}");
}
