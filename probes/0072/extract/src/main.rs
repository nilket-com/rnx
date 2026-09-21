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
    // Record 0075: the inventory carries its provenance, copied from the
    // documentation run's pins.json (`release` for a crates.io release,
    // `rev` for Git workspace sources, and the configuration), so a
    // generator can refuse a release policy that does not belong to it.
    let mut value = serde_json::to_value(&classified).unwrap();
    let pins_path = Path::new(&args[1]).join("pins.json");
    let provenance = match std::fs::read_to_string(&pins_path) {
        Ok(text) => {
            let pins: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", pins_path.display()));
            let mut prov = serde_json::Map::new();
            for key in ["release", "rev", "cfg"] {
                if let Some(v) = pins.get(key) {
                    prov.insert(key.to_string(), v.clone());
                }
            }
            if !prov.contains_key("release") && !prov.contains_key("rev") {
                eprintln!("{}: neither `release` nor `rev` recorded; the inventory gets no provenance", pins_path.display());
                serde_json::Value::Null
            } else {
                serde_json::Value::Object(prov)
            }
        }
        Err(_) => serde_json::Value::Null,
    };
    value.as_object_mut().unwrap().insert("provenance".to_string(), provenance);
    std::fs::write(out.join("inventory.json"), serde_json::to_string_pretty(&value).unwrap()).unwrap();
    let summary = classify::summarize(&classified);
    std::fs::write(out.join("summary.json"), serde_json::to_string_pretty(&summary).unwrap()).unwrap();
    std::fs::write(out.join("summary.md"), classify::summary_md(&summary)).unwrap();
    let mut rules: Vec<(&str, &str)> = classify::RULES.to_vec();
    rules.sort();
    let _ = HashMap::<(), ()>::new();
    println!("{}", classify::summary_md(&summary));
}
