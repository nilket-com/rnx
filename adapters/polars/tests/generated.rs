//! Record 0073: the generated bindings are reproducible and every
//! inventoried callable is accounted for.
//!
//! Both tests need the record 0072 inventory of the locked documentation
//! set; produce it with `probes/0072/run.sh report`. A missing inventory
//! fails the test rather than skipping it.
use std::path::PathBuf;
use std::process::Command;

fn root() -> PathBuf {
	PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn inventory() -> PathBuf {
	let p = root().join("probes/0072/out/0.55.2-adapter/result/inventory.json");
	assert!(p.exists(), "missing {}: run probes/0072/run.sh report first", p.display());
	p
}

/// Regenerating from the inventory must reproduce the committed files.
#[test]
fn generated_files_do_not_drift() {
	let status = Command::new("cargo")
		.args(["run", "-q", "--locked", "--manifest-path"])
		.arg(root().join("tools/polars-gen/Cargo.toml"))
		.arg("--")
		.arg(inventory())
		.arg(root().join("adapters/polars"))
		.args(["--buckets", BUCKETS, "--check"])
		.env("CARGO_TARGET_DIR", root().join("target/0073"))
		.status()
		.expect("run polars-gen");
	assert!(status.success(), "generated files differ from the generator's output; regenerate and commit");
}

/// The buckets this stage generates; kept in one place with the drift test.
const BUCKETS: &str = "mechanical,conversion,option_struct";

/// Every eligible callable of the API crates has exactly one status.
#[test]
fn every_callable_is_accounted_for() {
	let inv: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(inventory()).unwrap()).unwrap();
	let surface: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(root().join("adapters/polars/surface.json")).unwrap()).unwrap();
	let mut seen = std::collections::HashMap::new();
	for e in surface["entries"].as_array().unwrap() {
		let key = e["key"].as_str().unwrap().to_string();
		assert!(seen.insert(key.clone(), e["status"].as_str().unwrap().to_string()).is_none(), "{key} accounted twice");
		match e["status"].as_str().unwrap() {
			"generated" => assert!(e["rune"].is_string(), "{key}: generated without a Rune path"),
			"adapted" | "unsupported" | "out_of_scope" => assert!(e["reason"].is_string(), "{key}: no reason"),
			other => panic!("{key}: unknown status {other}"),
		}
	}
	let mut missing = Vec::new();
	let mut eligible = 0;
	for c in inv["callables"].as_array().unwrap() {
		let bucket = c["bucket"].as_str().unwrap();
		if bucket == "unsupported" || bucket == "unknown" {
			continue;
		}
		eligible += 1;
		let key = c["key"].as_str().unwrap();
		if !seen.contains_key(key) {
			missing.push(c["canonical_path"].as_str().unwrap().to_string());
		}
	}
	assert!(missing.is_empty(), "{} of {eligible} eligible callables unaccounted, e.g. {:?}", missing.len(), &missing[..missing.len().min(5)]);
	assert_eq!(seen.len(), eligible, "surface.json lists entries that are not eligible callables");
	// Every generated entry has exactly one execution disposition, and the
	// dispositions partition the generated set; `case` entries equal the
	// oracle cases the generator emitted.
	let mut cases = 0;
	let mut generated = 0;
	let mut dispositions = std::collections::BTreeMap::new();
	for e in surface["entries"].as_array().unwrap() {
		if e["status"] != "generated" {
			continue;
		}
		generated += 1;
		let d = e["execution"].as_str().unwrap_or_else(|| panic!("{}: generated without an execution disposition", e["canonical_path"]));
		if d == "case" {
			cases += 1;
		}
		*dispositions.entry(d.split(" (").next().unwrap().to_string()).or_insert(0usize) += 1;
	}
	assert_eq!(cases, surface["oracle_cases"].as_u64().unwrap() as usize, "case dispositions must equal emitted oracle cases");
	assert_eq!(dispositions.values().sum::<usize>(), generated, "dispositions must partition the generated entries");
	eprintln!("execution dispositions: {dispositions:?}");
}

