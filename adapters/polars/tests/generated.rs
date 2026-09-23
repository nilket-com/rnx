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
	// Record 0081: the adapter is documented with the four narrow integer
	// dtype features; the 0080 `0.55.2-adapter` inventory stays as a baseline.
	let p = root().join("probes/0072/out/0.55.2-adapter-narrow/result/inventory.json");
	assert!(p.exists(), "missing {}: run probes/0072/inventory/doc.sh 0.55.2 adapter-narrow and extract it first", p.display());
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
		.args(["--buckets", BUCKETS, "--release"])
		.arg(root().join("tools/polars-gen/releases").join(RELEASE_FILE))
		.arg("--check")
		.env("CARGO_TARGET_DIR", root().join("target/0073"))
		.status()
		.expect("run polars-gen");
	assert!(status.success(), "generated files differ from the generator's output; regenerate and commit");
}

/// Record 0075: a release policy that does not belong to the inventory is
/// refused before anything is generated, whichever way the mismatch goes.
#[test]
fn a_release_file_for_another_inventory_is_refused() {
	for wrong in ["rc2.toml", "0.54.4.toml"] {
		let out = Command::new("cargo")
			.args(["run", "-q", "--locked", "--manifest-path"])
			.arg(root().join("tools/polars-gen/Cargo.toml"))
			.arg("--")
			.arg(inventory())
			.arg(root().join("adapters/polars"))
			.args(["--buckets", BUCKETS, "--release"])
			.arg(root().join("tools/polars-gen/releases").join(wrong))
			.arg("--check")
			.env("CARGO_TARGET_DIR", root().join("target/0073"))
			.output()
			.expect("run polars-gen");
		let stderr = String::from_utf8_lossy(&out.stderr);
		assert!(!out.status.success(), "{wrong} against the 0.55.2 inventory must be refused");
		assert!(stderr.contains("refusing to generate"), "{wrong}: refusal must name the provenance mismatch, got: {stderr}");
	}
}

/// Record 0081: the shipped release policy names the `adapter-narrow`
/// configuration and its dtype features; the previous adapter inventory,
/// documented without them, is refused so a feature change cannot be
/// reported against the denominator that predates it.
#[test]
fn the_previous_feature_configuration_is_refused() {
	let old = root().join("probes/0072/out/0.55.2-adapter/result/inventory.json");
	assert!(old.exists(), "missing baseline {}", old.display());
	let out = Command::new("cargo")
		.args(["run", "-q", "--locked", "--manifest-path"])
		.arg(root().join("tools/polars-gen/Cargo.toml"))
		.arg("--")
		.arg(old)
		.arg(root().join("adapters/polars"))
		.args(["--buckets", BUCKETS, "--release"])
		.arg(root().join("tools/polars-gen/releases").join(RELEASE_FILE))
		.arg("--check")
		.env("CARGO_TARGET_DIR", root().join("target/0073"))
		.output()
		.expect("run polars-gen");
	let stderr = String::from_utf8_lossy(&out.stderr);
	assert!(!out.status.success(), "the 0.55.2-adapter inventory must be refused by the adapter-narrow policy");
	assert!(stderr.contains("refusing to generate") && stderr.contains("adapter-narrow"), "refusal must name the configuration mismatch, got: {stderr}");
}

/// Record 0081: with the right configuration label, an inventory whose
/// resolved feature set differs from the pinned one, by a missing base
/// feature or by an extra feature, is refused on the feature set itself.
#[test]
fn a_wrong_feature_set_under_the_right_configuration_is_refused() {
	let text = std::fs::read_to_string(inventory()).unwrap();
	let mut inv: serde_json::Value = serde_json::from_str(&text).unwrap();
	assert_eq!(inv["provenance"]["cfg"], "adapter-narrow");
	let pinned: Vec<String> = inv["provenance"]["features"].as_array().unwrap().iter().map(|f| f.as_str().unwrap().to_string()).collect();
	assert!(pinned.contains(&"lazy".to_string()) && pinned.contains(&"dtype-i8".to_string()), "{pinned:?}");
	let dir = root().join("target/0073/wrong-features");
	std::fs::create_dir_all(&dir).unwrap();
	let variants: [(&str, Vec<String>); 2] = [
		("missing-base", pinned.iter().filter(|f| *f != "lazy").cloned().collect()),
		("extra", pinned.iter().cloned().chain(["dtype-i128".to_string()]).collect()),
	];
	for (name, features) in variants {
		inv["provenance"]["features"] = serde_json::json!(features);
		let path = dir.join(format!("{name}.json"));
		std::fs::write(&path, serde_json::to_string(&inv).unwrap()).unwrap();
		let out = Command::new("cargo")
			.args(["run", "-q", "--locked", "--manifest-path"])
			.arg(root().join("tools/polars-gen/Cargo.toml"))
			.arg("--")
			.arg(&path)
			.arg(root().join("adapters/polars"))
			.args(["--buckets", BUCKETS, "--release"])
			.arg(root().join("tools/polars-gen/releases").join(RELEASE_FILE))
			.arg("--check")
			.env("CARGO_TARGET_DIR", root().join("target/0073"))
			.output()
			.expect("run polars-gen");
		let stderr = String::from_utf8_lossy(&out.stderr);
		assert!(!out.status.success(), "{name}: a wrong feature set under cfg adapter-narrow must be refused");
		assert!(stderr.contains("refusing to generate") && stderr.contains("feature set"), "{name}: refusal must name the feature set, got: {stderr}");
		let expected = if name == "missing-base" { "lacks [\"lazy\"]" } else { "adds [\"dtype-i128\"]" };
		assert!(stderr.contains(expected), "{name}: refusal must name the difference {expected}, got: {stderr}");
	}
}

/// Record 0092 review: a `[[method_scalar_generics]]` entry whose native does
/// not equal the type's `T::Native` (here `Int8Type` paired with `i64`) is
/// refused whole, end to end: the real generator, run on a copy of the
/// shipped release file into a scratch directory, emits no `lhs_sub` binding
/// and leaves its pairs unresolved.
#[test]
fn a_mispaired_scalar_native_emits_no_binding() {
	let shipped = std::fs::read_to_string(root().join("tools/polars-gen/releases").join(RELEASE_FILE)).unwrap();
	let at = shipped.find("key = \"polars_core:3719\"").expect("the lhs_sub entry");
	let from = at + shipped[at..].find("natives = [").unwrap();
	let to = from + shipped[from..].find(']').unwrap();
	let bad = format!("{}natives = [\"i64\", \"i16\", \"i32\", \"i64\", \"u8\", \"u16\", \"u32\", \"u64\", \"f32\", \"f64\"{}", &shipped[..from], &shipped[to..]);
	let dir = root().join("target/0073/mispaired-native");
	let _ = std::fs::remove_dir_all(&dir);
	std::fs::create_dir_all(&dir).unwrap();
	std::fs::write(dir.join("release.toml"), bad).unwrap();
	let status = Command::new("cargo")
		.args(["run", "-q", "--locked", "--manifest-path"])
		.arg(root().join("tools/polars-gen/Cargo.toml"))
		.arg("--")
		.arg(inventory())
		.arg(&dir)
		.args(["--buckets", BUCKETS, "--release"])
		.arg(dir.join("release.toml"))
		.env("CARGO_TARGET_DIR", root().join("target/0073"))
		.status()
		.expect("run polars-gen");
	assert!(status.success());
	let surface: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("surface.json")).unwrap()).unwrap();
	let entry = surface["entries"].as_array().unwrap().iter().find(|e| e["key"] == "polars_core:3719").unwrap();
	assert_eq!(entry["status"], "unsupported", "{entry}");
	for p in surface["instantiation"]["pairs"].as_array().unwrap().iter().filter(|p| p["key"] == "polars_core:3719") {
		assert!(p["disposition"].as_str().unwrap().starts_with("unresolved"), "{p}");
	}
	let functions = std::fs::read_to_string(dir.join("src/generated/functions.rs")).unwrap();
	assert!(!functions.contains("lhs_sub"), "no lhs_sub binding text");
}

/// Record 0076 gate 2 controls on the committed surface: the deref route
/// keeps receiver forms, retains inherent names, and a trait method bound
/// on two receivers keeps two independent results.
#[test]
fn deref_route_controls() {
	let surface: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(root().join("adapters/polars/surface.json")).unwrap()).unwrap();
	let results: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(root().join("adapters/polars/oracle-results.json")).unwrap()).unwrap();
	let by_id: std::collections::HashMap<&str, &serde_json::Value> = results["results"].as_array().unwrap().iter().map(|r| (r["id"].as_str().unwrap(), r)).collect();
	let mut deref_bindings = 0;
	let mut mut_exceptions = 0;
	let mut retained = 0;
	let mut two_outcomes = 0;
	// the names Series binds inherently: every SeriesTrait method with one
	// of these names must keep the inherent binding on the deref route
	let inherent: std::collections::BTreeSet<String> = surface["entries"].as_array().unwrap().iter()
		.filter(|e| e["status"] == "generated" && e["canonical_path"].as_str().unwrap().starts_with("polars_core::series::Series::"))
		.map(|e| e["canonical_path"].as_str().unwrap().rsplit("::").next().unwrap().to_string()).collect();
	let mut colliding = 0;
	for e in surface["entries"].as_array().unwrap() {
		let path = e["canonical_path"].as_str().unwrap();
		if !path.contains("SeriesTrait::") {
			continue;
		}
		if inherent.contains(path.rsplit("::").next().unwrap()) {
			colliding += 1;
		}
		let empty = vec![];
		let bindings = e["bindings"].as_array().unwrap_or(&empty);
		let exceptions = e["exceptions"].as_array().unwrap_or(&empty);
		for b in bindings {
			if b["route"] == "deref" {
				deref_bindings += 1;
				assert_eq!(b["receiver"], "polars_core::series::Series", "{path}: deref binding on an unexpected receiver");
				assert!(b["rune"].as_str().unwrap().starts_with("polars::Series::"), "{path}: deref binding not on polars::Series");
			}
		}
		for x in exceptions {
			if x["route"] != "deref" {
				continue;
			}
			let reason = x["reason"].as_str().unwrap();
			if reason.contains("needs DerefMut") {
				mut_exceptions += 1;
				assert!(bindings.iter().all(|b| b["route"] != "deref"), "{path}: a &mut self method must not have a deref binding");
			}
			if reason.starts_with("not separately exposed") {
				retained += 1;
				assert!(reason.contains("polars_core::series::Series::"), "{path}: the retained inherent binding must be named: {reason}");
			}
		}
		// two receivers, two results: both collected, judged apart, and the
		// null and data receivers recorded different approved results
		let cases: Vec<&str> = bindings.iter().filter_map(|b| b["case_id"].as_str()).collect();
		if cases.len() == 2 {
			let a = by_id[cases[0]];
			let b = by_id[cases[1]];
			assert_ne!(cases[0], cases[1]);
			if a["status"] != b["status"] || a["detail"] != b["detail"] {
				two_outcomes += 1;
			}
		}
	}
	assert!(deref_bindings > 0, "no SeriesTrait method reached Series through Deref");
	assert!(mut_exceptions > 0, "no &mut self SeriesTrait method was refused on the deref route (Series has no DerefMut)");
	assert_eq!(retained, colliding, "every SeriesTrait name that Series binds inherently must be retained with the trait route not separately exposed");
	assert!(colliding >= 8, "expected the inherent name collisions the inventory shows, got {colliding}");
	assert!(two_outcomes > 0, "expected at least one SeriesTrait method whose null and data receivers record different results");
	eprintln!("deref route: {deref_bindings} bindings on Series, {mut_exceptions} &mut self exceptions, {retained} inherent names retained, {two_outcomes} methods whose two receivers recorded different results");
}

/// The buckets this stage generates and the release input the committed
/// module was generated from; kept in one place with the drift test.
const BUCKETS: &str = "mechanical,conversion,option_struct,callback,generic_fn";
const RELEASE_FILE: &str = "0.55.2-joins.toml";

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
		if d.split(" (").next() == Some("case") {
			cases += 1;
		}
		*dispositions.entry(d.split(" (").next().unwrap().to_string()).or_insert(0usize) += 1;
	}
	assert_eq!(dispositions.values().sum::<usize>(), generated, "dispositions must partition the generated entries");
	eprintln!("execution dispositions: {dispositions:?}");
	// Record 0076: the binding level. Every generated entry has at least
	// one binding; every binding has exactly one disposition; the bindings
	// with a case equal the emitted cases; case ids are unique and are
	// exactly the ids in the generated harness.
	let mut binding_cases = std::collections::BTreeSet::new();
	let mut binding_ids = std::collections::BTreeSet::new();
	let mut multi = 0;
	for e in surface["entries"].as_array().unwrap() {
		if e["status"] != "generated" {
			continue;
		}
		let bs = e["bindings"].as_array().unwrap_or_else(|| panic!("{}: generated without bindings", e["canonical_path"]));
		assert!(!bs.is_empty(), "{}: generated without bindings", e["canonical_path"]);
		if bs.len() > 1 {
			multi += 1;
		}
		let mut entry_cases = 0;
		for b in bs {
			let id = b["id"].as_str().unwrap();
			assert!(binding_ids.insert(id.to_string()), "duplicate binding id {id}");
			let d = b["disposition"].as_str().unwrap_or_else(|| panic!("{id}: binding without a disposition"));
			if d.starts_with("case") {
				entry_cases += 1;
				let cid = b["case_id"].as_str().unwrap_or_else(|| panic!("{id}: case without a case id"));
				assert!(binding_cases.insert(cid.to_string()), "duplicate case id {cid}");
			} else {
				assert!(b["case_id"].is_null(), "{id}: a case id without a case");
			}
		}
		let d = e["execution"].as_str().unwrap();
		assert_eq!(d.starts_with("case"), entry_cases > 0, "{}: entry disposition {d} disagrees with its bindings", e["canonical_path"]);
	}
	assert_eq!(binding_cases.len(), surface["oracle_cases"].as_u64().unwrap() as usize, "binding cases must equal emitted oracle cases");
	assert_eq!(cases, surface["entries"].as_array().unwrap().iter().filter(|e| e["status"] == "generated" && e["execution"].as_str().map(|d| d.starts_with("case")).unwrap_or(false)).count());
	let harness = std::fs::read_to_string(root().join("adapters/polars/tests/generated_oracle.rs")).unwrap();
	let mut harness_ids = std::collections::BTreeSet::new();
	for line in harness.lines().filter(|l| l.trim_start().starts_with("Case { id: \"")) {
		let id = line.split("id: \"").nth(1).unwrap().split('"').next().unwrap().to_string();
		assert!(harness_ids.insert(id.clone()), "duplicate case id in the harness: {id}");
	}
	assert_eq!(harness_ids, binding_cases, "the harness's case ids must be exactly the bindings' case ids");
	eprintln!("bindings: {} ids, {} cases, {multi} entries with several receivers", binding_ids.len(), binding_cases.len());
	// Record 0076: every pair of the instantiation census has exactly one
	// disposition, and the proven pairs partition into emitted, refused,
	// excluded, not shipped and not eligible; an emitted pair has a binding.
	let mut proven_disp = std::collections::BTreeMap::new();
	let mut emitted_pairs = 0;
	for p in surface["instantiation"]["pairs"].as_array().unwrap() {
		let d = p["disposition"].as_str().unwrap_or_else(|| panic!("{} on {}: pair without a disposition", p["method"], p["alias"]));
		let head = d.split(':').next().unwrap();
		match p["result"].as_str().unwrap() {
			"Proven" => {
				assert!(["emitted", "refused", "excluded", "not shipped", "not eligible"].contains(&head), "{} on {}: proven pair with disposition {d}", p["method"], p["alias"]);
				*proven_disp.entry(head.to_string()).or_insert(0usize) += 1;
				if head == "emitted" {
					emitted_pairs += 1;
				}
			}
			other => assert_eq!(head, other.to_lowercase(), "{} on {}: {other} pair with disposition {d}", p["method"], p["alias"]),
		}
	}
	let instantiation_bindings = surface["entries"].as_array().unwrap().iter().flat_map(|e| e["bindings"].as_array().cloned().unwrap_or_default()).filter(|b| b["route"] == "instantiation").count();
	assert_eq!(emitted_pairs, instantiation_bindings, "emitted pairs must equal instantiation bindings");
	let proven = surface["instantiation"]["pairs"].as_array().unwrap().iter().filter(|p| p["result"] == "Proven").count();
	assert_eq!(proven_disp.values().sum::<usize>(), proven, "proven-pair dispositions must partition the proven pairs");
	eprintln!("proven pairs: {proven_disp:?}");
}
