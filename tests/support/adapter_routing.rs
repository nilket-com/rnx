//! Actual adapter executables share the one-shot routing contract. Raw streams
//! and exit statuses are retained; debug selection uses a separate invocation.
use std::{
	fs,
	path::Path,
	process::{Command, Output, Stdio},
	time::{SystemTime, UNIX_EPOCH},
};

fn invoke(binary: &str, input: &Path, legacy: bool, native: bool, debug: bool) -> Output {
	let mut command = Command::new("/usr/bin/timeout");
	command
		.args(["--kill-after=2s", "60s", binary, "--color=never", "run"])
		.arg(if native {
			"--native=on"
		} else {
			"--native=off"
		});
	if legacy {
		command.arg("--engine=legacy");
	}
	if debug {
		command.arg("--debug-engine");
	}
	command
		.arg(input)
		.stdin(Stdio::null())
		.env("RNX_CONFIG", "")
		.env_remove("RNX_MEMORY_CEILING");
	command.output().unwrap()
}

pub fn verify(binary: &str, manifest: &str, refusal: &str) {
	let nonce = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.unwrap()
		.as_nanos();
	let root =
		Path::new(manifest).join(format!("target/routing-cli/{}-{nonce}", std::process::id()));
	fs::create_dir_all(&root).unwrap();
	// The receipt starts terminal FAIL so an assertion cannot leave a false PASS.
	fs::write(root.join("RESULT.json"), b"{\"status\":\"FAIL\"}\n").unwrap();
	for (label, source, route, success) in [
		(
			"admitted",
			"pub fn main(args){let a=[];a.push(a);a}",
			"execution",
			true,
		),
		("refused", refusal, "legacy", true),
		(
			"terminal",
			"pub fn main(args){println!(\"effect-once\");panic!(\"terminal\")}",
			"execution",
			false,
		),
	] {
		let dir = root.join(label);
		fs::create_dir(&dir).unwrap();
		let input = dir.join("main.rn");
		fs::write(&input, source).unwrap();
		for native in [false, true] {
			let mode = if native { "on" } else { "off" };
			let mut runs = Vec::new();
			for (kind, legacy, debug) in [
				("legacy", true, false),
				("auto", false, false),
				("route", false, true),
			] {
				let out = invoke(binary, &input, legacy, native, debug);
				let stem = format!("{mode}-{kind}");
				fs::write(dir.join(format!("{stem}.stdout")), &out.stdout).unwrap();
				fs::write(dir.join(format!("{stem}.stderr")), &out.stderr).unwrap();
				fs::write(
					dir.join(format!("{stem}.status")),
					format!("{:?}\n", out.status.code()),
				)
				.unwrap();
				assert!(
					!matches!(out.status.code(), None | Some(124 | 137)),
					"deadline/signal: {}",
					dir.display()
				);
				runs.push(out);
			}
			let old = &runs[0];
			let new = &runs[1];
			let evidence = &runs[2];
			assert_eq!(
				(old.status.code(), &old.stdout, &old.stderr),
				(new.status.code(), &new.stdout, &new.stderr),
				"{} native={mode}",
				dir.display()
			);
			assert_eq!(new.status.success(), success, "{}", dir.display());
			let debug = String::from_utf8_lossy(&evidence.stderr);
			let selections: Vec<_> = debug
				.lines()
				.filter(|line| line.starts_with("rnx: engine="))
				.collect();
			assert_eq!(selections.len(), 1, "{}: {debug}", dir.display());
			assert!(
				selections[0].starts_with(&format!("rnx: engine={route} ")),
				"{}: {debug}",
				dir.display()
			);
			if label == "terminal" {
				assert_eq!(
					new.stdout
						.split(|b| *b == b'\n')
						.filter(|line| *line == b"effect-once")
						.count(),
					1
				);
			}
		}
	}
	fs::write(root.join("RESULT.json"), b"{\"status\":\"PASS\",\"native_modes\":[\"off\",\"on\"],\"cases\":[\"admitted\",\"refused\",\"terminal\"]}\n").unwrap();
	println!(
		"PASS adapter routing; receipt {}",
		root.join("RESULT.json").display()
	);
}
