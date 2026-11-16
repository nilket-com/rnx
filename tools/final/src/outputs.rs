//! A declared stage output: the Polars suite's oracle report, written to a
//! path the final owns and checked against the tracked one, so no stage
//! writes tracked source.
use crate::child::raise;
use anyhow::Result;
use serde_json::{Map, Value, json};
use sha2::{Digest as _, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const ORACLE: &str = "adapters/polars/oracle-results.json";
const APPROVED: [&str; 4] = ["match", "row_order_differs", "both_error", "both_panic"];

fn check(condition: bool, message: &str) -> Result<()> {
	if condition {
		Ok(())
	} else {
		Err(raise("AssertionError", message))
	}
}

pub fn validate_report(original: &[u8], actual: &[u8]) -> Result<()> {
	let (before, after): (Value, Value) = (
		serde_json::from_slice(original)?,
		serde_json::from_slice(actual)?,
	);
	let results = after["results"].as_array().cloned().unwrap_or_default();
	check(
		before["cases"] == after["cases"]
			&& after["cases"] == json!(results.len())
			&& after["verified"] == json!(results.len()),
		"incomplete oracle report",
	)?;
	let index = |rows: &Value| -> BTreeMap<String, Value> {
		rows.as_array()
			.into_iter()
			.flatten()
			.map(|r| (r["id"].to_string(), r.clone()))
			.collect()
	};
	let (old, new) = (index(&before["results"]), index(&after["results"]));
	check(
		new.len() == results.len() && old.keys().eq(new.keys()),
		"oracle cases changed",
	)?;
	let mut tally: Map<String, Value> = Map::new();
	for (key, row) in &new {
		check(
			["path", "policy", "unordered"]
				.iter()
				.all(|f| row[*f] == old[key][*f]),
			"oracle policy changed",
		)?;
		let status = row["status"].as_str().unwrap_or_default();
		check(APPROVED.contains(&status), "unapproved oracle outcome")?;
		check(
			status != "row_order_differs" || crate::failure::truthy(&row["unordered"]),
			"ordered oracle mismatch",
		)?;
		let count = tally.get(status).and_then(Value::as_u64).unwrap_or(0) + 1;
		tally.insert(status.to_owned(), json!(count));
	}
	check(
		Value::Object(tally) == after["tally"],
		"oracle tally mismatch",
	)
}

/// Where a final's Polars suite writes its report (RNX_ORACLE_RESULTS).
pub fn owned_report(out: &Path) -> PathBuf {
	out.join("polars-oracle-results.json")
}

fn git(root: &Path, arguments: &[&str]) -> Result<Vec<u8>> {
	let output = crate::child::git_command()
		.args(arguments)
		.current_dir(root)
		.output()?;
	anyhow::ensure!(output.status.success(), "git {arguments:?} failed");
	Ok(output.stdout)
}

/// Called after the stage's test, before the exact-source check; never
/// overrides its exit.
pub fn finish_stage(root: &Path, stage: &str, out: &Path) -> Result<Option<Value>> {
	if stage != "polars-suite" {
		return Ok(None);
	}
	let original = git(root, &["show", &format!("HEAD:{ORACLE}")])?;
	let archive = owned_report(out);
	let actual;
	if std::fs::symlink_metadata(&archive).is_ok() {
		// The report went to its owned path, so no tracked file may differ.
		let meta = std::fs::symlink_metadata(&archive)?;
		check(meta.is_file(), "oracle report is not a regular file")?;
		check(
			String::from_utf8_lossy(&git(root, &["status", "--porcelain"])?)
				.trim()
				.is_empty(),
			"unexpected source mutation",
		)?;
		actual = std::fs::read(&archive)?;
		validate_report(&original, &actual)?;
	} else {
		let status = git(root, &["status", "--porcelain=v1", "-z"])?;
		let allowed = format!(" M {ORACLE}");
		check(
			status
				.split(|b| *b == 0)
				.all(|row| row.is_empty() || row == allowed.as_bytes()),
			"unexpected source mutation",
		)?;
		let path = root.join(ORACLE);
		check(
			std::fs::symlink_metadata(&path).is_ok_and(|m| m.is_file()),
			"oracle report is not a regular file",
		)?;
		check(
			git(root, &["diff", "--summary", "--", ORACLE])?.is_empty(),
			"oracle mode changed",
		)?;
		actual = std::fs::read(&path)?;
		std::fs::OpenOptions::new()
			.write(true)
			.create_new(true)
			.open(&archive)
			.and_then(|mut f| std::io::Write::write_all(&mut f, &actual))?;
		validate_report(&original, &actual)?;
		std::fs::write(&path, &original)?;
	}
	check(
		String::from_utf8_lossy(&git(root, &["status", "--porcelain"])?)
			.trim()
			.is_empty(),
		"source changed",
	)?;
	let verified: Value = serde_json::from_slice::<Value>(&actual)?["verified"].clone();
	Ok(Some(json!({
		"output": ORACLE, "archive": archive.to_string_lossy(),
		"original_sha256": crate::pyjson::hex(&Sha256::digest(&original)),
		"actual_sha256": crate::pyjson::hex(&Sha256::digest(&actual)), "verified": verified,
	})))
}
