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
	let installed = row.get("dispatch").is_some();
	if installed {
		let reference = policy["installed_setup"].get("stack_query");
		let observed = row["dispatch"].get("stack_query");
		ensure!(
			reference.is_some() == observed.is_some(),
			"incompatible outer stack-query window"
		);
		if let (Some(reference), Some(observed)) = (reference, observed) {
			let before = observed["native_before_stack"]
				.as_u64()
				.context("missing native stack map")?;
			ensure!(
				before > 0 && reference[mode]["native_before_stack"].as_u64() == Some(before),
				"native mappings before stack differ"
			);
			net(&row["dispatch"], Some(observed), "T")?;
			net(&row["dispatch"], Some(observed), "W")?;
			ensure!(
				integer(&observed["W"])? <= integer(&observed["T"])?
					&& integer(&observed["T"])? > 0,
				"invalid excluded stack work"
			);
		}
		ensure!(
			row["dispatch"]["launch_sha256"].is_string()
				&& row["dispatch"]["launch_sha256"]
					== policy["installed_setup"]["outer_launch_sha256"][mode],
			"incomparable installed outer launch"
		);
	}
	let table = if installed {
		"installed_setup"
	} else {
		"native_setup"
	};
	let required = policy[table]["launch_sha256"][mode]
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

// Frozen and current outer totals both retain the raw cost. Deduct only
// the independently observed stack query when scoring the installed window.
fn net(raw: &Value, query: Option<&Value>, metric: &str) -> Result<i128> {
	let before = integer(&raw[metric])?;
	let excluded = query.map(|q| integer(&q[metric])).transpose()?.unwrap_or(0);
	ensure!(before >= excluded, "stack query exceeds outer window");
	Ok(before - excluded)
}

fn outer_reference(installed: &Value, mode: &str, metric: &str) -> Result<i128> {
	let reference = installed
		.get("outer_reference")
		.unwrap_or(&installed["outer"]);
	net(
		&reference[mode],
		installed.get("stack_query").map(|q| &q[mode]),
		metric,
	)
}

fn measure(row: &Value, phase: &str, metric: &str) -> Result<i128> {
	let a = &row["phases"][phase]["accounting"];
	let inner = if metric == "T" {
		integer(&a["total_Ir"])?
	} else {
		integer(&a["rnx_Ir"])? + integer(&a["other_Ir"])?
	};
	let outer = if phase == "setup"
		&& let Some(dispatch) = row.get("dispatch")
	{
		net(dispatch, dispatch.get("stack_query"), metric)?
	} else {
		0
	};
	Ok(inner + outer)
}

/// Keep the native-off whole-window gate and score the separate outer process.
pub(super) fn score_off_dispatch(
	whole: &mut Value,
	base: [&Value; 2],
	candidate: [&Value; 2],
	policy: &Value,
	limit: i128,
) -> Result<()> {
	let metric = whole["metric"].as_str().context("metric")?;
	let installed = &policy["installed_setup"];

	let mut checks = Vec::new();
	let mut passed = whole["pass_all"] == true;
	for (baseline, current) in base.into_iter().zip(candidate) {
		let before = if limit == 10 || baseline.get("dispatch").is_none() {
			outer_reference(installed, "off", metric)?
		} else {
			net(
				&baseline["dispatch"],
				baseline["dispatch"].get("stack_query"),
				metric,
			)?
		};
		let after = net(
			&current["dispatch"],
			current["dispatch"].get("stack_query"),
			metric,
		)?;
		let allowance = before * limit / 1000;
		let pass = after - before <= allowance;
		checks.push(json!({"before":before,"after":after,"allowance":allowance,"pass":pass}));
		passed &= pass;
	}
	whole["dispatch_levels"] = json!(checks);
	whole["pass_all"] = json!(passed);
	Ok(())
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
		let installed = inputs[1][0].get("dispatch").is_some();
		let migration = installed && inputs[0][0].get("dispatch").is_none();
		let reference = if installed {
			&policy["installed_setup"]["kernels"][kind][metric]
		} else {
			&budget["reference"]
		};
		let [b1, b5] = if phase == "setup" && (cumulative || migration) {
			// The 0324 per-kernel total includes its raw outer window.
			// Preserve that kernel's inner cost when replacing only the outer component.
			let outer_change = if installed {
				let setup = &policy["installed_setup"];
				outer_reference(setup, "on", metric)? - integer(&setup["outer"]["on"][metric])?
			} else {
				0
			};
			[
				integer(&reference[0])? + outer_change,
				integer(&reference[1])? + outer_change,
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
	fn off_dispatch_preserves_whole_verdict_and_uses_frozen_cumulative_reference() {
		let policy = json!({"installed_setup":{"outer":{"off":{"W":1000}}}});
		let stock = json!({});
		let prior = json!({"dispatch":{"W":1005}});
		for (baseline, limit, whole, after, expected) in [
			(&stock, 5, true, 1005, true),
			(&stock, 5, true, 1006, false),
			(&prior, 5, true, 1010, true),
			(&prior, 10, true, 1011, false),
			(&prior, 5, false, 1000, false),
		] {
			let changed = json!({"dispatch":{"W":after}});
			let mut row = json!({"metric":"W","pass_all":whole});
			score_off_dispatch(&mut row, [baseline; 2], [&prior, &changed], &policy, limit)
				.unwrap();
			assert_eq!(row["pass_all"], expected);
		}
	}

	#[test]
	fn installed_launch_is_charged_to_setup_on_migration_and_later_changes() {
		let mut policy: Value =
			serde_json::from_str(include_str!("../../column/fixed_route_policy.json")).unwrap();
		policy["installed_setup"]
			.as_object_mut()
			.unwrap()
			.remove("stack_query");
		policy["installed_setup"]
			.as_object_mut()
			.unwrap()
			.remove("outer_reference");
		let reference = policy["installed_setup"]["kernels"]["shared_rows"]["W"][0]
			.as_i64()
			.unwrap();
		let limit = policy["native_setup"]["kernels"]["shared_rows"]["W"]["per_record"]["level"]
			.as_i64()
			.unwrap();
		let pair = |outer: i64| {
			[100_000, 500_000].map(|execution| {
				let mut v = row(reference - 100, execution);
				v["dispatch"] = json!({"W":outer,"T":outer});
				v
			})
		};
		let old = [row(1, 100_000), row(1, 500_000)];
		let base = pair(100);
		for (baseline, permille) in [(&old, 5), (&base, 5), (&old, 10)] {
			for (extra, pass) in [(limit, true), (limit + 1, false)] {
				let after = pair(100 + extra);
				let mut verdict =
					json!({"kind":"shared_rows","metric":"W","level_pass":true,"growth_pass":true});
				score_row(
					&mut verdict,
					[[&baseline[0], &baseline[1]], [&after[0], &after[1]]],
					&policy,
					permille,
				)
				.unwrap();
				assert_eq!(verdict["pass_all"], pass);
				assert_eq!(verdict["phases"]["execution"][0]["delta"], 0);
			}
		}
	}

	#[test]
	fn query_window_preserves_frozen_and_per_record_charges() {
		let policy: Value =
			serde_json::from_str(include_str!("../../column/fixed_route_policy.json")).unwrap();
		let installed = &policy["installed_setup"];
		let reference = installed["kernels"]["shared_rows"]["T"][0]
			.as_i64()
			.unwrap();
		let mk = |extra: i64, observer: i64| {
			let mut r = row(
				reference - installed["outer"]["on"]["T"].as_i64().unwrap() + extra,
				100_000,
			);
			let mut d = installed["outer_reference"]["on"].clone();
			d["T"] = json!(d["T"].as_i64().unwrap() + observer);
			d["stack_query"] = installed["stack_query"]["on"].clone();
			d["stack_query"]["T"] = json!(d["stack_query"]["T"].as_i64().unwrap() + observer);
			d["launch_sha256"] = installed["outer_launch_sha256"]["on"].clone();
			r["dispatch"] = d;
			r["policy"] = json!("on");
			r["n"] = json!(1);
			r["kind"] = json!("shared_rows");
			r["launch_sha256"] = installed["launch_sha256"]["on"].clone();
			r
		};
		let baseline = mk(0, 0);
		let mut invalid = mk(0, 2010);
		validate_setup_context(&invalid, &policy).unwrap();
		invalid["dispatch"]["stack_query"]["native_before_stack"] = json!(33);
		assert!(validate_setup_context(&invalid, &policy).is_err());
		for limit in [5, 10] {
			for (extra, pass) in [(11, true), (12, false)] {
				let after = mk(extra, 2010);
				let mut b5 = baseline.clone();
				let mut a5 = after.clone();
				b5["phases"]["execution"] = row(0, 500_000)["phases"]["execution"].clone();
				a5["phases"]["execution"] = b5["phases"]["execution"].clone();
				let mut verdict =
					json!({"kind":"shared_rows","metric":"T","level_pass":true,"growth_pass":true});
				score_row(
					&mut verdict,
					[[&baseline, &b5], [&after, &a5]],
					&policy,
					limit,
				)
				.unwrap();
				assert_eq!(verdict["pass_all"], pass);
				assert_eq!(verdict["phases"]["setup"][0]["delta"], extra);
			}
		}
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
