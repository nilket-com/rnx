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
	let (surface, functions) = generate_with_natives("polars_core:3719", "[\"i64\", \"i16\", \"i32\", \"i64\", \"u8\", \"u16\", \"u32\", \"u64\", \"f32\", \"f64\"", "mispaired-native");
	let entry = surface["entries"].as_array().unwrap().iter().find(|e| e["key"] == "polars_core:3719").unwrap();
	assert_eq!(entry["status"], "unsupported", "{entry}");
	for p in surface["instantiation"]["pairs"].as_array().unwrap().iter().filter(|p| p["key"] == "polars_core:3719") {
		assert!(p["disposition"].as_str().unwrap().starts_with("unresolved"), "{p}");
	}
	assert!(!functions.contains("lhs_sub"), "no lhs_sub binding text");
	// record 0095: the same fail-closed check guards lhs_div, and one bad entry does not touch its neighbour
	let (surface, functions) = generate_with_natives("polars_core:3722", "[\"i64\", \"i16\", \"i32\", \"i64\", \"u8\", \"u16\", \"u32\", \"u64\", \"f32\", \"f64\"", "mispaired-native-div");
	let entry = surface["entries"].as_array().unwrap().iter().find(|e| e["key"] == "polars_core:3722").unwrap();
	assert_eq!(entry["status"], "unsupported", "{entry}");
	assert!(!functions.contains("lhs_div"), "no lhs_div binding text");
	assert_eq!(functions.matches("#[rune::function(instance, path = lhs_rem)]").count(), 10, "lhs_rem is unaffected");
}

/// Record 0095: a type left out of the entry is refused by name while the
/// listed types still bind.
#[test]
fn an_unlisted_scalar_type_is_refused_by_name() {
	let shipped = std::fs::read_to_string(root().join("tools/polars-gen/releases").join(RELEASE_FILE)).unwrap();
	let at = shipped.find("key = \"polars_core:3723\"").expect("the lhs_rem entry");
	let entry = &shipped[at..at + shipped[at..].find("cite = ").unwrap()];
	let fixed = entry.replace("\"polars_core::datatypes::Int64Type\", ", "").replace("\"i64\", ", "");
	assert_ne!(fixed, entry, "the Int64 pair was listed");
	let (surface, functions) = generate_release(&shipped.replacen(entry, &fixed, 1), "unlisted-scalar-type");
	let entry = surface["entries"].as_array().unwrap().iter().find(|e| e["key"] == "polars_core:3723").unwrap();
	assert_eq!(entry["status"], "generated", "{entry}");
	assert_eq!(functions.matches("#[rune::function(instance, path = lhs_rem)]").count(), 9, "nine listed types still bind");
	assert!(!functions.contains("lhs_rem_polars_core__datatypes__int64chunked"), "no Int64 lhs_rem binding");
	let int64 = surface["instantiation"]["pairs"].as_array().unwrap().iter().find(|p| p["key"] == "polars_core:3723" && p["alias"].as_str().is_some_and(|a| a.ends_with("::Int64Chunked"))).expect("the Int64 pair");
	assert!(!int64["disposition"].as_str().unwrap().starts_with("proven"), "{int64}");
}

/// Record 0096: the real generator on a modified release file. An entry
/// without a citation lifts nothing and emits no text; an entry without
/// `Int64Type` binds the other nine and refuses Int64 by name.
#[test]
fn null_aware_entries_fail_closed_and_refuse_unlisted_types() {
	let shipped = std::fs::read_to_string(root().join("tools/polars-gen/releases").join(RELEASE_FILE)).unwrap();
	let at = shipped.find("key = \"polars_core:3658\"").expect("the to_vec_null_aware entry");
	let entry = &shipped[at..at + shipped[at..].find("\n\n").unwrap()];
	let pairs = |surface: &serde_json::Value| surface["instantiation"]["pairs"].as_array().unwrap().iter().filter(|p| p["key"] == "polars_core:3658").cloned().collect::<Vec<_>>();
	let cite_at = entry.find("cite = ").unwrap();
	let uncited = format!("{}cite = \" \"", &entry[..cite_at]);
	let (surface, functions) = generate_release(&shipped.replacen(entry, &uncited, 1), "null-aware-uncited");
	assert!(!functions.contains("to_vec_null_aware"), "no binding text");
	let refused: Vec<_> = pairs(&surface).into_iter().filter(|p| p["disposition"].as_str().unwrap().contains("null-aware return: no citation")).collect();
	assert_eq!(refused.len(), 10, "every numeric pair names the malformed entry");
	let fixed = entry.replace("\"polars_core::datatypes::Int64Type\", ", "");
	assert_ne!(fixed, entry);
	let (surface, functions) = generate_release(&shipped.replacen(entry, &fixed, 1), "null-aware-unlisted");
	assert_eq!(functions.matches("#[rune::function(instance, path = to_vec_null_aware)]").count(), 9);
	let int64 = pairs(&surface).into_iter().find(|p| p["alias"].as_str().is_some_and(|a| a.ends_with("::Int64Chunked"))).unwrap();
	assert!(int64["disposition"].as_str().unwrap().contains("null-aware return: `polars_core::chunked_array::ChunkedArray<polars_core::datatypes::Int64Type>` is not a listed type"), "{int64}");
}

/// Record 0097: the sized-self gate on the real generator. In a modified
/// inventory `head` returns `PolarsResult<Self>`; in a modified release file
/// `tail` has no citation. Neither emits any text, both name the fault on
/// all 16 pairs, and `limit` still binds on all 16.
#[test]
fn sized_self_drift_is_refused_by_name() {
	let shipped = std::fs::read_to_string(root().join("tools/polars-gen/releases").join(RELEASE_FILE)).unwrap();
	let at = shipped.find("key = \"polars_core:3501\"").expect("the tail entry");
	let cite = at + shipped[at..].find("cite = ").unwrap();
	let end = cite + shipped[cite..].find('\n').unwrap();
	let release = format!("{}cite = \" \"{}", &shipped[..cite], &shipped[end..]);
	let mut inv: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(inventory()).unwrap()).unwrap();
	let head = inv["callables"].as_array_mut().unwrap().iter_mut().find(|c| c["key"] == "polars_core:3500").expect("head");
	assert_eq!(head["ret_canonical"], "Self");
	head["ret_canonical"] = "polars_error::PolarsResult<Self>".into();
	let (surface, functions) = generate_with(&release, Some(&serde_json::to_string(&inv).unwrap()), "sized-self-drift");
	let pairs = |key: &str| surface["instantiation"]["pairs"].as_array().unwrap().iter().filter(|p| p["key"] == key).map(|p| p["disposition"].as_str().unwrap().to_string()).collect::<Vec<_>>();
	assert!(pairs("polars_core:3500").iter().all(|d| d.contains("sized-self method: return polars_error::PolarsResult<Self> is not Self")), "{:?}", pairs("polars_core:3500"));
	assert!(pairs("polars_core:3501").iter().all(|d| d.contains("sized-self method: no citation")), "{:?}", pairs("polars_core:3501"));
	assert_eq!(pairs("polars_core:3500").len(), 16);
	assert!(!functions.contains("ChunkedArray::head`") && !functions.contains("ChunkedArray::tail`"), "no head or tail binding text");
	assert_eq!(functions.matches("ChunkedArray::limit`").count(), 16, "limit is unaffected");
}

/// Record 0098: the external-bound gate on the real generator. Run A's
/// inventory drops `Canonical` from `to_canonical` and adds an external bound
/// to `is_nan`; its release file swaps `is_finite`'s natives and lists only
/// Float32 for `is_infinite`. Run B's inventory replaces `Canonical`. Each
/// fault is named on its pairs, with no binding text, and every other
/// float method still binds on both types.
#[test]
fn external_bound_drift_is_refused_by_name() {
	let shipped = std::fs::read_to_string(root().join("tools/polars-gen/releases").join(RELEASE_FILE)).unwrap();
	let entry = |key: &str| { let at = shipped.find(&format!("key = \"{key}\"")).unwrap(); let end = at + shipped[at..].find("cite = ").unwrap(); (at, end) };
	let mut release = shipped.clone();
	let (a, b) = entry("polars_core:3548");
	release.replace_range(a..b, &shipped[a..b].replace("[[\"polars_core::datatypes::Float32Type\", \"f32\"], [\"polars_core::datatypes::Float64Type\", \"f64\"]]", "[[\"polars_core::datatypes::Float32Type\", \"f64\"], [\"polars_core::datatypes::Float64Type\", \"f32\"]]"));
	let (a, b) = { let at = release.find("key = \"polars_core:3549\"").unwrap(); (at, at + release[at..].find("cite = ").unwrap()) };
	let fixed = release[a..b].replace(", [\"polars_core::datatypes::Float64Type\", \"f64\"]", "");
	assert_ne!(fixed, release[a..b]);
	release.replace_range(a..b, &fixed);
	let base: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(inventory()).unwrap()).unwrap();
	let edit = |inv: &mut serde_json::Value, key: &str, wh: serde_json::Value| { let c = inv["callables"].as_array_mut().unwrap().iter_mut().find(|c| c["key"] == key).unwrap(); c["impl_where"] = wh; };
	let mut inv = base.clone();
	edit(&mut inv, "polars_core:3554", serde_json::json!(["T: polars_core::datatypes::PolarsFloatType", "T::Native: num_traits::float::Float"]));
	edit(&mut inv, "polars_core:3546", serde_json::json!(["T: polars_core::datatypes::PolarsFloatType", "T::Native: num_traits::float::Float", "T::Native: core::fmt::LowerExp"]));
	let (surface, functions) = generate_with(&release, Some(&serde_json::to_string(&inv).unwrap()), "external-bound-drift");
	let pairs = |surface: &serde_json::Value, key: &str| surface["instantiation"]["pairs"].as_array().unwrap().iter().filter(|p| p["key"] == key && p["alias"].as_str().unwrap().contains("Float")).map(|p| p["disposition"].as_str().unwrap().to_string()).collect::<Vec<_>>();
	let count = |f: &str, m: &str| f.matches(&format!("ChunkedArray::{m}`")).count();
	for (key, method, why) in [("polars_core:3554", "to_canonical", "external bound: where-clauses"), ("polars_core:3546", "is_nan", "external bound: where-clauses"), ("polars_core:3548", "is_finite", "is not a float type and its native")] {
		let p = pairs(&surface, key);
		assert_eq!(p.len(), 2, "{method}");
		assert!(p.iter().all(|d| d.contains(why)), "{method}: {p:?}");
		assert_eq!(count(&functions, method), 0, "{method}: no binding text");
	}
	let infinite = pairs(&surface, "polars_core:3549");
	assert!(infinite.iter().any(|d| d == "emitted") && infinite.iter().any(|d| d.contains("external bound: `polars_core::chunked_array::ChunkedArray<polars_core::datatypes::Float64Type>` is not a listed pair")), "{infinite:?}");
	assert_eq!(count(&functions, "is_infinite"), 1, "only the listed Float32 pair binds");
	for m in ["is_not_nan", "none_to_nan"] { assert_eq!(count(&functions, m), 2, "{m} is unaffected"); }
	let mut inv = base;
	edit(&mut inv, "polars_core:3554", serde_json::json!(["T: polars_core::datatypes::PolarsFloatType", "T::Native: num_traits::float::Float + polars_core::other::Canonical"]));
	let (surface, functions) = generate_with(&shipped, Some(&serde_json::to_string(&inv).unwrap()), "external-bound-replaced");
	assert!(pairs(&surface, "polars_core:3554").iter().all(|d| d.contains("external bound: where-clauses")));
	assert_eq!(count(&functions, "to_canonical"), 0);
	assert_eq!(count(&functions, "is_finite"), 2, "the shipped entries bind");
}

/// Record 0099: the chunk-snapshot gate on the real generator (13 of the
/// 14 shipped pairs are listed below the one removed). Without the
/// Int64 pair the others bind and Int64 is named; a String pair given the
/// binary kind refuses all 16; without Boolean 13 bind; a blank citation, or an
/// inventory whose `chunks` returns an owned vector, refuses all ten by
/// name with no binding text.
#[test]
fn chunk_snapshot_drift_is_refused_by_name() {
	let shipped = std::fs::read_to_string(root().join("tools/polars-gen/releases").join(RELEASE_FILE)).unwrap();
	let at = shipped.find("key = \"polars_core:3681\"").expect("the chunks entry");
	let cite = at + shipped[at..].find("cite = ").unwrap();
	let end = cite + shipped[cite..].find('\n').unwrap();
	let count = |f: &str| f.matches("ChunkedArray::chunks`").count();
	let pairs = |surface: &serde_json::Value| surface["instantiation"]["pairs"].as_array().unwrap().iter().filter(|p| p["key"] == "polars_core:3681").map(|p| (p["alias"].as_str().unwrap().to_string(), p["disposition"].as_str().unwrap().to_string())).collect::<Vec<_>>();
	let without = shipped[at..cite].replace(", [\"polars_core::datatypes::Int64Type\", \"i64\"]", "");
	assert_ne!(without, shipped[at..cite]);
	let (surface, functions) = generate_release(&shipped.replacen(&shipped[at..cite], &without, 1), "chunk-snapshot-unlisted");
	assert_eq!(count(&functions), 13);
	assert!(pairs(&surface).iter().any(|(a, d)| a.ends_with("::Int64Chunked") && d.contains("chunk snapshot: `polars_core::chunked_array::ChunkedArray<polars_core::datatypes::Int64Type>` is not a listed pair")));
	// record 0100: a scalar owner with the wrong kind is named; without Boolean, 13 bind
	let mispaired = shipped[at..cite].replace("[\"polars_core::datatypes::StringType\", \"str\"]", "[\"polars_core::datatypes::StringType\", \"binary\"]");
	assert_ne!(mispaired, shipped[at..cite]);
	let (surface, functions) = generate_release(&shipped.replacen(&shipped[at..cite], &mispaired, 1), "chunk-snapshot-mispaired");
	assert_eq!(count(&functions), 0, "a malformed entry emits no text");
	assert_eq!(pairs(&surface).iter().filter(|(_, d)| d.contains("is not a numeric type and its native, nor a listed scalar owner and its kind")).count(), 16);
	let no_bool = shipped[at..cite].replace(", [\"polars_core::datatypes::BooleanType\", \"bool\"]", "");
	assert_ne!(no_bool, shipped[at..cite]);
	let (surface, functions) = generate_release(&shipped.replacen(&shipped[at..cite], &no_bool, 1), "chunk-snapshot-no-bool");
	assert_eq!(count(&functions), 13);
	assert!(pairs(&surface).iter().any(|(a, d)| a.ends_with("::BooleanChunked") && d.contains("is not a listed pair")));
	let (surface, functions) = generate_release(&format!("{}cite = \" \"{}", &shipped[..cite], &shipped[end..]), "chunk-snapshot-uncited");
	assert_eq!(count(&functions), 0, "no binding text");
	assert_eq!(pairs(&surface).iter().filter(|(_, d)| d.contains("chunk snapshot: no citation")).count(), 16);
	let mut inv: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(inventory()).unwrap()).unwrap();
	let c = inv["callables"].as_array_mut().unwrap().iter_mut().find(|c| c["key"] == "polars_core:3681").unwrap();
	c["ret_canonical"] = "alloc::vec::Vec<polars_arrow::array::ArrayRef>".into();
	let (surface, functions) = generate_with(&shipped, Some(&serde_json::to_string(&inv).unwrap()), "chunk-snapshot-owned");
	assert_eq!(count(&functions), 0, "no binding text");
	assert_eq!(pairs(&surface).iter().filter(|(_, d)| d.contains("chunk snapshot: return alloc::vec::Vec<polars_arrow::array::ArrayRef> is not")).count(), 16);
}

/// Record 0101: the indexed-chunk gate on the real generator. A blank
/// citation or a String pair given the binary kind refuses all 16 by name
/// with no text; without the Int64 pair 13 bind and Int64 is named; an
/// inventory whose `downcast_get` returns a non-optional borrow refuses all.
#[test]
fn indexed_chunk_drift_is_refused_by_name() {
	let shipped = std::fs::read_to_string(root().join("tools/polars-gen/releases").join(RELEASE_FILE)).unwrap();
	let at = shipped.find("key = \"polars_core:3509\"").expect("the downcast_get entry");
	let cite = at + shipped[at..].find("cite = ").unwrap();
	let end = cite + shipped[cite..].find('\n').unwrap();
	let count = |f: &str| f.matches("ChunkedArray::downcast_get`").count();
	let pairs = |surface: &serde_json::Value| surface["instantiation"]["pairs"].as_array().unwrap().iter().filter(|p| p["key"] == "polars_core:3509").map(|p| (p["alias"].as_str().unwrap().to_string(), p["disposition"].as_str().unwrap().to_string())).collect::<Vec<_>>();
	let (surface, functions) = generate_release(&format!("{}cite = \" \"{}", &shipped[..cite], &shipped[end..]), "indexed-uncited");
	assert_eq!(count(&functions), 0);
	assert_eq!(pairs(&surface).iter().filter(|(_, d)| d.contains("indexed chunk snapshot: no citation")).count(), 16);
	let mispaired = shipped[at..cite].replace("[\"polars_core::datatypes::StringType\", \"str\"]", "[\"polars_core::datatypes::StringType\", \"binary\"]");
	assert_ne!(mispaired, shipped[at..cite]);
	let (surface, functions) = generate_release(&shipped.replacen(&shipped[at..cite], &mispaired, 1), "indexed-mispaired");
	assert_eq!(count(&functions), 0);
	assert_eq!(pairs(&surface).iter().filter(|(_, d)| d.contains("nor a listed scalar owner and its kind")).count(), 16);
	let without = shipped[at..cite].replace(", [\"polars_core::datatypes::Int64Type\", \"i64\"]", "");
	assert_ne!(without, shipped[at..cite]);
	let (surface, functions) = generate_release(&shipped.replacen(&shipped[at..cite], &without, 1), "indexed-unlisted");
	assert_eq!(count(&functions), 13);
	assert!(pairs(&surface).iter().any(|(a, d)| a.ends_with("::Int64Chunked") && d.contains("indexed chunk snapshot: `polars_core::chunked_array::ChunkedArray<polars_core::datatypes::Int64Type>` is not a listed pair")));
	let mut inv: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(inventory()).unwrap()).unwrap();
	let c = inv["callables"].as_array_mut().unwrap().iter_mut().find(|c| c["key"] == "polars_core:3509").unwrap();
	c["ret_canonical"] = "&T::Array".into();
	let (surface, functions) = generate_with(&shipped, Some(&serde_json::to_string(&inv).unwrap()), "indexed-borrowed");
	assert_eq!(count(&functions), 0);
	assert_eq!(pairs(&surface).iter().filter(|(_, d)| d.contains("indexed chunk snapshot: return &T::Array is not")).count(), 16);
}

/// Record 0102: the array-snapshot gate on the real generator: a blank
/// citation, a mispaired kind, a missing Int64 pair and an inventory whose
/// `downcast_as_array` returns an optional array are each refused by name.
#[test]
fn array_snapshot_drift_is_refused_by_name() {
	let shipped = std::fs::read_to_string(root().join("tools/polars-gen/releases").join(RELEASE_FILE)).unwrap();
	let at = shipped.find("key = \"polars_core:3510\"").expect("the downcast_as_array entry");
	let cite = at + shipped[at..].find("cite = ").unwrap();
	let end = cite + shipped[cite..].find('\n').unwrap();
	let count = |f: &str| f.matches("ChunkedArray::downcast_as_array`").count();
	let pairs = |surface: &serde_json::Value| surface["instantiation"]["pairs"].as_array().unwrap().iter().filter(|p| p["key"] == "polars_core:3510").map(|p| (p["alias"].as_str().unwrap().to_string(), p["disposition"].as_str().unwrap().to_string())).collect::<Vec<_>>();
	let (surface, functions) = generate_release(&format!("{}cite = \" \"{}", &shipped[..cite], &shipped[end..]), "array-uncited");
	assert_eq!(count(&functions), 0);
	assert_eq!(pairs(&surface).iter().filter(|(_, d)| d.contains("array snapshot: no citation")).count(), 16);
	let mispaired = shipped[at..cite].replace("[\"polars_core::datatypes::BooleanType\", \"bool\"]", "[\"polars_core::datatypes::BooleanType\", \"u8\"]");
	assert_ne!(mispaired, shipped[at..cite]);
	let (surface, functions) = generate_release(&shipped.replacen(&shipped[at..cite], &mispaired, 1), "array-mispaired");
	assert_eq!(count(&functions), 0);
	assert_eq!(pairs(&surface).iter().filter(|(_, d)| d.contains("nor a listed scalar owner and its kind")).count(), 16);
	let without = shipped[at..cite].replace(", [\"polars_core::datatypes::Int64Type\", \"i64\"]", "");
	let (surface, functions) = generate_release(&shipped.replacen(&shipped[at..cite], &without, 1), "array-unlisted");
	assert_eq!(count(&functions), 13);
	assert!(pairs(&surface).iter().any(|(a, d)| a.ends_with("::Int64Chunked") && d.contains("array snapshot: `polars_core::chunked_array::ChunkedArray<polars_core::datatypes::Int64Type>` is not a listed pair")));
	let mut inv: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(inventory()).unwrap()).unwrap();
	let c = inv["callables"].as_array_mut().unwrap().iter_mut().find(|c| c["key"] == "polars_core:3510").unwrap();
	c["ret_canonical"] = "core::option::Option<&T::Array>".into();
	let (surface, functions) = generate_with(&shipped, Some(&serde_json::to_string(&inv).unwrap()), "array-optional");
	assert_eq!(count(&functions), 0);
	assert_eq!(pairs(&surface).iter().filter(|(_, d)| d.contains("array snapshot: return core::option::Option<&T::Array> is not &T::Array")).count(), 16);
}

/// Record 0103: the iterator-snapshot gate on the real generator: a blank
/// citation, a mispaired kind, a missing Int64 pair, and an inventory whose
/// `downcast_iter` item becomes an owned array, are each refused by name.
#[test]
fn iter_snapshot_drift_is_refused_by_name() {
	let shipped = std::fs::read_to_string(root().join("tools/polars-gen/releases").join(RELEASE_FILE)).unwrap();
	let at = shipped.find("key = \"polars_core:3505\"").expect("the downcast_iter entry");
	let cite = at + shipped[at..].find("cite = ").unwrap();
	let end = cite + shipped[cite..].find('\n').unwrap();
	let count = |f: &str| f.matches("ChunkedArray::downcast_iter`").count();
	let pairs = |surface: &serde_json::Value| surface["instantiation"]["pairs"].as_array().unwrap().iter().filter(|p| p["key"] == "polars_core:3505").map(|p| (p["alias"].as_str().unwrap().to_string(), p["disposition"].as_str().unwrap().to_string())).collect::<Vec<_>>();
	let (surface, functions) = generate_release(&format!("{}cite = \" \"{}", &shipped[..cite], &shipped[end..]), "iter-uncited");
	assert_eq!(count(&functions), 0);
	assert_eq!(pairs(&surface).iter().filter(|(_, d)| d.contains("iterator snapshot: no citation")).count(), 16);
	let mispaired = shipped[at..cite].replace("[\"polars_core::datatypes::BinaryType\", \"binary\"]", "[\"polars_core::datatypes::BinaryType\", \"str\"]");
	assert_ne!(mispaired, shipped[at..cite]);
	let (surface, functions) = generate_release(&shipped.replacen(&shipped[at..cite], &mispaired, 1), "iter-mispaired");
	assert_eq!(count(&functions), 0);
	assert_eq!(pairs(&surface).iter().filter(|(_, d)| d.contains("nor a listed scalar owner and its kind")).count(), 16);
	let without = shipped[at..cite].replace(", [\"polars_core::datatypes::Int64Type\", \"i64\"]", "");
	let (surface, functions) = generate_release(&shipped.replacen(&shipped[at..cite], &without, 1), "iter-unlisted");
	assert_eq!(count(&functions), 13);
	assert!(pairs(&surface).iter().any(|(a, d)| a.ends_with("::Int64Chunked") && d.contains("iterator snapshot: `polars_core::chunked_array::ChunkedArray<polars_core::datatypes::Int64Type>` is not a listed pair")));
	let mut inv: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(inventory()).unwrap()).unwrap();
	let c = inv["callables"].as_array_mut().unwrap().iter_mut().find(|c| c["key"] == "polars_core:3505").unwrap();
	c["ret_canonical"] = "impl core::iter::traits::double_ended::DoubleEndedIterator<Item = T::Array>".into();
	let (surface, functions) = generate_with(&shipped, Some(&serde_json::to_string(&inv).unwrap()), "iter-owned-item");
	assert_eq!(count(&functions), 0);
	assert_eq!(pairs(&surface).iter().filter(|(_, d)| d.contains("iterator snapshot: return impl core::iter::traits::double_ended::DoubleEndedIterator<Item = T::Array> is not")).count(), 16);
}

/// Record 0104: the view-snapshot gate on the real generator: a blank
/// citation, a mispaired kind, a missing Int64 pair, and an inventory whose
/// `downcast_chunks` view holds `T` rather than `T::Array`, are each refused
/// by name with no binding text.
#[test]
fn view_snapshot_drift_is_refused_by_name() {
	let shipped = std::fs::read_to_string(root().join("tools/polars-gen/releases").join(RELEASE_FILE)).unwrap();
	let at = shipped.find("key = \"polars_core:3508\"").expect("the downcast_chunks entry");
	let cite = at + shipped[at..].find("cite = ").unwrap();
	let end = cite + shipped[cite..].find('\n').unwrap();
	let count = |f: &str| f.matches("ChunkedArray::downcast_chunks`").count();
	let pairs = |surface: &serde_json::Value| surface["instantiation"]["pairs"].as_array().unwrap().iter().filter(|p| p["key"] == "polars_core:3508").map(|p| (p["alias"].as_str().unwrap().to_string(), p["disposition"].as_str().unwrap().to_string())).collect::<Vec<_>>();
	let (surface, functions) = generate_release(&format!("{}cite = \" \"{}", &shipped[..cite], &shipped[end..]), "view-uncited");
	assert_eq!(count(&functions), 0);
	assert_eq!(pairs(&surface).iter().filter(|(_, d)| d.contains("view snapshot: no citation")).count(), 16);
	let mispaired = shipped[at..cite].replace("[\"polars_core::datatypes::Float32Type\", \"f32\"]", "[\"polars_core::datatypes::Float32Type\", \"f64\"]");
	assert_ne!(mispaired, shipped[at..cite]);
	let (surface, functions) = generate_release(&shipped.replacen(&shipped[at..cite], &mispaired, 1), "view-mispaired");
	assert_eq!(count(&functions), 0);
	assert_eq!(pairs(&surface).iter().filter(|(_, d)| d.contains("nor a listed scalar owner and its kind")).count(), 16);
	let without = shipped[at..cite].replace(", [\"polars_core::datatypes::Int64Type\", \"i64\"]", "");
	let (surface, functions) = generate_release(&shipped.replacen(&shipped[at..cite], &without, 1), "view-unlisted");
	assert_eq!(count(&functions), 13);
	assert!(pairs(&surface).iter().any(|(a, d)| a.ends_with("::Int64Chunked") && d.contains("view snapshot: `polars_core::chunked_array::ChunkedArray<polars_core::datatypes::Int64Type>` is not a listed pair")));
	let mut inv: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(inventory()).unwrap()).unwrap();
	let c = inv["callables"].as_array_mut().unwrap().iter_mut().find(|c| c["key"] == "polars_core:3508").unwrap();
	c["ret_canonical"] = "polars_core::chunked_array::ops::downcast::Chunks<T>".into();
	let (surface, functions) = generate_with(&shipped, Some(&serde_json::to_string(&inv).unwrap()), "view-changed-element");
	assert_eq!(count(&functions), 0);
	assert_eq!(pairs(&surface).iter().filter(|(_, d)| d.contains("view snapshot: return polars_core::chunked_array::ops::downcast::Chunks<T> is not")).count(), 16);
}

/// Record 0105: the owned-iterator gate on the real generator: a blank
/// citation, a mispaired kind, a missing Int64 pair, and an inventory whose
/// `downcast_into_iter` item becomes a borrow, are each refused by name.
#[test]
fn owned_iter_drift_is_refused_by_name() {
	let shipped = std::fs::read_to_string(root().join("tools/polars-gen/releases").join(RELEASE_FILE)).unwrap();
	let at = shipped.find("key = \"polars_core:3504\"").expect("the downcast_into_iter entry");
	let cite = at + shipped[at..].find("cite = ").unwrap();
	let end = cite + shipped[cite..].find('\n').unwrap();
	let count = |f: &str| f.matches("ChunkedArray::downcast_into_iter`").count();
	let pairs = |surface: &serde_json::Value| surface["instantiation"]["pairs"].as_array().unwrap().iter().filter(|p| p["key"] == "polars_core:3504").map(|p| (p["alias"].as_str().unwrap().to_string(), p["disposition"].as_str().unwrap().to_string())).collect::<Vec<_>>();
	let (surface, functions) = generate_release(&format!("{}cite = \" \"{}", &shipped[..cite], &shipped[end..]), "owned-uncited");
	assert_eq!(count(&functions), 0);
	assert_eq!(pairs(&surface).iter().filter(|(_, d)| d.contains("owned iterator snapshot: no citation")).count(), 16);
	let mispaired = shipped[at..cite].replace("[\"polars_core::datatypes::UInt8Type\", \"u8\"]", "[\"polars_core::datatypes::UInt8Type\", \"i8\"]");
	assert_ne!(mispaired, shipped[at..cite]);
	let (surface, functions) = generate_release(&shipped.replacen(&shipped[at..cite], &mispaired, 1), "owned-mispaired");
	assert_eq!(count(&functions), 0);
	assert_eq!(pairs(&surface).iter().filter(|(_, d)| d.contains("nor a listed scalar owner and its kind")).count(), 16);
	let without = shipped[at..cite].replace(", [\"polars_core::datatypes::Int64Type\", \"i64\"]", "");
	let (surface, functions) = generate_release(&shipped.replacen(&shipped[at..cite], &without, 1), "owned-unlisted");
	assert_eq!(count(&functions), 13);
	assert!(pairs(&surface).iter().any(|(a, d)| a.ends_with("::Int64Chunked") && d.contains("owned iterator snapshot: `polars_core::chunked_array::ChunkedArray<polars_core::datatypes::Int64Type>` is not a listed pair")));
	let mut inv: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(inventory()).unwrap()).unwrap();
	let c = inv["callables"].as_array_mut().unwrap().iter_mut().find(|c| c["key"] == "polars_core:3504").unwrap();
	c["ret_canonical"] = "impl core::iter::traits::double_ended::DoubleEndedIterator<Item = &T::Array>".into();
	let (surface, functions) = generate_with(&shipped, Some(&serde_json::to_string(&inv).unwrap()), "owned-borrowed-item");
	assert_eq!(count(&functions), 0);
	assert_eq!(pairs(&surface).iter().filter(|(_, d)| d.contains("owned iterator snapshot: return impl core::iter::traits::double_ended::DoubleEndedIterator<Item = &T::Array> is not")).count(), 16);
}

fn generate_with_natives(key: &str, natives: &str, dir: &str) -> (serde_json::Value, String) {
	let shipped = std::fs::read_to_string(root().join("tools/polars-gen/releases").join(RELEASE_FILE)).unwrap();
	let at = shipped.find(&format!("key = \"{key}\"")).expect("the entry");
	let from = at + shipped[at..].find("natives = [").unwrap();
	let to = from + shipped[from..].find(']').unwrap();
	generate_release(&format!("{}natives = {natives}{}", &shipped[..from], &shipped[to..]), dir)
}

/// Runs the real generator on a modified copy of the shipped release file.
fn generate_release(release: &str, name: &str) -> (serde_json::Value, String) {
	generate_with(release, None, name)
}

/// Runs the real generator on a modified release file and, if given, a
/// modified copy of the inventory.
fn generate_with(release: &str, inventory_json: Option<&str>, name: &str) -> (serde_json::Value, String) {
	let dir = root().join("target/0073").join(name);
	let _ = std::fs::remove_dir_all(&dir);
	std::fs::create_dir_all(&dir).unwrap();
	std::fs::write(dir.join("release.toml"), release).unwrap();
	let inv = match inventory_json {
		Some(text) => { let p = dir.join("inventory.json"); std::fs::write(&p, text).unwrap(); p }
		None => inventory(),
	};
	let status = Command::new("cargo")
		.args(["run", "-q", "--locked", "--manifest-path"])
		.arg(root().join("tools/polars-gen/Cargo.toml"))
		.arg("--")
		.arg(inv)
		.arg(&dir)
		.args(["--buckets", BUCKETS, "--release"])
		.arg(dir.join("release.toml"))
		.env("CARGO_TARGET_DIR", root().join("target/0073"))
		.status()
		.expect("run polars-gen");
	assert!(status.success());
	let surface = serde_json::from_str(&std::fs::read_to_string(dir.join("surface.json")).unwrap()).unwrap();
	(surface, std::fs::read_to_string(dir.join("src/generated/functions.rs")).unwrap())
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
