//! Record 0072: inventory and classification of a crate's reachable public
//! surface from rustdoc JSON.
//!
//!   surface <docs-dir> <root-crate> <out-dir>
//!
//! Reads every `<crate>.json` in `docs-dir`, walks public reachability from
//! the root crate, deduplicates by defining crate and id, and writes
//! `inventory.json`, `summary.json` and `summary.md` to `out-dir`.
mod classify;
mod inventory;
mod render;

use std::collections::HashMap;
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        eprintln!("usage: surface <docs-dir> <root-crate> <out-dir>");
        std::process::exit(2);
    }
    let docs = inventory::Docs::load(Path::new(&args[1]));
    let inv = inventory::extract(&docs, &args[2]);
    let classified = classify::classify_all(&inv, &docs);
    let out = Path::new(&args[3]);
    std::fs::create_dir_all(out).unwrap();
    std::fs::write(out.join("inventory.json"), serde_json::to_string_pretty(&classified).unwrap()).unwrap();
    let summary = classify::summarize(&classified);
    std::fs::write(out.join("summary.json"), serde_json::to_string_pretty(&summary).unwrap()).unwrap();
    std::fs::write(out.join("summary.md"), classify::summary_md(&summary)).unwrap();
    let mut rules: Vec<(&str, &str)> = classify::RULES.to_vec();
    rules.sort();
    let _ = HashMap::<(), ()>::new();
    println!("{}", classify::summary_md(&summary));
}
