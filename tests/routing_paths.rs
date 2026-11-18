//! Live file/module/source-map routing through the actual rnx executable.
#![cfg(unix)]
use std::{
	fs,
	path::{Path, PathBuf},
	process::{Command, Output, Stdio},
	time::{SystemTime, UNIX_EPOCH},
};
fn scratch(name: &str) -> PathBuf {
	let nonce = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.unwrap()
		.as_nanos();
	let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
		"target/routing-paths/{name}-{}-{nonce}",
		std::process::id()
	));
	fs::create_dir_all(&root).unwrap();
	fs::write(root.join("RESULT.json"), b"{\"status\":\"FAIL\"}\n").unwrap();
	root
}
fn run(
	root: &Path,
	extra: &[&Path],
	legacy: bool,
	native: bool,
	debug: &[&str],
	label: &str,
) -> Output {
	let mut c = Command::new("/usr/bin/timeout");
	c.args([
		"--kill-after=2s",
		"60s",
		env!("CARGO_BIN_EXE_rnx"),
		"--color=never",
		"run",
	])
	.arg(if native {
		"--native=on"
	} else {
		"--native=off"
	});
	if legacy {
		c.arg("--engine=legacy");
	}
	c.args(debug);
	if !extra.is_empty() {
		c.arg("--source-map").arg(extra[0]);
	}
	c.arg(root.join("main.rn"))
		.args(["sentinel", "--engine=legacy"])
		.stdin(Stdio::null())
		.env("RNX_CONFIG", "")
		.current_dir(root);
	let output = c.output().unwrap();
	fs::write(root.join(format!("{label}.stdout")), &output.stdout).unwrap();
	fs::write(root.join(format!("{label}.stderr")), &output.stderr).unwrap();
	fs::write(
		root.join(format!("{label}.status")),
		format!("{:?}\n", output.status.code()),
	)
	.unwrap();
	assert!(
		!matches!(output.status.code(), None | Some(124 | 137)),
		"deadline/signal: {}",
		root.display()
	);
	output
}
fn verify(root: &Path, extra: &[&Path], route: &str) {
	verify_outcome(root, extra, route, true);
}

fn verify_outcome(root: &Path, extra: &[&Path], route: &str, success: bool) {
	let mut routes = Vec::new();
	for native in [false, true] {
		let mode = if native { "on" } else { "off" };
		let old = run(root, extra, true, native, &[], &format!("{mode}-legacy"));
		let new = run(root, extra, false, native, &[], &format!("{mode}-auto"));
		assert_eq!(
			old.status.success(),
			success,
			"{}",
			String::from_utf8_lossy(&old.stderr)
		);
		assert_eq!(
			(old.status.code(), old.stdout, old.stderr),
			(new.status.code(), new.stdout, new.stderr)
		);
		let evidence = run(
			root,
			extra,
			false,
			native,
			&["--debug-engine"],
			&format!("{mode}-route"),
		);
		assert_eq!(evidence.status.success(), success);
		let text = String::from_utf8(evidence.stderr).unwrap();
		let selected: Vec<_> = text
			.lines()
			.filter(|line| line.starts_with("rnx: engine="))
			.collect();
		assert_eq!(selected.len(), 1, "{text}");
		assert!(selected[0].starts_with(route), "{text}");
		routes.push(
			serde_json::json!({"native": mode, "route": selected[0], "exit": evidence.status.code()}),
		);
	}
	fs::write(
		root.join("RESULT.json"),
		serde_json::json!({"status":"PASS","native_modes":["off","on"],"auto_routes":routes})
			.to_string(),
	)
	.unwrap();
	println!(
		"PASS routing path; receipt {}",
		root.join("RESULT.json").display()
	);
}
#[test]
fn nested_module_files_use_execution_and_keep_layout_precedence() {
	let root = scratch("modules");
	fs::write(
		root.join("main.rn"),
		"mod helper;pub fn main(args){println!(\"once\");[helper::value(),args]}",
	)
	.unwrap();
	fs::create_dir(root.join("helper")).unwrap();
	fs::write(root.join("helper.rn"), "pub fn value(){999}").unwrap();
	fs::write(
		root.join("helper/mod.rn"),
		"mod nested;pub fn value(){nested::value()}",
	)
	.unwrap();
	fs::write(root.join("helper/nested.rn"), "pub fn value(){42}").unwrap();
	verify(&root, &[], "rnx: engine=execution ");
	let debug = run(&root, &[], false, false, &["--debug-source"], "source");
	let text = String::from_utf8(debug.stderr).unwrap();
	for file in ["main.rn", "helper/mod.rn", "helper/nested.rn"] {
		let mark = format!("--- compiled source of {}", root.join(file).display());
		assert_eq!(text.matches(&mark).count(), 1, "{text}");
	}
	assert!(!text.contains("999"), "{text}");
}

#[test]
fn refused_module_falls_back_before_side_effects_and_shows_each_source_once() {
	let root = scratch("module-refusal");
	fs::write(
		root.join("main.rn"),
		"mod helper;pub fn main(args){println!(\"once\");helper::value()}",
	)
	.unwrap();
	fs::write(root.join("helper.rn"), "pub fn value(){let x=b\"abc\";42}").unwrap();
	verify(&root, &[], "rnx: engine=legacy refusal=ByteStr");
	let debug = run(&root, &[], false, false, &["--debug-source"], "source");
	assert_eq!(debug.stdout, b"once\n42\n");
	let text = String::from_utf8(debug.stderr).unwrap();
	for file in ["main.rn", "helper.rn"] {
		let mark = format!("--- compiled source of {}", root.join(file).display());
		assert_eq!(text.matches(&mark).count(), 1, "{text}");
	}
}

#[test]
fn missing_read_and_allowance_errors_keep_the_declaring_module_location() {
	for error in ["missing", "read", "allowance"] {
		let root = scratch(error);
		fs::write(
			root.join("main.rn"),
			"mod helper;pub fn main(args){println!(\"unexpected\");helper::value()}",
		)
		.unwrap();
		fs::write(
			root.join("helper.rn"),
			"mod child;pub fn value(){child::value()}",
		)
		.unwrap();
		fs::create_dir(root.join("helper")).unwrap();
		match error {
			"read" => fs::write(root.join("helper/child.rn"), [0xff]).unwrap(),
			"allowance" => {
				fs::write(root.join("helper/child.rn"), vec![b' '; 8 * 1024 * 1024]).unwrap()
			}
			_ => {}
		}
		verify_outcome(
			&root,
			&[],
			"rnx: engine=legacy refusal=FrontendDiagnostic",
			false,
		);
		let result = run(&root, &[], false, false, &[], "diagnostic");
		assert!(result.stdout.is_empty());
		let text = String::from_utf8(result.stderr).unwrap();
		assert!(
			text.contains(&format!(
				"{}, line 1, column 1:",
				root.join("helper.rn").display()
			)),
			"{text}"
		);
		assert!(text.contains("mod child;"), "{text}");
	}
}

#[test]
fn a_runtime_failure_in_a_module_is_located_and_terminal() {
	let root = scratch("module-runtime");
	fs::write(
		root.join("main.rn"),
		"mod helper;pub fn main(args){println!(\"once\");helper::value()}",
	)
	.unwrap();
	fs::write(
		root.join("helper.rn"),
		"pub fn value(){\n  panic!(\"module failure\")\n}",
	)
	.unwrap();
	verify_outcome(&root, &[], "rnx: engine=execution ", false);
	let result = run(&root, &[], false, false, &[], "diagnostic");
	assert_eq!(result.stdout, b"once\n");
	let text = String::from_utf8(result.stderr).unwrap();
	assert!(
		text.contains(&format!(
			"{}, line 2, column 3:",
			root.join("helper.rn").display()
		)),
		"{text}"
	);
}

#[test]
#[cfg(feature = "project-sources")]
fn explicit_source_map_preserves_admission_and_argument_boundaries() {
	let root = scratch("source-map");
	fs::write(
		root.join("main.rn"),
		"pub fn main(args){let a=[];a.push(a);[a,args]}",
	)
	.unwrap();
	let map = root.join("map.json");
	fs::write(
		&map,
		serde_json::to_vec(
			&serde_json::json!({"format":1,"entry":root.join("main.rn"),"mounts":[]}),
		)
		.unwrap(),
	)
	.unwrap();
	verify(&root, &[&map], "rnx: engine=execution native-requested=");
}

#[test]
#[cfg(feature = "project-sources")]
fn explicit_source_map_loads_an_external_module_and_keeps_entry_admission() {
	let root = scratch("source-map");
	fs::write(
		root.join("main.rn"),
		"mod pkg;pub fn main(args){[pkg::value(),args]}",
	)
	.unwrap();
	let package = scratch("mapped-module");
	fs::write(package.join("mod.rn"), "pub fn value(){42}").unwrap();
	let map = root.join("map.json");
	fs::write(
		&map,
		serde_json::to_vec(
			&serde_json::json!({"format":1,"entry":root.join("main.rn"),"mounts":[{"prefix":["pkg"],"root":package}]}),
		)
		.unwrap(),
	)
	.unwrap();
	verify(&root, &[&map], "rnx: engine=execution native-requested=");

	fs::write(
		&map,
		serde_json::json!({"format":1,"entry":root.join("other.rn"),"mounts":[]}).to_string(),
	)
	.unwrap();
	let legacy = run(&root, &[&map], true, false, &[], "bad-map-legacy");
	let auto = run(
		&root,
		&[&map],
		false,
		false,
		&["--debug-engine"],
		"bad-map-auto",
	);
	assert_eq!(
		(legacy.status.code(), &legacy.stdout, &legacy.stderr),
		(auto.status.code(), &auto.stdout, &auto.stderr)
	);
	assert_eq!(auto.status.code(), Some(1));
	let text = String::from_utf8(auto.stderr).unwrap();
	assert!(
		text.contains("source map entry does not match the absolute run entry"),
		"{text}"
	);
}

#[test]
fn default_native_request_matches_on_and_off_stays_explicit_even_under_perf_environment() {
	let root = scratch("native-default");
	let input = root.join("main.rn");
	fs::write(
		&input,
		"pub fn main(args){println!(\"once\");let n=0;while n<10000{n+=1;}n}",
	)
	.unwrap();
	for environment in [false, true] {
		for (label, option, requested) in [
			("default", None, "on"),
			("on", Some("--native=on"), "on"),
			("off", Some("--native=off"), "off"),
		] {
			let mut command = Command::new("/usr/bin/timeout");
			command.args([
				"--kill-after=2s",
				"60s",
				env!("CARGO_BIN_EXE_rnx"),
				"--color=never",
				"run",
				"--debug-engine",
			]);
			if let Some(option) = option {
				command.arg(option);
			}
			command
				.arg(&input)
				.current_dir(&root)
				.stdin(Stdio::null())
				.env("RNX_CONFIG", "");
			if environment {
				command.env("PERF_BUILDID_DIR", root.join("perf"));
			} else {
				command.env_remove("PERF_BUILDID_DIR");
			}
			let out = command.output().unwrap();
			let label = format!("{label}-perf-env-{environment}");
			fs::write(root.join(format!("{label}.stdout")), &out.stdout).unwrap();
			fs::write(root.join(format!("{label}.stderr")), &out.stderr).unwrap();
			fs::write(
				root.join(format!("{label}.status")),
				format!("{:?}\n", out.status.code()),
			)
			.unwrap();
			assert_eq!(
				out.status.code(),
				Some(0),
				"{label}: {}",
				String::from_utf8_lossy(&out.stderr)
			);
			assert_eq!(out.stdout, b"once\n10000\n", "{label}");
			// This is a request report, including when the environment disables loading.
			assert_eq!(
				out.stderr,
				format!("rnx: engine=execution native-requested={requested}\n").as_bytes(),
				"{label}"
			);
		}
	}
	fs::write(root.join("RESULT.json"), b"{\"status\":\"PASS\"}\n").unwrap();
}
