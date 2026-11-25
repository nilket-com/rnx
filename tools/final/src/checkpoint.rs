//! Proof of the baseline that preceded a complete run. The bootstrap and
//! terminal receipt bind the same admission. Times are seconds since boot.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Admission {
	pub baseline: String,
	pub candidate: String,
	pub boot_id: String,
	pub preparation: String,
	pub recipe_sha256: String,
	pub toolchains_sha256: String,
	pub prepared_state_sha256: String,
	pub developer_state_sha256: String,
	pub requested: f64,
	pub acquired: f64,
	pub admitted: f64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Timing {
	pub admission: Admission,
	pub finished: f64,
	pub lease_wait_seconds: f64,
	pub preparation_seconds: f64,
	pub timed_seconds: f64,
	pub total_seconds: f64,
}

impl Admission {
	pub fn check(&self, candidate: &str, started: f64) -> Result<()> {
		let hex = |s: &str, n| {
			s.len() == n
				&& s.bytes()
					.all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
		};
		ensure!(
			self.candidate == candidate && hex(&self.candidate, 40) && hex(&self.baseline, 40),
			"admission revisions differ"
		);
		ensure!(
			hex(&self.recipe_sha256, 64)
				&& hex(&self.toolchains_sha256, 64)
				&& hex(&self.prepared_state_sha256, 64)
				&& hex(&self.developer_state_sha256, 64),
			"incomplete checkpoint identity"
		);
		ensure!(
			matches!(self.preparation.as_str(), "cold" | "restored" | "promoted"),
			"unknown checkpoint preparation"
		);
		ensure!(
			self.requested >= 0.0
				&& self.requested <= self.acquired
				&& self.acquired <= self.admitted
				&& self.admitted == started,
			"invalid admission clock boundaries"
		);
		let fields: Vec<_> = self.boot_id.split('-').collect();
		ensure!(
			fields.len() == 5 && fields.iter().zip([8, 4, 4, 4, 12]).all(|(s, n)| hex(s, n)),
			"invalid checkpoint boot identity"
		);
		Ok(())
	}

	pub fn timing(&self, seconds: f64) -> Timing {
		let finished = self.admitted + seconds;
		Timing {
			admission: self.clone(),
			finished,
			lease_wait_seconds: self.acquired - self.requested,
			preparation_seconds: self.admitted - self.acquired,
			timed_seconds: seconds,
			total_seconds: finished - self.requested,
		}
	}
}

pub fn revalidate(
	out: &Path,
	manifest: &Value,
	timing: &Timing,
	candidate: &str,
	seconds: f64,
) -> Result<()> {
	let boot = &manifest["tools"]["controller"]["bootstrap"];
	let started = boot["started"]
		.as_f64()
		.context("missing bootstrap admission time")?;
	let original: Admission = serde_json::from_value(boot["checkpoint"].clone())?;
	original.check(candidate, started)?;
	let proof = out.join("preparation");
	let stored: Admission =
		serde_json::from_reader(std::fs::File::open(proof.join("admission.json"))?)?;
	ensure!(
		crate::receipt::digest(&proof.join("classes.json"))? == original.prepared_state_sha256,
		"published checkpoint class identities differ"
	);
	let classes: Value = serde_json::from_reader(std::fs::File::open(proof.join("classes.json"))?)?;
	let anchor: Value =
		serde_json::from_reader(std::fs::File::open(proof.join("anchor-result.json"))?)?;
	ensure!(
		classes["baseline"] == original.baseline
			&& classes["lane_units"] == crate::receipt::digest(&proof.join("units.jsonl"))?
			&& classes["dependency_home"]
				== crate::receipt::digest(&proof.join("locked-sources.txt"))?
			&& anchor["status"] == "PASS"
			&& classes["anchor"].as_str().is_some()
			&& classes["anchor"] == anchor["anchor_cache"]["key"],
		"checkpoint class evidence differs"
	);
	ensure!(
		original == timing.admission && original == stored,
		"terminal checkpoint differs from bootstrap"
	);
	use sha2::{Digest, Sha256};
	let mut hash = Sha256::new();
	for name in ["recipes.jsonl", "fetch.json"] {
		hash.update(format!(
			"{}  {name}\n",
			crate::receipt::digest(&proof.join(name))?
		));
	}
	ensure!(
		crate::pyjson::hex(&hash.finalize()) == original.recipe_sha256,
		"published checkpoint preparation recipes differ"
	);
	let expected = original.timing(seconds);
	// The bootstrap, typed terminal and retained JSON are serialized
	// separately. Only arithmetic on decoded seconds can differ by an ULP.
	let same_seconds = |a: f64, b: f64| (a - b).abs() <= 0.000_001;
	ensure!(
		seconds >= 0.0
			&& timing.finished >= original.admitted
			&& same_seconds(timing.finished, expected.finished)
			&& same_seconds(timing.timed_seconds, seconds)
			&& same_seconds(timing.total_seconds, expected.total_seconds)
			&& same_seconds(timing.lease_wait_seconds, expected.lease_wait_seconds)
			&& same_seconds(timing.preparation_seconds, expected.preparation_seconds),
		"checkpoint durations differ from monotonic boundaries"
	);
	Ok(())
}
