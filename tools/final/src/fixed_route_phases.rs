//! Direct phase evidence and the independently frozen setup allowances.
use super::{at, count};
use anyhow::{Context as _, Result, ensure};
use serde_json::{Value, json};

pub(super) const TABLE_DIGEST: &str =
	"63b2eebead7cae916d19f17f741c3c2244e1b517e5ec00e63df7c51aa2d9aabc";

fn integer(value: &Value) -> Result<i128> {
	count(value).context("invalid phase instruction count")
}

fn partitions(value: &Value) -> Result<[i128; 4]> {
	let mut counts = [0; 4];
	for (n, key) in counts
		.iter_mut()
		.zip(["rnx_Ir", "other_Ir", "dynamic_Ir", "total_Ir"])
	{
		*n = integer(at(value, key)?)?;
	}
	ensure!(
		counts[0] + counts[1] + counts[2] == counts[3],
		"phase partitions differ from total"
	);
	Ok(counts)
}

pub(crate) fn validate_setup_context(row: &Value, policy: &Value) -> Result<()> {
	let kind = row["kind"].as_str().context("phase kernel")?;
	let size = row["n"].as_u64().context("phase size")?.to_string();
	let mode = row["policy"].as_str().context("capture mode")?;
	let required = policy["native_setup"]["launch_sha256"][mode]
		.as_str()
		.context("missing frozen setup launch")?;
	let actual = row["launch_sha256"].as_str().unwrap_or("missing");
	ensure!(
		actual == required,
		"incomparable native setup capture for {kind} size {size}: argv paths, working directory or ordered environment differ from the setup reference (expected {required}, observed {actual})"
	);
	Ok(())
}

pub(super) fn validate_row(row: &Value, policy: &Value) -> Result<()> {
	validate_setup_context(row, policy)?;
	if row["policy"] == "off" {
		ensure!(
			row.get("phases").is_none() && row.get("boundary").is_none(),
			"native-off phase evidence"
		);
		return Ok(());
	}
	let kind = row["kind"].as_str().context("phase kernel")?;
	let n = match row["n"].as_u64() {
		Some(1) => 0,
		Some(5) => 1,
		_ => anyhow::bail!("phase size"),
	};
	let frozen = &policy["native_setup"]["kernels"][kind]["fixtures"][n];
	ensure!(
		frozen == &json!([row["fixture_sha256"], row["stdout_sha256"]]),
		"phase fixture/output differs from calibration"
	);
	let parts = at(row, "phases")?;
	ensure!(
		parts.as_object().is_some_and(|p| p.len() == 2),
		"expected two phases"
	);
	let a = partitions(&parts["setup"]["accounting"])?;
	let b = partitions(&parts["execution"]["accounting"])?;
	let whole = partitions(&row["accounting"])?;
	ensure!(
		(0..4).all(|i| a[i] + b[i] == whole[i]),
		"phase counts do not reconstruct whole"
	);
	let boundary = at(row, "boundary")?;
	let symbol = boundary["split"].as_str().context("missing phase split")?;
	ensure!(
		boundary["status"] == "PASS"
			&& boundary["armed"] == true
			&& boundary["entries"] == 1
			&& boundary["returns"] == 1
			&& boundary["exit"] == 0
			&& boundary["errors"] == json!([])
			&& boundary["thread"]
				.as_array()
				.is_some_and(|t| t.len() == 3 && t.iter().all(Value::is_u64))
			&& symbol.starts_with("<rune::experimental_core::execute::Executor>::invoke_args::<")
			&& symbol.ends_with(">::{closure#0}")
			&& parts["execution"]["accounting"]["rnx_functions"][symbol]
				.as_u64()
				.is_some_and(|n| n > 0),
		"invalid invocation boundary evidence"
	);
	Ok(())
}

fn measure(row: &Value, phase: &str, metric: &str) -> Result<i128> {
	let a = &row["phases"][phase]["accounting"];
	if metric == "T" {
		integer(&a["total_Ir"])
	} else {
		Ok(integer(&a["rnx_Ir"])? + integer(&a["other_Ir"])?)
	}
}

pub(super) fn score_row(
	whole: &mut Value,
	inputs: [[&Value; 2]; 2],
	policy: &Value,
	limit: i128,
) -> Result<()> {
	let metric = whole["metric"].as_str().context("metric")?;
	let kind = whole["kind"].as_str().context("kernel")?;
	let budget = &policy["native_setup"]["kernels"][kind][metric];
	let cumulative = limit == 10;
	let scope = if cumulative {
		"cumulative"
	} else {
		"per_record"
	};
	let mut passed = whole["growth_pass"] == true && (!cumulative || whole["level_pass"] == true);
	let whole_level_scored = cumulative;
	let mut phases = json!({});
	for phase in ["setup", "execution"] {
		let pair = |i: usize| -> Result<[i128; 2]> {
			Ok([
				measure(inputs[i][0], phase, metric)?,
				measure(inputs[i][1], phase, metric)?,
			])
		};
		let [b1, b5] = if phase == "setup" && cumulative {
			[
				integer(&budget["reference"][0])?,
				integer(&budget["reference"][1])?,
			]
		} else {
			pair(0)?
		};
		let [c1, c5] = pair(1)?;
		let mut checks = Vec::new();
		for (axis, before, after) in [
			("size1", b1, c1),
			("size5", b5, c5),
			("growth", b5 - b1, c5 - c1),
		] {
			if phase == "execution" && axis == "size1" {
				continue;
			}
			let allowance = if phase == "setup" {
				integer(&budget[scope][if axis == "growth" { "growth" } else { "level" }])?
			} else {
				ensure!(before > 0, "invalid execution denominator");
				before * limit / 1000
			};
			let delta = after - before;
			let pass = delta <= allowance;
			checks.push(json!({"axis":axis,"before":before,"after":after,"delta":delta,"allowance":allowance,"pass":pass}));
			passed &= pass;
		}
		phases[phase] = json!(checks);
	}
	whole["phases"] = phases;
	whole["whole_level_scored"] = json!(whole_level_scored);
	whole["pass_all"] = json!(passed);
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	fn row(setup: i64, execution: i64) -> Value {
		let account = |n| json!({"total_Ir":n,"rnx_Ir":n,"other_Ir":0,"dynamic_Ir":0});
		json!({"phases":{"setup":{"accounting":account(setup)},"execution":{"accounting":account(execution)}}})
	}

	#[test]
	fn independently_enforces_levels_growth_and_fixed_setup_reference() {
		let policy: Value =
			serde_json::from_str(include_str!("../../column/fixed_route_policy.json")).unwrap();
		let reference = policy["native_setup"]["kernels"]["shared_rows"]["W"]["reference"][0]
			.as_i64()
			.unwrap();
		let compare = |before: [(i64, i64); 2],
		               after: [(i64, i64); 2],
		               limit: i128,
		               whole_level: bool,
		               whole_growth: bool| {
			let b = before.map(|(s, e)| row(s, e));
			let c = after.map(|(s, e)| row(s, e));
			let mut result = json!({"kind":"shared_rows","metric":"W","level_pass":whole_level,"growth_pass":whole_growth});
			score_row(
				&mut result,
				[[&b[0], &b[1]], [&c[0], &c[1]]],
				&policy,
				limit,
			)?;
			Ok::<_, anyhow::Error>(result)
		};
		let baseline = [(reference, 100_000), (reference, 500_000)];
		let raised = |s, e| [(reference + s, 100_000 + e), (reference + s, 500_000 + e)];
		// Two individually permitted 20-Ir changes exceed the absolute 37-Ir allowance.
		assert_eq!(
			compare(raised(20, 0), raised(40, 0), 5, false, true).unwrap()["pass_all"],
			true
		);
		assert_eq!(
			compare(raised(20, 0), raised(40, 0), 10, true, true).unwrap()["pass_all"],
			false
		);
		for limit in [5, 10] {
			for (delta, passed) in [(37, true), (38, false)] {
				assert_eq!(
					compare(baseline, raised(delta, 0), limit, true, true).unwrap()["pass_all"],
					passed
				);
			}
			// Zero setup growth is exact even when both levels fit.
			assert_eq!(
				compare(
					baseline,
					[(reference, 100_000), (reference + 1, 500_000)],
					limit,
					true,
					true
				)
				.unwrap()["pass_all"],
				false
			);
			let max = 500 * limit as i64;
			assert_eq!(
				compare(baseline, raised(-max, max), limit, true, true).unwrap()["pass_all"],
				true
			);
			assert_eq!(
				compare(baseline, raised(-max - 1, max + 1), limit, true, true).unwrap()["pass_all"],
				false
			);
			assert_eq!(
				compare(baseline, baseline, limit, true, false).unwrap()["pass_all"],
				false
			);
			assert!(compare([(reference, 500_000); 2], baseline, limit, true, true).is_err());
			let growth = 400 * limit as i64;
			for (extra, passed) in [(0, true), (1, false)] {
				let after = [(reference, 100_000), (reference, 500_000 + growth + extra)];
				assert_eq!(
					compare(baseline, after, limit, true, true).unwrap()["pass_all"],
					passed
				);
			}
		}
		assert_eq!(
			compare(baseline, baseline, 10, false, true).unwrap()["pass_all"],
			false
		);
	}
}
