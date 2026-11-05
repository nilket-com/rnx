use std::{env, path::Path, process::Command};

fn command(root: &Path) -> Command {
	let mut command = Command::new("git");
	command.current_dir(root);
	for (key, _) in env::vars_os() {
		if key.to_string_lossy().starts_with("GIT_") {
			command.env_remove(key);
		}
	}
	command
		.env("GIT_CONFIG_GLOBAL", "/dev/null")
		.env("GIT_CONFIG_SYSTEM", "/dev/null")
		.env("GIT_CONFIG_NOSYSTEM", "1")
		.env("GIT_OPTIONAL_LOCKS", "0")
		.env("GIT_NO_REPLACE_OBJECTS", "1");
	command
}

pub fn emit(root: &Path) {
	fn git(root: &Path, args: &[&str]) -> String {
		let output = command(root)
			.args(args)
			.output()
			.expect("git build identity");
		if !output.status.success() {
			return "unknown".to_owned();
		}
		String::from_utf8(output.stdout)
			.expect("UTF-8 git identity")
			.trim()
			.to_owned()
	}
	let revision = git(root, &["rev-parse", "HEAD"]);
	let tree = git(root, &["rev-parse", "HEAD:rune"]);
	let dirty = !git(root, &["status", "--porcelain", "--untracked-files=normal"]).is_empty();
	let mut references = vec!["HEAD".to_owned()];
	let branch = command(root)
		.args(["symbolic-ref", "--quiet", "HEAD"])
		.output()
		.expect("git branch identity");
	if branch.status.success() {
		references.push(String::from_utf8(branch.stdout).unwrap().trim().to_owned());
	}
	for reference in references {
		let path = git(root, &["rev-parse", "--git-path", &reference]);
		if path != "unknown" {
			println!("cargo:rerun-if-changed={}", root.join(path).display());
		}
	}
	println!("cargo:rerun-if-changed={}", root.join("rune").display());
	println!(
		"cargo:rerun-if-changed={}",
		root.join("Cargo.toml").display()
	);
	println!(
		"cargo:rerun-if-changed={}",
		root.join("Cargo.lock").display()
	);
	println!(
		"cargo:rustc-env=RNX_ENGINE_IDENTITY=0.15.0 (monorepo {revision}; engine {tree}; dirty={dirty})"
	);
	// Dirty identity must also notice mode-only changes and untracked sources.
	let marker = Path::new(&env::var_os("OUT_DIR").expect("Cargo output directory"))
		.join("engine-identity-rescan-required");
	let _ = std::fs::remove_file(&marker);
	println!("cargo:rerun-if-changed={}", marker.display());
}
