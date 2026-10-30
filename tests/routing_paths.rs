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
	debug: bool,
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
	if debug {
		c.arg("--debug-engine");
	}
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
	for native in [false, true] {
		let mode = if native { "on" } else { "off" };
		let old = run(root, extra, true, native, false, &format!("{mode}-legacy"));
		let new = run(root, extra, false, native, false, &format!("{mode}-auto"));
		assert!(
			old.status.success(),
			"{}",
			String::from_utf8_lossy(&old.stderr)
		);
		assert_eq!(
			(old.status.code(), old.stdout, old.stderr),
			(new.status.code(), new.stdout, new.stderr)
		);
		let evidence = run(root, extra, false, native, true, &format!("{mode}-route"));
		assert!(evidence.status.success());
		let text = String::from_utf8(evidence.stderr).unwrap();
		let selected: Vec<_> = text
			.lines()
			.filter(|line| line.starts_with("rnx: engine="))
			.collect();
		assert_eq!(selected.len(), 1, "{text}");
		assert!(selected[0].starts_with(route), "{text}");
	}
	fs::write(
		root.join("RESULT.json"),
		b"{\"status\":\"PASS\",\"native_modes\":[\"off\",\"on\"]}\n",
	)
	.unwrap();
	println!(
		"PASS routing path; receipt {}",
		root.join("RESULT.json").display()
	);
}
#[test]
fn module_files_select_legacy_before_effects_while_the_loader_is_unsupported() {
	let root = scratch("modules");
	fs::write(
		root.join("main.rn"),
		"mod helper;pub fn main(args){println!(\"once\");[helper::value(),args]}",
	)
	.unwrap();
	fs::write(root.join("helper.rn"), "pub fn value(){42}").unwrap();
	verify(&root, &[], "rnx: engine=legacy refusal=SourceLoader");
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
