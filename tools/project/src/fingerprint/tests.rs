use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
struct Temp(PathBuf);
impl Temp {
	fn new() -> Self {
		static N: AtomicUsize = AtomicUsize::new(0);
		let p = std::env::temp_dir().join(format!(
			"rnx-fingerprint-{}-{}",
			std::process::id(),
			N.fetch_add(1, Ordering::Relaxed)
		));
		fs::create_dir(&p).unwrap();
		Self(p.canonicalize().unwrap())
	}
	fn write(&self, p: &str, b: impl AsRef<[u8]>) {
		let p = self.0.join(p);
		fs::create_dir_all(p.parent().unwrap()).unwrap();
		fs::write(p, b).unwrap();
	}
	fn git(&self, args: &[&str]) {
		assert!(
			Command::new("git")
				.arg("-C")
				.arg(&self.0)
				.args(args)
				.stdout(Stdio::null())
				.stderr(Stdio::null())
				.status()
				.unwrap()
				.success()
		);
	}
	fn init(&self) {
		self.git(&["init", "--quiet"]);
		self.git(&["add", "."]);
	}
	fn source(&self) -> Tree {
		source(&self.0, false, &mut Allowance::default()).unwrap()
	}
	fn native(&self) -> Tree {
		native(&self.0, &mut Allowance::default()).unwrap()
	}
}
impl Drop for Temp {
	fn drop(&mut self) {
		fs::remove_dir_all(&self.0).unwrap();
	}
}
#[test]
fn versioned_encoding_and_changes() {
	let t = Temp::new();
	t.write("a", b"x");
	let initial = t.source();
	let mut expected = Sha256::new();
	expected.update(b"rnx-tree-v1\0");
	expected.update(1u64.to_be_bytes());
	expected.update(b"a");
	expected.update([0]);
	expected.update(1u64.to_be_bytes());
	expected.update(b"x");
	assert_eq!(initial.sha256, format!("{:x}", expected.finalize()));
	t.write("a", b"y");
	assert_ne!(initial.sha256, t.source().sha256);
	t.write("a", b"x");
	t.write("b", b"");
	assert_ne!(initial.sha256, t.source().sha256);
	fs::remove_file(t.0.join("b")).unwrap();
	assert_eq!(initial, t.source());
	fs::rename(t.0.join("a"), t.0.join("b")).unwrap();
	assert_ne!(initial.sha256, t.source().sha256);
}
#[test]
fn source_exclusions_are_exact_and_scoped() {
	let t = Temp::new();
	t.write("main.rn", b"x");
	t.write(".git/state", b"ignored");
	let before = source(&t.0, true, &mut Allowance::default()).unwrap();
	for p in [".rnx/stage", "rnx.lock", "rnx.Cargo.lock"] {
		t.write(p, b"output");
	}
	assert_eq!(
		before,
		source(&t.0, true, &mut Allowance::default()).unwrap()
	);
	assert_ne!(before.sha256, t.source().sha256);
	t.write("nested/rnx.lock", b"input");
	assert_ne!(
		before.sha256,
		source(&t.0, true, &mut Allowance::default())
			.unwrap()
			.sha256
	);
}
#[test]
fn native_working_tree_not_index_and_ignored_qualification() {
	let t = Temp::new();
	t.write("Cargo.toml", b"manifest");
	t.write("src.rs", b"old");
	t.write(".gitignore", b"ignored\n");
	t.init();
	let initial = t.native();
	t.write("src.rs", b"new");
	assert_ne!(initial.sha256, t.native().sha256);
	t.write("src.rs", b"old");
	t.write("ignored", b"outside contract");
	assert_eq!(initial, t.native());
	t.write("untracked", b"x");
	assert!(
		native(&t.0, &mut Allowance::default())
			.unwrap_err()
			.contains("untracked")
	);
	t.git(&["add", "untracked"]);
	assert_ne!(initial.sha256, t.native().sha256);
	fs::remove_file(t.0.join("src.rs")).unwrap();
	assert!(native(&t.0, &mut Allowance::default()).is_err());
}
#[test]
fn allowances_span_roots_and_bound_empty_entries() {
	let a = Temp::new();
	a.write("a", b"abc");
	let b = Temp::new();
	b.write("b", b"def");
	let mut budget = Allowance {
		entries: 2,
		bytes: 6,
	};
	source(&a.0, false, &mut budget).unwrap();
	source(&b.0, false, &mut budget).unwrap();
	assert_eq!((budget.entries, budget.bytes), (0, 0));
	let mut budget = Allowance {
		entries: 2,
		bytes: 5,
	};
	source(&a.0, false, &mut budget).unwrap();
	assert!(
		source(&b.0, false, &mut budget)
			.unwrap_err()
			.contains("byte allowance")
	);
	let mut budget = Allowance {
		entries: 0,
		bytes: BYTES,
	};
	assert!(
		source(&a.0, false, &mut budget)
			.unwrap_err()
			.contains("entry allowance")
	);
	let empty = Temp::new();
	fs::create_dir(empty.0.join("empty")).unwrap();
	assert!(
		source(
			&empty.0,
			false,
			&mut Allowance {
				entries: 0,
				bytes: BYTES
			}
		)
		.is_err()
	);
}
#[cfg(unix)]
#[test]
fn modes_symlinks_special_and_non_unicode_refuse() {
	use std::os::unix::{
		ffi::OsStringExt,
		fs::{PermissionsExt, symlink},
	};
	let t = Temp::new();
	t.write("x", b"a");
	let old = t.source();
	fs::set_permissions(t.0.join("x"), fs::Permissions::from_mode(0o755)).unwrap();
	assert_ne!(old.sha256, t.source().sha256);
	t.init();
	symlink("x", t.0.join("link")).unwrap();
	assert!(
		source(&t.0, false, &mut Allowance::default())
			.unwrap_err()
			.contains("symlink")
	);
	t.git(&["add", "link"]);
	assert!(
		native(&t.0, &mut Allowance::default())
			.unwrap_err()
			.contains("symlink")
	);
	t.git(&["rm", "--cached", "link"]);
	fs::remove_file(t.0.join("link")).unwrap();
	let weird = std::ffi::OsString::from_vec(vec![255]);
	fs::write(t.0.join(&weird), b"x").unwrap();
	assert!(
		source(&t.0, false, &mut Allowance::default())
			.unwrap_err()
			.contains("non-Unicode")
	);
	t.git(&["add", "."]);
	assert!(
		native(&t.0, &mut Allowance::default())
			.unwrap_err()
			.contains("non-Unicode")
	);
	fs::remove_file(t.0.join(weird)).unwrap();
	assert!(
		Command::new("mkfifo")
			.arg(t.0.join("fifo"))
			.status()
			.unwrap()
			.success()
	);
	assert!(
		source(&t.0, false, &mut Allowance::default())
			.unwrap_err()
			.contains("special")
	);
}
#[test]
fn submodules_and_unmerged_entries_refuse() {
	let t = Temp::new();
	t.write("x", b"a");
	t.init();
	let hash = Command::new("git")
		.arg("-C")
		.arg(&t.0)
		.args(["hash-object", "x"])
		.output()
		.unwrap();
	let hash = String::from_utf8(hash.stdout).unwrap();
	t.git(&[
		"update-index",
		"--add",
		"--cacheinfo",
		&format!("160000,{},child", hash.trim()),
	]);
	assert!(
		native(&t.0, &mut Allowance::default())
			.unwrap_err()
			.contains("submodule")
	);
}
