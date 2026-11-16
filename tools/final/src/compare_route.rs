//! The fixed-route screen's instruction counts. A screen's verdicts are
//! recomputed from its own counts; counts are compared exactly between two
//! screens whose finals' directories have paths of one length (callgrind
//! counts the startup over the process's arguments and environment, which
//! carry that path), except for the one case named below.
use anyhow::{Context as _, Result};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

/// The one case whose JIT run varies from run to run under either
/// controller: its `rnx_Ir`, and so its `total_Ir`, by at most this many
/// instructions. Measured on candidate 28e5d807's engine: 5 in the anchor
/// role (a Python and a Rust final at one path length), 11 in the
/// candidate role (two Python finals). Every other count is exact.
pub const VARIABLE_CASE: (&str, &str, u64) = ("numeric_calls", "on", 1);
pub const VARIATION_BOUND: u64 = 16;

/// The band limits in per mille, as `drift_band.validate_policy` fixes them.
const PER_RECORD_PER_MILLE: i128 = 5;
const CUMULATIVE_PER_MILLE: i128 = 10;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Accounting {
	rnx: u64,
	other: u64,
	dynamic: u64,
	total: u64,
}

type Case = (String, String, u64);

/// One screen's raw counts by role and case.
pub struct Screen {
	roles: BTreeMap<String, BTreeMap<Case, Accounting>>,
}

fn records(value: &Value, role: &str) -> Result<BTreeMap<Case, Accounting>> {
	let mut out = BTreeMap::new();
	for row in value["records"]
		.as_array()
		.with_context(|| format!("{role}: no records"))?
	{
		let count = |k: &str| {
			row["accounting"][k]
				.as_u64()
				.with_context(|| format!("{role}: {k} is not a count"))
		};
		let accounting = Accounting {
			rnx: count("rnx_Ir")?,
			other: count("other_Ir")?,
			dynamic: count("dynamic_Ir")?,
			total: count("total_Ir")?,
		};
		anyhow::ensure!(
			accounting.total == accounting.rnx + accounting.other + accounting.dynamic,
			"{role}: partitions do not reconcile"
		);
		let case = (
			row["kind"].as_str().context("kind")?.to_owned(),
			row["policy"].as_str().context("policy")?.to_owned(),
			row["n"].as_u64().context("n")?,
		);
		anyhow::ensure!(
			out.insert(case.clone(), accounting).is_none(),
			"{role}: duplicate case {case:?}"
		);
	}
	Ok(out)
}

impl Screen {
	/// Read a screen's receipt and its anchor evidence, and check that every
	/// derived value in the receipt follows from those counts: each band row,
	/// its verdicts, each dynamic row, and the status.
	pub fn read(receipt: &Value) -> Result<Self> {
		let anchor_path = receipt["anchor"].as_str().context("no anchor evidence")?;
		let anchor: Value = serde_json::from_slice(
			&std::fs::read(Path::new(anchor_path))
				.with_context(|| format!("anchor evidence {anchor_path}"))?,
		)?;
		let mut roles = BTreeMap::new();
		roles.insert("anchor".to_owned(), records(&anchor, "anchor")?);
		roles.insert(
			"baseline".to_owned(),
			records(&receipt["baseline"], "baseline")?,
		);
		roles.insert(
			"candidate".to_owned(),
			records(&receipt["candidate"], "candidate")?,
		);
		let screen = Self { roles };
		let mut all_pass = true;
		for (section, left, limit) in [
			("per_record", "baseline", PER_RECORD_PER_MILLE),
			("cumulative", "anchor", CUMULATIVE_PER_MILLE),
		] {
			let rows = receipt[section]
				.as_array()
				.with_context(|| format!("no {section}"))?;
			let kinds: std::collections::BTreeSet<(String, String)> = screen.roles["candidate"]
				.keys()
				.map(|(k, m, _)| (k.clone(), m.clone()))
				.collect();
			anyhow::ensure!(
				rows.len() == kinds.len() * 2,
				"{section}: {} rows for {} cases",
				rows.len(),
				kinds.len()
			);
			for row in rows {
				let expected = screen
					.band(row, left, limit)
					.with_context(|| format!("{section} {row}"))?;
				for field in [
					"B1",
					"B5",
					"C1",
					"C5",
					"level_pass",
					"growth_pass",
					"pass_all",
				] {
					anyhow::ensure!(
						row[field] == expected[field],
						"{section} {}/{}/{}: {field} is {} but its counts give {}",
						row["kind"],
						row["mode"],
						row["metric"],
						row[field],
						expected[field]
					);
				}
				all_pass &= row["pass_all"] == true;
			}
		}
		let dynamic = receipt["dynamic"].as_array().context("no dynamic rows")?;
		anyhow::ensure!(
			dynamic.len() == screen.roles.values().map(BTreeMap::len).sum::<usize>(),
			"dynamic rows do not cover the cases"
		);
		for row in dynamic {
			let role = row["role"].as_str().context("role")?;
			let case = (
				row["kind"].as_str().context("kind")?.to_owned(),
				row["mode"].as_str().context("mode")?.to_owned(),
				row["n"].as_u64().context("n")?,
			);
			let accounting = screen
				.roles
				.get(role)
				.and_then(|r| r.get(&case))
				.with_context(|| format!("dynamic row {role} {case:?} has no record"))?;
			anyhow::ensure!(
				row["Ir"].as_u64() == Some(accounting.dynamic),
				"dynamic {role} {case:?}: {} but the record has {}",
				row["Ir"],
				accounting.dynamic
			);
		}
		anyhow::ensure!(
			(receipt["status"] == "PASS") == all_pass,
			"status {} does not follow from its verdicts",
			receipt["status"]
		);
		Ok(screen)
	}

	/// One band row as `drift_band.compare` computes it.
	fn band(&self, row: &Value, left: &str, limit: i128) -> Result<serde_json::Map<String, Value>> {
		let (kind, mode, metric) = (
			row["kind"].as_str().context("kind")?,
			row["mode"].as_str().context("mode")?,
			row["metric"].as_str().context("metric")?,
		);
		let count = |role: &str, n: u64| -> Result<i128> {
			let a = self.roles[role]
				.get(&(kind.to_owned(), mode.to_owned(), n))
				.with_context(|| format!("no {role} {kind}/{mode}/{n}"))?;
			Ok(match metric {
				"T" => a.total,
				"W" => a.rnx + a.other,
				other => anyhow::bail!("unknown metric {other}"),
			} as i128)
		};
		let (b1, b5, c1, c5) = (
			count(left, 1)?,
			count(left, 5)?,
			count("candidate", 1)?,
			count("candidate", 5)?,
		);
		anyhow::ensure!(b5 > 0 && b5 > b1, "invalid baseline denominator");
		let level = 1000 * (c5 - b5) <= limit * b5;
		let growth = 1000 * ((c5 - c1) - (b5 - b1)) <= limit * (b5 - b1);
		let mut out = serde_json::Map::new();
		for (k, v) in [("B1", b1), ("B5", b5), ("C1", c1), ("C5", c5)] {
			out.insert(k.into(), Value::from(v as u64));
		}
		out.insert("level_pass".into(), level.into());
		out.insert("growth_pass".into(), growth.into());
		out.insert("pass_all".into(), (level && growth).into());
		Ok(out)
	}

	/// Every raw count that differs between two screens, beyond the named
	/// case's bound. The derived rows were checked against these counts.
	pub fn counts_differ(&self, other: &Self) -> Vec<String> {
		let mut out = Vec::new();
		for (role, cases) in &self.roles {
			let Some(theirs) = other.roles.get(role) else {
				out.push(format!("{role}: in one screen only"));
				continue;
			};
			let keys: std::collections::BTreeSet<&Case> =
				cases.keys().chain(theirs.keys()).collect();
			for case in keys {
				let (Some(a), Some(b)) = (cases.get(case), theirs.get(case)) else {
					out.push(format!("{role} {case:?}: in one screen only"));
					continue;
				};
				let variable = (case.0.as_str(), case.1.as_str(), case.2) == VARIABLE_CASE;
				let near = |x: u64, y: u64| {
					if variable {
						x.abs_diff(y) <= VARIATION_BOUND
					} else {
						x == y
					}
				};
				if a.other != b.other
					|| a.dynamic != b.dynamic
					|| !near(a.rnx, b.rnx)
					|| !near(a.total, b.total)
				{
					out.push(format!("{role} {case:?}: {a:?} != {b:?}"));
				}
			}
		}
		out
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

	fn accounting(rnx: u64) -> Value {
		json!({"rnx_Ir": rnx, "other_Ir": 10, "dynamic_Ir": 100, "total_Ir": rnx + 110})
	}

	fn screen(kind: &str, mode: &str, counts: [u64; 2]) -> Screen {
		let role = |c: [u64; 2]| {
			json!({"records": [
				{"kind": kind, "policy": mode, "n": 1, "accounting": accounting(c[0])},
				{"kind": kind, "policy": mode, "n": 5, "accounting": accounting(c[1])},
			]})
		};
		let mut roles = BTreeMap::new();
		for name in ["anchor", "baseline", "candidate"] {
			roles.insert(name.to_owned(), records(&role(counts), name).unwrap());
		}
		Screen { roles }
	}

	#[test]
	fn a_deterministic_count_must_match_exactly() {
		let a = screen("fib", "off", [1000, 5000]);
		assert!(
			a.counts_differ(&screen("fib", "off", [1000, 5000]))
				.is_empty()
		);
		assert!(
			!a.counts_differ(&screen("fib", "off", [1001, 5000]))
				.is_empty()
		);
	}

	#[test]
	fn only_the_named_case_may_vary_and_only_within_its_bound() {
		let a = screen("numeric_calls", "on", [1000, 5000]);
		assert!(
			a.counts_differ(&screen("numeric_calls", "on", [1011, 5000]))
				.is_empty()
		);
		assert!(
			!a.counts_differ(&screen(
				"numeric_calls",
				"on",
				[1000 + VARIATION_BOUND + 1, 5000]
			))
			.is_empty()
		);
		// n = 5 of the same case is exact.
		assert!(
			!a.counts_differ(&screen("numeric_calls", "on", [1000, 5001]))
				.is_empty()
		);
		// The same kind in the other mode is exact.
		let off = screen("numeric_calls", "off", [1000, 5000]);
		assert!(
			!off.counts_differ(&screen("numeric_calls", "off", [1005, 5000]))
				.is_empty()
		);
	}

	#[test]
	fn a_verdict_must_follow_from_its_counts() {
		let s = screen("fib", "off", [1000, 5000]);
		let row = json!({"kind": "fib", "mode": "off", "metric": "W"});
		let band = s.band(&row, "baseline", PER_RECORD_PER_MILLE).unwrap();
		assert_eq!(
			(band["B5"].clone(), band["pass_all"].clone()),
			(json!(5010), json!(true))
		);
		// A candidate 1% heavier at n = 5 fails the 5 per mille level.
		let mut heavier = screen("fib", "off", [1000, 5000]);
		heavier
			.roles
			.get_mut("candidate")
			.unwrap()
			.get_mut(&("fib".into(), "off".into(), 5))
			.unwrap()
			.rnx = 5060;
		let band = heavier
			.band(&row, "baseline", PER_RECORD_PER_MILLE)
			.unwrap();
		assert_eq!(
			(band["level_pass"].clone(), band["pass_all"].clone()),
			(json!(false), json!(false))
		);
	}
}
