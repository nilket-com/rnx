use std::{
	env, fs,
	io::Write,
	path::Path,
	process::{Command, Stdio},
};

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

// Git expands this only in an archive. An unstamped source copy cannot
// borrow the identity of a repository above it.
const ARCHIVE: &str = "$Format:%H %T$";

fn object_id(text: &str) -> bool {
	text.len() == 40
		&& text
			.bytes()
			.all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

fn export_identity(root: &Path, stamp: &str) -> Result<(String, String), String> {
	let (commit, tree) = stamp.split_once(' ').ok_or("invalid Git archive stamp")?;
	if !object_id(commit) || !object_id(tree) {
		return Err("invalid Git archive stamp".into());
	}
	let proof =
		fs::read(root.join(".rnx-export")).map_err(|e| format!("missing export identity: {e}"))?;
	let split = proof
		.iter()
		.position(|b| *b == b'\n')
		.ok_or("incomplete export identity")?;
	let (declared, rune) = std::str::from_utf8(&proof[..split])
		.ok()
		.and_then(|line| line.split_once(' '))
		.ok_or("incomplete export identity")?;
	if declared != commit || !object_id(rune) {
		return Err("export revision or rune tree differs from declaration".into());
	}
	let bytes = &proof[split + 1..];
	// Git's root tree authenticates both the entry name and its subtree ID.
	let mut hash = command(root)
		.arg("--git-dir=/dev/null")
		.args(["hash-object", "--no-filters", "-t", "tree", "--stdin"])
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.spawn()
		.map_err(|e| e.to_string())?;
	hash.stdin
		.take()
		.ok_or("missing Git input")?
		.write_all(bytes)
		.map_err(|e| e.to_string())?;
	let result = hash.wait_with_output().map_err(|e| e.to_string())?;
	if !result.status.success() || result.stdout.strip_suffix(b"\n") != Some(tree.as_bytes()) {
		return Err("export root tree differs from Git archive".into());
	}
	let mut rest = bytes;
	while !rest.is_empty() {
		let end = rest
			.iter()
			.position(|b| *b == 0)
			.ok_or("invalid Git tree")?;
		let (id, next) = rest[end + 1..]
			.split_at_checked(20)
			.ok_or("invalid Git tree ID")?;
		if &rest[..end] == b"40000 rune" {
			let actual: String = id.iter().map(|b| format!("{b:02x}")).collect();
			if actual == rune {
				return Ok((commit.to_owned(), actual));
			}
			break;
		}
		rest = next;
	}
	Err("export rune subtree differs from declaration".into())
}

pub fn emit(root: &Path) -> bool {
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
	let exported = !ARCHIVE.starts_with('$');
	let (revision, tree, dirty) = if exported {
		println!(
			"cargo:rerun-if-changed={}",
			root.join(".rnx-export").display()
		);
		println!(
			"cargo:rerun-if-changed={}",
			root.join("tools/build_identity.rs").display()
		);
		let (rev, tree) = export_identity(root, ARCHIVE).expect("declared archive identity");
		(rev, tree, false)
	} else if root.join(".git").exists() {
		watch_sources(root);
		(
			git(root, &["rev-parse", "HEAD"]),
			git(root, &["rev-parse", "HEAD:rune"]),
			!git(root, &["status", "--porcelain", "--untracked-files=normal"]).is_empty(),
		)
	} else {
		watch_sources(root);
		("unknown".to_owned(), "unknown".to_owned(), true)
	};
	println!(
		"cargo:rustc-env=RNX_ENGINE_IDENTITY=0.15.0 (monorepo {revision}; engine {tree}; dirty={dirty})"
	);
	exported
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
	if !root.join(".git").exists() {
		return None;
	}
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
