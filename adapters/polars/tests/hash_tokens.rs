//! Record 0094: categorical hashes cross the script boundary as exact 16-digit lowercase hex tokens, compared bit for bit with direct Polars.
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

use rnx_polars::generated::support::categorical_fixtures as cf;
use std::hash::BuildHasher;

fn tok(v: u64) -> String { format!("{v:016x}") }
fn lookup(s: &str) -> u64 { cf::lookup_hasher().hash_one(s) }
fn cat(r: Option<u32>) -> String { r.map_or("None".into(), |c| format!("Some({c})")) }

const HELPERS: &str = r#"
    fn show(r) { match r { Ok(Some(v)) => `Some(${v})`, Ok(None) => "None", Some(v) => `Some(${v})`, None => "None", Ok(v) => `Some(${v})`, Err(e) => `${e.kind()}` } }
"#;

#[test]
fn producers_emit_the_exact_bits() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let got = run(r#"
        pub fn main() {
            let m = fx::categorical_mapping();
            let stored = match m.cat_to_hash(0).unwrap() { Some(t) => t, None => "none" };
            let low = match m.cat_to_hash(1).unwrap() { Some(t) => t, None => "none" };
            let absent = match m.cat_to_hash(5).unwrap() { Some(t) => t, None => "none" };
            `${fx::categories().hash()} ${fx::frozen_categories().hash()} ${stored} ${low} ${absent}`
        }
    "#);
    let m = cf::mapping();
    let want = format!("{} {} {} {} none", tok(cf::categories().hash()), tok(cf::frozen_categories().hash()), tok(m.cat_to_hash(0).unwrap()), tok(m.cat_to_hash(1).unwrap()));
    assert_eq!(got, want);
    // the fixtures reach the upper half on every producer, so a signed or truncating route would show
    assert!(cf::categories().hash() > i64::MAX as u64 && cf::frozen_categories().hash() > i64::MAX as u64 && m.cat_to_hash(0).unwrap() > i64::MAX as u64);
    assert!(m.cat_to_hash(1).unwrap() <= i64::MAX as u64, "and a lower-half value keeps its leading zeros");
}

#[test]
fn consumers_pass_the_exact_bits_to_polars() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (h2, h1, hmiss, hnew) = (lookup("rnx-0094-2"), lookup("rnx-0094-1"), lookup("missing"), lookup("new"));
    let m = cf::mapping();
    let stored = m.cat_to_hash(0).unwrap();
    let bounds = [0u64, i64::MAX as u64, i64::MAX as u64 + 1, u64::MAX];
    let got = run(&format!(r#"{HELPERS}
        pub fn main() {{
            let m = fx::categorical_mapping();
            let found = [show(m.get_cat_with_hash("rnx-0094-2", "{t2}")), show(m.get_cat_with_hash("rnx-0094-1", "{t1}")), show(m.get_cat_with_hash("missing", "{tm}"))];
            let wrong = show(m.get_cat_with_hash("rnx-0094-2", "{ts}"));
            let edges = [show(m.get_cat_with_hash("rnx-0094-2", "{b0}")), show(m.get_cat_with_hash("rnx-0094-2", "{b1}")), show(m.get_cat_with_hash("rnx-0094-2", "{b2}")), show(m.get_cat_with_hash("rnx-0094-2", "{b3}"))];
            let inserted = show(m.insert_cat_with_hash("new", "{tn}"));
            let again = [show(m.insert_cat_with_hash("new", "{tn}")), show(m.get_cat("new")), m.len().unwrap()];
            `${{found[0]}} ${{found[1]}} ${{found[2]}} | ${{wrong}} | ${{edges[0]}} ${{edges[1]}} ${{edges[2]}} ${{edges[3]}} | ${{inserted}} ${{again[0]}} ${{again[1]}} ${{again[2]}}`
        }}
    "#, t2 = tok(h2), t1 = tok(h1), tm = tok(hmiss), ts = tok(stored), tn = tok(hnew), b0 = tok(bounds[0]), b1 = tok(bounds[1]), b2 = tok(bounds[2]), b3 = tok(bounds[3])));
    let mut r = cf::mapping();
    let edges: Vec<String> = bounds.iter().map(|b| cat(r.get_cat_with_hash("rnx-0094-2", *b))).collect();
    let inserted = r.insert_cat_with_hash("new", hnew).unwrap();
    let again = r.insert_cat_with_hash("new", hnew).unwrap();
    let want = format!(
        "{} {} {} | {} | {} | Some({inserted}) Some({again}) {} {}",
        cat(r.get_cat_with_hash("rnx-0094-2", h2)), cat(r.get_cat_with_hash("rnx-0094-1", h1)), cat(r.get_cat_with_hash("missing", hmiss)),
        cat(r.get_cat_with_hash("rnx-0094-2", stored)),
        edges.join(" "),
        cat(r.get_cat("new")), r.len(),
    );
    assert_eq!(got, want);
    assert!(want.starts_with("Some(0) Some(1) None | "), "the lookup hash finds existing categories: {want}");
    assert!(h2 > i64::MAX as u64, "a positive lookup crosses with its high bit set");
}

#[test]
fn a_wrong_but_well_formed_insert_hash_behaves_as_in_polars() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    // the stored (fixed-state) hash is a different domain from the lookup
    // hash; Polars files the string under whatever hash it is given
    let mut r = cf::mapping();
    let stored = r.cat_to_hash(0).unwrap();
    let got = run(&format!(r#"{HELPERS}
        pub fn main() {{
            let m = fx::categorical_mapping();
            let dup = show(m.insert_cat_with_hash("rnx-0094-2", "{ts}"));
            `${{dup}} ${{show(m.get_cat("rnx-0094-2"))}} ${{m.len().unwrap()}}`
        }}
    "#, ts = tok(stored)));
    let dup = r.insert_cat_with_hash("rnx-0094-2", stored).unwrap();
    assert_eq!(got, format!("Some({dup}) {} {}", cat(r.get_cat("rnx-0094-2")), r.len()));
}

#[test]
fn malformed_tokens_are_refused_before_polars_and_leave_the_mapping_usable() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let got = run(r#"
        pub fn main() {
            let m = fx::categorical_mapping();
            let bad = ["", "123", "00000000000000000", "FFFFFFFFFFFFFFFF", "0x00000000000000", "+000000000000000", "000000000000000g", "00000000000000é", "0000000000000 00"];
            let out = [];
            for t in bad {
                let g = match m.get_cat_with_hash("rnx-0094-2", t) { Err(e) => `${e.kind()}: ${e.message()}`, Ok(_) => "accepted" };
                let i = match m.insert_cat_with_hash("rnx-0094-9", t) { Err(e) => e.kind(), Ok(_) => "inserted" };
                out.push(`${g} / ${i} / ${m.len().unwrap()} ${m.get_cat("rnx-0094-2") == Some(0)}`);
            }
            out.iter().fold("", |a, b| a + b + "\n")
        }
    "#);
    let lines: Vec<&str> = got.lines().collect();
    assert_eq!(lines.len(), 9, "{got}");
    for l in &lines {
        assert!(l.starts_with("ConversionError: get_cat_with_hash: hash must be 16 lowercase hex digits, got "), "{l}");
        assert!(l.ends_with(" / ConversionError / 2 true"), "refused before Polars, mapping unchanged and usable: {l}");
    }
}
