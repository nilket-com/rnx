//! The selection receipt of every scope matches the Python controller's at
//! ADR 0284's witness revision, except the controller's own policy
//! identity and the working tree's source digest, which the retained Python
//! stage protocol computes for this tree.
mod common;
use common::witness::{WITNESS, expected};
use rnx_final::selection;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

#[test]
fn selections_match_the_witness() {
	let root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../..")
		.canonicalize()
		.unwrap();
	let out = std::env::temp_dir().join("rnx-final-selection/FINAL-x");
	// Tools as a complete final's launch names them.
	let tools = std::env::temp_dir().join(format!(
		"rnx-final-selection-tools-{}",
		rnx_final::receipt::invocation_id()
	));
	{
		use std::os::unix::fs::PermissionsExt as _;
		std::fs::create_dir_all(tools.join("valgrind/bin")).unwrap();
		std::fs::write(tools.join("valgrind/bin/valgrind"), "#!/bin/sh\n").unwrap();
		std::fs::set_permissions(
			tools.join("valgrind/bin/valgrind"),
			std::fs::Permissions::from_mode(0o755),
		)
		.unwrap();
		std::fs::write(tools.join("python"), "").unwrap();
	}
	let environment: BTreeMap<String, String> = [
		("PATH".to_string(), std::env::var("PATH").unwrap()),
		(
			"RNX_VALGRIND_ROOT".into(),
			tools.join("valgrind").to_string_lossy().into_owned(),
		),
		(
			"RNX_JUPYTER_PYTHON".into(),
			tools.join("python").to_string_lossy().into_owned(),
		),
	]
	.into();
	let tag = {
		use sha2::Digest as _;
		let resolved = std::fs::canonicalize(&out).unwrap_or_else(|_| out.clone());
		rnx_final::pyjson::hex(&sha2::Sha256::digest(resolved.to_string_lossy().as_bytes()))
	};
	let fixture = expected(
		"selection",
		&[
			("<OUT>", &out.to_string_lossy()),
			("<ROOT>", &root.to_string_lossy()),
			("<TOOLS>", &tools.to_string_lossy()),
			("<TAG>", &tag[..16]),
		],
	);
	let source = python_source(&root);
	let scopes = fixture.as_object().unwrap();
	assert_eq!(scopes.len(), 4);
	for (scope, case) in scopes {
		let names: Option<Vec<String>> = serde_json::from_value(case["names"].clone()).unwrap();
		let mut want = case["expected"].clone();
		want["source"] = json!(source);
		let mut actual = selection::select(
			&root,
			&out,
			names.as_deref(),
			WITNESS,
			Some(scope),
			&environment,
		)
		.unwrap();
		actual["policy"] = json!("controller");
		assert_eq!(actual, want, "{scope}");
	}
	// The explicit selection closed over its producer.
	let explicit = selection::select(
		&root,
		&out,
		Some(&["polars-suite".to_string()]),
		WITNESS,
		Some("explicit"),
		&environment,
	)
	.unwrap();
	assert!(
		explicit["stages"]
			.as_array()
			.unwrap()
			.iter()
			.any(|s| s[0] == "polars-inventories")
	);
	let _ = std::fs::remove_dir_all(&tools);
}

/// The working tree's source digest as the Python stage protocol computes it.
fn python_source(root: &Path) -> Value {
	let script = "import json, sys; from pathlib import Path; import child_result; print(json.dumps(child_result.source(Path(sys.argv[1]))))";
	let output = std::process::Command::new("python3")
		.args(["-B", "-c", script])
		.arg(root)
		.current_dir(root.join("tools/column"))
		.output()
		.unwrap();
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	serde_json::from_slice(&output.stdout).unwrap()
}
