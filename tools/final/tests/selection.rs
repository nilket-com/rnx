//! The selection receipt matches the Python controller's for every scope,
//! except the controller's own policy identity.
use rnx_final::selection;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

#[test]
fn selections_match_the_python_controller() {
	let root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../..")
		.canonicalize()
		.unwrap();
	let out = std::env::temp_dir().join("rnx-final-selection/FINAL-x");
	// Tools as a complete final's launch names them; both controllers see them.
	let mut environment: BTreeMap<String, String> = std::env::vars().collect();
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
	environment.insert(
		"RNX_VALGRIND_ROOT".into(),
		tools.join("valgrind").to_string_lossy().into_owned(),
	);
	environment.insert(
		"RNX_JUPYTER_PYTHON".into(),
		tools.join("python").to_string_lossy().into_owned(),
	);
	let script = "import json, sys\nfrom pathlib import Path\nimport gate_policy as g\na = json.loads(sys.argv[1])\nr = g.select(Path(a['root']), Path(a['out']), a['names'], scope=a['scope'])\nr['policy'] = 'controller'\nprint(json.dumps(r))";
	for (scope, names) in [
		("complete", None),
		("required", None),
		("docs", None),
		(
			"explicit",
			Some(vec!["polars-suite".to_string(), "format".into()]),
		),
	] {
		let input = json!({"root": root, "out": out, "names": names, "scope": scope});
		let output = std::process::Command::new("python3")
			.args(["-B", "-c", script, &input.to_string()])
			.env_clear()
			.envs(&environment)
			.current_dir(root.join("tools/column"))
			.output()
			.unwrap();
		assert!(
			output.status.success(),
			"{scope}: {}",
			String::from_utf8_lossy(&output.stderr)
		);
		let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
		let mut actual = selection::select(
			&root,
			&out,
			names.as_deref(),
			"HEAD",
			Some(scope),
			&environment,
		)
		.unwrap();
		actual["policy"] = json!("controller");
		assert_eq!(actual, expected, "{scope}");
	}
	// The explicit selection closed over its producer.
	let explicit = selection::select(
		&root,
		&out,
		Some(&["polars-suite".to_string()]),
		"HEAD",
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
}
