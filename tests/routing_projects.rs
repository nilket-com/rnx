//! Explicit end-to-end gate: the real management CLI, an executable override,
//! and a genuinely generated assembly. No mode change may relock or rebuild.
#![cfg(all(unix, feature = "stock-management", feature = "project-sources"))]
use std::{
	fs,
	path::{Path, PathBuf},
	process::{Command, Output, Stdio},
	time::{SystemTime, UNIX_EPOCH},
};

fn command(root: &Path, manifest: &Path, args: &[&str], label: &str, timeout: &str) -> Output {
	let mut command = Command::new("/usr/bin/timeout");
	command
		.args([
			"--kill-after=5s",
			timeout,
			env!("CARGO_BIN_EXE_rnx"),
			"project",
		])
		.args(args)
		.arg("--manifest")
		.arg(manifest)
		.current_dir(root)
		.stdin(Stdio::null())
		.env("NO_COLOR", "1")
		.env("RNX_CONFIG", "");
	// The product deliberately rejects inherited Cargo build overrides.
	for (key, _) in std::env::vars_os() {
		let name = key.to_string_lossy();
		if ((name.starts_with("CARGO_") && name != "CARGO_HOME") || name.starts_with("RUST"))
			&& !matches!(name.as_ref(), "RUSTUP_HOME" | "RUSTUP_TOOLCHAIN")
		{
			command.env_remove(key);
		}
	}
	if args.first() == Some(&"run") {
		command.args(["--", "sentinel", "--engine=legacy"]);
	}
	let out = command.output().unwrap();
	// Captures are outside the source tree whose identity lock/build verifies.
	let logs = root.parent().unwrap().join("logs");
	fs::create_dir_all(&logs).unwrap();
	let label = format!("{}-{label}", root.file_name().unwrap().to_string_lossy());
	fs::write(logs.join(format!("{label}.stdout")), &out.stdout).unwrap();
	fs::write(logs.join(format!("{label}.stderr")), &out.stderr).unwrap();
	fs::write(
		logs.join(format!("{label}.status")),
		format!("{:?}\n", out.status.code()),
	)
	.unwrap();
	assert!(
		!matches!(out.status.code(), None | Some(124 | 137)),
		"deadline/signal: {}",
		root.display()
	);
	out
}

fn fingerprints(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
	["rnx.lock", "rnx.Cargo.lock", ".rnx/receipt.json"]
		.into_iter()
		.map(|name| root.join(name))
		.filter(|path| path.exists())
		.map(|path| {
			let bytes = fs::read(&path).unwrap();
			(path, bytes)
		})
		.collect()
}

#[test]
#[ignore = "0197 explicit fresh gate: builds a generated assembly with the locked Rune pin"]
fn actual_project_cli_routes_overrides_and_generated_assemblies_without_rebuilding() {
	let nonce = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.unwrap()
		.as_nanos();
	// A generated project's outputs must also be outside the runtime package
	// root, so neither source nor native fingerprints can include them.
	let out = std::env::temp_dir().join(format!(
		"rnx-routing-projects-{}-{nonce}",
		std::process::id()
	));
	fs::create_dir_all(&out).unwrap();
	fs::write(out.join("RESULT.json"), b"{\"status\":\"FAIL\"}\n").unwrap();
	for generated in [false, true] {
		let root = out.join(if generated { "generated" } else { "override" });
		fs::create_dir(&root).unwrap();
		let selection = if generated {
			format!(
				"[runtime]\npath={}\n",
				serde_json::to_string(env!("CARGO_MANIFEST_DIR")).unwrap()
			)
		} else {
			format!(
				"[executable]\npath={}\n",
				serde_json::to_string(env!("CARGO_BIN_EXE_rnx")).unwrap()
			)
		};
		let manifest = root.join("rnx.toml");
		fs::write(
			&manifest,
			format!("format=1\n[application]\nentry='main.rn'\n{selection}"),
		)
		.unwrap();
		let input = root.join("main.rn");
		fs::write(&input, "pub fn main(args){let a=[];a.push(a);[a,args]}").unwrap();
		for step in ["lock", "build"] {
			let result = command(&root, &manifest, &[step, "--offline"], step, "1800s");
			assert!(
				result.status.success(),
				"{}: {}",
				root.display(),
				String::from_utf8_lossy(&result.stderr)
			);
		}
		let before = fingerprints(&root);
		for native in ["--native=off", "--native=on"] {
			let old = command(
				&root,
				&manifest,
				&["run", native, "--engine=legacy"],
				&format!("{native}-legacy"),
				"60s",
			);
			let new = command(
				&root,
				&manifest,
				&["run", native],
				&format!("{native}-auto"),
				"60s",
			);
			assert!(
				old.status.success(),
				"{}",
				String::from_utf8_lossy(&old.stderr)
			);
			assert_eq!(
				(old.status.code(), old.stdout, old.stderr),
				(new.status.code(), new.stdout, new.stderr),
				"{}",
				root.display()
			);
			let evidence = command(
				&root,
				&manifest,
				&["run", native, "--debug-engine"],
				&format!("{native}-route"),
				"60s",
			);
			assert!(evidence.status.success());
			let stderr = String::from_utf8(evidence.stderr).unwrap();
			let routes: Vec<_> = stderr
				.lines()
				.filter(|line| line.starts_with("rnx: engine="))
				.collect();
			assert_eq!(
				routes,
				[format!(
					"rnx: engine=execution native-requested={}",
					native.strip_prefix("--native=").unwrap()
				)
				.as_str()]
			);
			assert_eq!(
				before,
				fingerprints(&root),
				"routing flags changed lock identity"
			);
		}
	}
	fs::write(out.join("RESULT.json"), b"{\"status\":\"PASS\",\"assemblies\":[\"override\",\"generated\"],\"native_modes\":[\"off\",\"on\"]}\n").unwrap();
	println!(
		"PASS project routing; receipt {}",
		out.join("RESULT.json").display()
	);
}
