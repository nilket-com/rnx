//! Image identities are role-specific inputs. The common measurement recipe
//! can therefore still be compared across two different builds.
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::Value;
use std::path::Path;

const RUNNER: &str = "/run/rnx-route/rnx";
const ENGINE: &str = "/run/rnx-route/libengine_image.so";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Artifact {
	path: String,
	sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Images {
	version: u32,
	target: String,
	runner: Artifact,
	engine: Option<Artifact>,
	installed: Option<Artifact>,
}

pub(crate) fn validate(desc: &Value, runner_sha: &Value) -> Result<()> {
	let images: Images = serde_json::from_value(desc.clone())?;
	let valid = |a: &Artifact, path| {
		a.path == path && a.sha256.len() == 64 && a.sha256.bytes().all(|c| c.is_ascii_hexdigit())
	};
	ensure!(
		images.version == if images.installed.is_some() { 2 } else { 1 }
			&& images.target == "x86_64-unknown-linux-gnu"
			&& valid(&images.runner, RUNNER)
			&& Some(images.runner.sha256.as_str()) == runner_sha.as_str(),
		"invalid measured runner"
	);
	if let Some(outer) = images.installed {
		ensure!(
			images.engine.is_some()
				&& valid(&outer, "/run/rnx-route/installed")
				&& outer.sha256 != images.runner.sha256,
			"invalid installed CLI"
		);
	} else {
		ensure!(desc.get("installed").is_none(), "null installed artifact");
	}
	if let Some(engine) = images.engine {
		ensure!(
			valid(&engine, ENGINE) && engine.sha256 != images.runner.sha256,
			"invalid measured engine"
		);
	} else {
		ensure!(desc.get("engine").is_none(), "null engine artifact");
	}
	Ok(())
}

/// Select the file whose code the identity precheck must inspect.
pub(crate) fn owner(build: &Value) -> Result<&str> {
	if let Some(desc) = build.get("measured") {
		validate(desc, &build["sha256"])?;
		ensure!(
			build.get("installed_binary").is_some() == desc.get("installed").is_some(),
			"undeclared or missing installed binary"
		);
		if desc.get("engine").is_some() {
			let path = build["engine_binary"]
				.as_str()
				.context("missing engine file")?;
			ensure!(
				Path::new(path).is_absolute(),
				"engine file must be absolute"
			);
			return Ok(path);
		}
	}
	ensure!(
		build.get("engine_binary").is_none(),
		"undeclared engine file"
	);
	build["binary"].as_str().context("missing runner")
}

/// Bind the outer exec observation to the same artifact set and mode as the
/// inner observation. Paths may move together when evidence enters a cache.
pub(crate) fn dispatch_proof(row: &Value, desc: &Value, proof: &Value) -> Result<()> {
	let d = &row["dispatch"];
	ensure!(
		proof["status"] == "PASS"
			&& proof["mode"] == row["policy"]
			&& proof["artifacts"] == *desc
			&& proof["route"]
				== serde_json::json!({"route":"execution","category":null,"reason":null,"location":null})
			&& proof["launch"].is_object()
			&& proof["launch_sha256"] == crate::pyjson::identity(&proof["launch"])?,
		"installed dispatch proof differs"
	);
	if d.get("stack_query").is_some() || proof.get("stack_query").is_some() {
		for key in ["stack_query", "query_sha256", "maps_sha256"] {
			ensure!(
				d.get(key).is_some() && d[key] == proof[key],
				"installed stack-query proof changed: {key}"
			);
		}
	}
	for key in ["profile_sha256", "inner_sha256", "W", "T", "launch_sha256"] {
		ensure!(
			d.get(key).is_some() && d[key] == proof[key],
			"installed dispatch proof changed: {key}"
		);
	}
	Ok(())
}

pub(crate) fn validate_row(row: &Value, desc: &Value) -> Result<()> {
	ensure!(
		row.get("dispatch").is_some() == desc.get("installed").is_some(),
		"installed dispatch evidence mismatch"
	);
	if let Some(dispatch) = row.get("dispatch") {
		let w = dispatch["W"].as_u64().context("missing dispatch work")?;
		let t = dispatch["T"]
			.as_u64()
			.context("missing total dispatch instructions")?;
		ensure!(w > 0 && t >= w, "invalid dispatch counts");
	}
	let Some(engine) = desc.get("engine") else {
		return Ok(());
	};
	let phased = row["policy"] == "on";
	let b = &row[if phased { "boundary" } else { "image_boundary" }];
	let text = b["engine"]["text"]
		.as_array()
		.context("missing engine text")?;
	let inside = |key| {
		b.get(key).and_then(Value::as_u64).is_some_and(|pc| {
			text.iter()
				.any(|range| match (range[0].as_u64(), range[1].as_u64()) {
					(Some(lo), Some(hi)) => lo <= pc && pc < hi,
					_ => false,
				})
		})
	};
	ensure!(
		b["engine"]["artifact"] == *engine
			&& inside("entry_pc")
			&& (!phased || inside("split_pc"))
			&& b["caller_pc"].is_u64()
			&& b["return_pc"] == b["caller_pc"]
			&& b["status"] == "PASS"
			&& b["armed"] == true
			&& b["exit"] == 0
			&& b["thread"]
				.as_array()
				.is_some_and(|t| t.len() == 3 && t.iter().all(Value::is_u64))
			&& b["entries"] == 1
			&& b["returns"] == 1
			&& b["errors"].as_array().is_some_and(Vec::is_empty),
		"invalid engine invocation evidence"
	);
	let counts = |a: &Value| -> Result<[u64; 2]> {
		let objects = a["product_objects"]
			.as_object()
			.context("missing product objects")?;
		let runner = objects
			.get(RUNNER)
			.and_then(Value::as_u64)
			.context("missing runner count")?;
		let engine = objects
			.get(ENGINE)
			.and_then(Value::as_u64)
			.context("missing engine count")?;
		ensure!(
			objects.len() == 2
				&& runner
					.checked_add(engine)
					.context("product count overflow")?
					== a["rnx_Ir"].as_u64().context("missing product count")?,
			"product object counts differ"
		);
		Ok([runner, engine])
	};
	let whole = counts(&row["accounting"])?;
	if phased {
		let setup = counts(&row["phases"]["setup"]["accounting"])?;
		let execution = counts(&row["phases"]["execution"]["accounting"])?;
		for i in 0..2 {
			ensure!(
				setup[i].checked_add(execution[i]) == Some(whole[i]),
				"product phase counts differ"
			);
		}
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

	#[test]
	fn rejects_wrong_finish_and_lost_object_work_even_with_the_same_raw_total() {
		let desc = json!({"engine":{"sha256":"bound"}});
		let accounted = json!({"product_objects":{(RUNNER):3,(ENGINE):16},"rnx_Ir":19,"total_Ir":19,"other_Ir":0});
		let row = json!({"policy":"off","accounting":accounted,"image_boundary":{
			"engine":{"artifact":desc["engine"],"text":[[100,200]]},"entry_pc":101,"caller_pc":202,"return_pc":202,
			"entries":1,"returns":1,"armed":true,"exit":0,"thread":[1,2,3],"status":"PASS","errors":[]}});
		validate_row(&row, &desc).unwrap();
		for (key, value) in [
			("return_pc", json!(203)),
			("returns", json!(0)),
			("errors", json!(["unwind"])),
			("entry_pc", json!(90)),
		] {
			let mut wrong = row.clone();
			wrong["image_boundary"][key] = value;
			assert!(validate_row(&wrong, &desc).is_err());
		}
		let mut omitted = row;
		omitted["accounting"]["rnx_Ir"] = json!(16);
		omitted["accounting"]["other_Ir"] = json!(3);
		assert!(validate_row(&omitted, &desc).is_err());
	}
}
