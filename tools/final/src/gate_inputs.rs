//! Only a published source revision can supply reusable inputs to a later gate.
use crate::{
	controller, fixed_route_images,
	pyjson::identity,
	receipt::{atomic_json, digest},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
	fs,
	path::{Path, PathBuf},
};

const FILES: [(&str, &str); 3] = [
	("binary", "runner"),
	("installed_binary", "installed"),
	("engine_binary", "engine"),
];

fn read(path: &Path) -> Result<Value> {
	Ok(serde_json::from_reader(fs::File::open(path)?)?)
}

fn file_sha<'a>(build: &'a Value, role: &str) -> Result<&'a str> {
	build["measured"][role]["sha256"]
		.as_str()
		.context("missing measured image digest")
}

pub fn save(dir: &Path, build: &Value, recipe: &Value, policy: &Value) -> Result<()> {
	fs::create_dir(dir)?;
	fixed_route_images::owner(build)?;
	let mut row = build.clone();
	for (field, role) in FILES {
		if let Some(path) = build[field].as_str() {
			let file = dir.join(role);
			fs::copy(path, &file)?;
			ensure!(
				digest(&file)? == file_sha(build, role)?,
				"changed {role} during retention"
			);
			row[field] = json!(role);
		}
	}
	atomic_json(
		&dir.join("BUILD.json"),
		&json!({
			"source": build["source_commit"],
			"recipe": identity(&crate::fixed_route::reuse_recipe(recipe)?)?,
			"policy": identity(&crate::fixed_route::measurement_policy(policy)?)?,
			"full_policy": identity(policy)?,
			"build": row
		}),
	)
}

fn publication(area: &Path, root: &Path, commit: &str) -> Result<(PathBuf, Value)> {
	let index = read(&area.join("gate-publications").join(commit))?;
	ensure!(
		index["publication_commit"] == commit,
		"different publication"
	);
	let out = PathBuf::from(index["out"].as_str().context("missing final output")?);
	let receipt = out.join("RESULT.json");
	ensure!(
		index["result_sha256"] == digest(&receipt)?,
		"publication result changed"
	);
	let result = read(&receipt)?;
	let manifest = out.join("GATEINPUTS.json");
	ensure!(
		result["status"] == "PASS"
			&& result["complete_final"] == true
			&& result["records"]["GATEINPUTS"] == digest(&manifest)?,
		"unaccepted gate inputs"
	);
	let tested = result["candidate"]
		.as_str()
		.context("missing tested candidate")?;
	if tested != commit {
		controller::docs_change(root, tested, commit)?;
	}
	Ok((out, result))
}

pub fn prior_anchor(area: &Path, root: &Path, commit: &str, key: &str) -> Result<Value> {
	let (out, _) = publication(area, root, commit)?;
	let manifest = read(&out.join("GATEINPUTS.json"))?;
	let path = Path::new(
		manifest["capture"]["path"]
			.as_str()
			.context("missing capture")?,
	);
	ensure!(
		manifest["capture"]["sha256"] == digest(&path.join("ANCHOR.json"))?,
		"anchor binding changed"
	);
	crate::fixed_route::Cache::verify(path, key)
}
/// The index is a hint. A missing, corrupt or stale entry is an ordinary miss.
pub fn load(
	area: &Path,
	root: &Path,
	commit: &str,
	role: &str,
	recipe: &Value,
	policy: &Value,
) -> Result<Value> {
	let (out, result) = publication(area, root, commit)?;
	let manifest = out.join("GATEINPUTS.json");
	let refs = read(&manifest)?;
	let dir = out.join("gate-inputs").join(role);
	let descriptor = dir.join("BUILD.json");
	ensure!(
		refs[role] == digest(&descriptor)?,
		"build descriptor changed"
	);
	let source = read(&descriptor)?;
	let built_at = source["source"].as_str().context("missing build source")?;
	if role == "candidate" {
		ensure!(
			result["candidate"] == built_at,
			"build source differs from tested candidate"
		);
	}
	ensure!(
		source["recipe"] == identity(&crate::fixed_route::reuse_recipe(recipe)?)?
			&& source["policy"] == identity(&crate::fixed_route::measurement_policy(policy)?)?,
		"gate recipe changed"
	);
	let mut row = source["build"].clone();
	ensure!(
		row["source_commit"] == built_at,
		"embedded build source differs"
	);

	for (field, name) in FILES {
		if let Some(relative) = row[field].as_str() {
			ensure!(relative == name, "wrong retained file");
			let file = dir.join(name);
			ensure!(
				file.canonicalize()? == file && digest(&file)? == file_sha(&row, name)?,
				"retained {name} differs"
			);
			row[field] = json!(file);
		}
	}
	fixed_route_images::owner(&row)?;
	row["build_source_commit"] = json!(built_at);
	if role == "candidate" {
		row["publication_commit"] = json!(commit);
	}
	Ok(row)
}

pub fn index(area: &Path, commit: &str, out: &Path) -> Result<()> {
	let directory = area.join("gate-publications");
	fs::create_dir_all(&directory)?;
	atomic_json(
		&directory.join(commit),
		&json!({"publication_commit":commit,"out":out,"result_sha256":digest(&out.join("RESULT.json"))?}),
	)
}

/// Persist gate artifacts only after the gate and the other selected stages passed.
pub fn retain(root: &Path, out: &Path) -> Result<String> {
	let stage = root.join("target/gate-inputs");
	let retained = out.join("gate-inputs");
	fs::create_dir(&retained)?;
	let mut bindings = serde_json::Map::new();
	for role in ["anchor", "candidate"] {
		let src = stage.join(role);
		let dest = retained.join(role);
		fs::create_dir(&dest)?;
		let row = read(&src.join("BUILD.json"))?;
		for (field, name) in FILES {
			if row["build"].get(field).is_some() {
				fs::copy(src.join(name), dest.join(name))?;
				ensure!(
					digest(&dest.join(name))? == file_sha(&row["build"], name)?,
					"gate input changed during retention"
				);
			}
		}
		fs::copy(src.join("BUILD.json"), dest.join("BUILD.json"))?;
		bindings.insert(role.into(), json!(digest(&dest.join("BUILD.json"))?));
	}
	let anchor = read(&stage.join("capture.json"))?;
	let key = anchor["capture_key"]
		.as_str()
		.context("missing anchor key")?;
	let binary = Path::new(anchor["binary"].as_str().context("missing anchor binary")?);
	let mut directory = binary
		.parent()
		.context("missing anchor directory")?
		.to_owned();
	if directory.starts_with(root) {
		let mut cache = crate::fixed_route::Cache::open(&retained.join("capture"))?;
		let stored = cache.publish(
			&anchor,
			binary,
			&json!({"capture_sha256":identity(&anchor)?}),
		)?;
		directory = Path::new(
			stored["binary"]
				.as_str()
				.context("missing retained anchor")?,
		)
		.parent()
		.context("missing retained directory")?
		.to_owned();
	}
	crate::fixed_route::Cache::verify(&directory, key)?;
	bindings.insert(
		"capture".into(),
		json!({"path":directory,"sha256":digest(&directory.join("ANCHOR.json"))?}),
	);
	let proof = out.join("GATEINPUTS.json");
	atomic_json(&proof, &bindings)?;
	digest(&proof)
}

/// Remove only our unpublished staging after a late failure.
pub fn discard(out: &Path) -> Result<()> {
	let dir = out.join("gate-inputs");
	if dir.try_exists()? {
		for role in ["anchor", "candidate"] {
			let path = dir.join(role);
			if !path.try_exists()? {
				continue;
			}
			for file in ["runner", "installed", "engine", "BUILD.json"] {
				match fs::remove_file(path.join(file)) {
					Ok(()) => {}
					Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
					Err(e) => return Err(e.into()),
				}
			}
			fs::remove_dir(path)?;
		}
		let capture = dir.join("capture");
		if capture.try_exists()? {
			fs::remove_dir_all(capture)?;
		}
		fs::remove_dir(dir)?;
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::receipt::invocation_id;

	#[test]
	fn reuse_requires_the_published_source_recipe_policy_and_each_image() {
		let area = std::env::temp_dir().join(format!("rnx-gate-inputs-{}", invocation_id()));
		fs::create_dir(&area).unwrap();
		struct Guard(PathBuf);
		impl Drop for Guard {
			fn drop(&mut self) {
				fs::remove_dir_all(&self.0).unwrap();
			}
		}
		let _guard = Guard(area.clone());
		let out = area.join("passed");
		let retained = out.join("gate-inputs/candidate");
		fs::create_dir_all(retained.parent().unwrap()).unwrap();
		let binary = area.join("binary");
		fs::write(&binary, b"tested runner").unwrap();
		let sha = digest(&binary).unwrap();
		let recipe = json!({"toolchain":"pinned","build":"release","policy":{"schema":2,"per_record_per_mille":5,"cumulative_per_mille":10}});
		let policy = json!({
			"schema": 2,
			"anchor": "a".repeat(40),
			"kernels": ["fib"],
			"modes": ["on", "off"],
			"sizes": [1, 5],
			"meter": {"version": 1, "hash_keys": [1, 2], "epoch_ns": 0, "tick_ns": 1},
			"native_setup": {"launch_sha256": "1".repeat(64)},
			"installed_setup": {"launch_sha256": "2".repeat(64), "outer_reference": {"host_inputs": "3".repeat(64)}},
			"relative_relocations": {"version": 1, "loader": "ld", "sha256": "4".repeat(64)},
			"source_transitions": [],
			"inventory": ["item_a"],
			"anchor_transition": {"old": "0".repeat(40), "new": "a".repeat(40)},
			"per_record_per_mille": 5,
			"cumulative_per_mille": 10
		});
		let revision = "tested";
		let row = json!({"source_commit":revision,"sha256":sha,"binary":binary,
			"measured":{"version":1,"target":"x86_64-unknown-linux-gnu","runner":{"path":"/run/rnx-route/rnx","sha256":sha}}});
		save(&retained, &row, &recipe, &policy).unwrap();
		let manifest = out.join("GATEINPUTS.json");
		atomic_json(
			&manifest,
			&json!({"candidate":digest(&retained.join("BUILD.json")).unwrap()}),
		)
		.unwrap();
		atomic_json(&out.join("RESULT.json"),&json!({"candidate":revision,"status":"PASS","complete_final":true,"records":{"GATEINPUTS":digest(&manifest).unwrap()}})).unwrap();
		let lookup = |commit: &str, recipe: &Value, policy: &Value| {
			load(&area, &area, commit, "candidate", recipe, policy)
		};
		assert!(lookup(revision, &recipe, &policy).is_err());
		index(&area, revision, &out).unwrap();
		assert_eq!(
			lookup(revision, &recipe, &policy).unwrap()["build_source_commit"],
			revision
		);
		assert!(
			lookup(
				revision,
				&json!({"toolchain":"changed","build":"release","policy":{"schema":2,"per_record_per_mille":5,"cumulative_per_mille":10}}),
				&policy
			)
			.is_err()
		);

		// Control 1 accepting witnesses: each declaration/scoring-only field
		// may change without invalidating reuse, while the planted-back full
		// policy identity rejects every one of them.
		let full_policy_before = identity(&policy).unwrap();
		let accepting: Vec<(&str, Value, Value)> = vec![
			("source_transitions", recipe.clone(), {
				let mut p = policy.clone();
				p["source_transitions"] =
					json!([{"selector": "invoke_args", "from": "1", "to": "2"}]);
				p
			}),
			("inventory", recipe.clone(), {
				let mut p = policy.clone();
				p["inventory"] = json!(["item_a", "item_b"]);
				p
			}),
			("anchor_transition", recipe.clone(), {
				let mut p = policy.clone();
				p["anchor_transition"]["plan"] = json!("b".repeat(40));
				p
			}),
			(
				"per_record_per_mille",
				{
					let mut r = recipe.clone();
					r["policy"]["per_record_per_mille"] = json!(6);
					r
				},
				{
					let mut p = policy.clone();
					p["per_record_per_mille"] = json!(6);
					p
				},
			),
			(
				"cumulative_per_mille",
				{
					let mut r = recipe.clone();
					r["policy"]["cumulative_per_mille"] = json!(11);
					r
				},
				{
					let mut p = policy.clone();
					p["cumulative_per_mille"] = json!(11);
					p
				},
			),
		];
		for (label, next_recipe, next_policy) in &accepting {
			assert_eq!(
				lookup(revision, next_recipe, next_policy)
					.unwrap_or_else(|e| panic!("accepting witness {label} failed: {e}"))["build_source_commit"],
				revision,
				"accepting witness {label}"
			);
			// Planted-back old identity (full policy digest) fails every accepting witness.
			assert_ne!(
				identity(next_policy).unwrap(),
				full_policy_before,
				"old full-policy identity must reject {label}"
			);
		}

		// Control 1 refusing witnesses: every field that affects built images
		// or capture counts must refuse reuse.
		let refusing: Vec<(&str, Value)> = vec![
			("schema", {
				let mut p = policy.clone();
				p["schema"] = json!(3);
				p
			}),
			("anchor", {
				let mut p = policy.clone();
				p["anchor"] = json!("b".repeat(40));
				p
			}),
			("kernels", {
				let mut p = policy.clone();
				p["kernels"] = json!(["fib", "permute"]);
				p
			}),
			("modes", {
				let mut p = policy.clone();
				p["modes"] = json!(["on"]);
				p
			}),
			("sizes", {
				let mut p = policy.clone();
				p["sizes"] = json!([1, 10]);
				p
			}),
			("meter", {
				let mut p = policy.clone();
				p["meter"]["tick_ns"] = json!(2);
				p
			}),
			("native_setup", {
				let mut p = policy.clone();
				p["native_setup"]["launch_sha256"] = json!("9".repeat(64));
				p
			}),
			("installed_setup", {
				let mut p = policy.clone();
				p["installed_setup"]["launch_sha256"] = json!("8".repeat(64));
				p
			}),
			("relative_relocations", {
				let mut p = policy.clone();
				p["relative_relocations"]["sha256"] = json!("7".repeat(64));
				p
			}),
		];
		for (label, mutated) in &refusing {
			assert!(
				lookup(revision, &recipe, mutated).is_err(),
				"refusing witness {label} must fail reuse"
			);
		}

		index(&area, "other", &out).unwrap();
		assert!(lookup("other", &recipe, &policy).is_err());
		fs::write(retained.join("runner"), b"changed runner").unwrap();
		assert!(lookup(revision, &recipe, &policy).is_err());
	}
}
