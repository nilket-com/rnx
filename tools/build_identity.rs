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
	watch_sources(root);
	println!(
		"cargo:rustc-env=RNX_ENGINE_IDENTITY=0.15.0 (monorepo {revision}; engine {tree}; dirty={dirty})"
	);
}

/// Rerun the build script when the checkout's revision or any tracked file
/// changes, and at no other time. Cargo watches files by modification time,
/// so a new untracked file or a mode-only edit is seen at the next change to
/// one of these. Watching directories instead would also watch the ignored
/// build output beneath them, which changes on every build. The index is not
/// watched: the identity's own Git commands refresh it.
pub fn watch_sources(root: &Path) {
	match watched_paths(root) {
		Some(paths) => {
			for path in paths {
				println!("cargo:rerun-if-changed={}", path.display());
			}
		}
		// Outside a checkout nothing identifies the source, so every build
		// rescans.
		None => println!("cargo:rerun-if-changed={}", root.join(".git").display()),
	}
}

/// Every path names an existing file or directory: Cargo treats a missing
/// one as always out of date, which would rebuild on every invocation.
/// `packed-refs` and the branch's loose reference each exist only in some
/// states, so the loose reference's directory is watched too: creating,
/// rewriting or packing that reference changes it.
fn watched_paths(root: &Path) -> Option<Vec<std::path::PathBuf>> {
	let mut watched = vec![git_path(root, "HEAD")?, git_path(root, "packed-refs")?];
	let branch = command(root)
		.args(["symbolic-ref", "--quiet", "HEAD"])
		.output()
		.expect("git branch identity");
	if branch.status.success() {
		let reference = String::from_utf8(branch.stdout).expect("UTF-8 branch");
		let loose = git_path(root, reference.trim())?;
		let directory = loose
			.ancestors()
			.skip(1)
			.find(|d| d.is_dir())?
			.to_path_buf();
		watched.extend([loose, directory]);
	}
	let tracked = command(root)
		.args(["ls-files", "-z"])
		.output()
		.expect("git tracked files");
	if !tracked.status.success() {
		return None;
	}
	for name in tracked.stdout.split(|b| *b == 0).filter(|n| !n.is_empty()) {
		let name = std::str::from_utf8(name).expect("UTF-8 tracked path");
		watched.push(root.join(name));
	}
	watched.retain(|path| path.exists());
	Some(watched)
}

fn git_path(root: &Path, path: &str) -> Option<std::path::PathBuf> {
	let output = command(root)
		.args(["rev-parse", "--git-path", path])
		.output()
		.ok()?;
	if !output.status.success() {
		return None;
	}
	let relative = String::from_utf8(output.stdout).ok()?;
	Some(root.join(relative.trim()))
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::{fs, path::PathBuf};

	fn repository(name: &str) -> PathBuf {
		let root = env::temp_dir().join(format!("rnx-identity-{name}-{}", std::process::id()));
		let _ = fs::remove_dir_all(&root);
		fs::create_dir_all(root.join("src")).unwrap();
		fs::write(root.join("src/lib.rs"), "").unwrap();
		fs::write(root.join("README"), "").unwrap();
		for args in [
			&["init", "-q", "-b", "main"][..],
			&["add", "-A"],
			&[
				"-c",
				"user.name=t",
				"-c",
				"user.email=t@t",
				"-c",
				"commit.gpgsign=false",
				"commit",
				"-q",
				"-m",
				"x",
			],
		] {
			assert!(command(&root).args(args).status().unwrap().success());
		}
		root
	}

	fn assert_watchable(root: &Path) -> Vec<PathBuf> {
		let paths = watched_paths(root).expect("a checkout");
		for path in &paths {
			assert!(path.exists(), "{} is missing", path.display());
		}
		assert!(
			paths.iter().any(|p| p.ends_with(".git/refs/heads")),
			"{paths:?}"
		);
		assert!(paths.iter().any(|p| p.ends_with("src/lib.rs")), "{paths:?}");
		paths
	}

	#[test]
	fn loose_branch_without_packed_refs() {
		let root = repository("loose");
		assert!(!root.join(".git/packed-refs").exists());
		let paths = assert_watchable(&root);
		assert!(paths.iter().any(|p| p.ends_with(".git/refs/heads/main")));
		fs::remove_dir_all(root).unwrap();
	}

	#[test]
	fn packed_branch_without_a_loose_reference() {
		let root = repository("packed");
		assert!(
			command(&root)
				.args(["pack-refs", "--all", "--prune"])
				.status()
				.unwrap()
				.success()
		);
		assert!(!root.join(".git/refs/heads/main").exists());
		let paths = assert_watchable(&root);
		assert!(paths.iter().any(|p| p.ends_with(".git/packed-refs")));
		fs::remove_dir_all(root).unwrap();
	}

	#[test]
	fn deleted_tracked_file_is_not_watched() {
		let root = repository("deleted");
		fs::remove_file(root.join("README")).unwrap();
		let paths = assert_watchable(&root);
		assert!(!paths.iter().any(|p| p.ends_with("README")));
		fs::remove_dir_all(root).unwrap();
	}

	#[test]
	fn outside_a_checkout_nothing_is_watched() {
		let root = env::temp_dir().join(format!("rnx-identity-none-{}", std::process::id()));
		fs::create_dir_all(&root).unwrap();
		assert!(watched_paths(&root).is_none());
		fs::remove_dir_all(root).unwrap();
	}
}
