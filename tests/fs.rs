//! Record 0035: script-visible filesystem contracts, with isolated fixtures.
use std::{
	path::PathBuf,
	process::{Command, Output, Stdio},
	sync::atomic::{AtomicUsize, Ordering},
	time::{Duration, Instant},
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Tree(PathBuf);
impl Tree {
	fn new() -> Self {
		let path = std::env::temp_dir().join(format!(
			"rnx-fs-{}-{}",
			std::process::id(),
			NEXT.fetch_add(1, Ordering::Relaxed)
		));
		std::fs::create_dir(&path).unwrap();
		Self(path)
	}
	fn path(&self, name: &str) -> PathBuf {
		self.0.join(name)
	}
	fn command(&self, source: &str) -> Command {
		let mut c = Command::new(env!("CARGO_BIN_EXE_rnx"));
		c.current_dir(&self.0)
			.args(["eval", source])
			.env("TERM", "dumb")
			.env_remove("RNX_TEST_COPY_FAIL_AFTER");
		c
	}
	fn eval(&self, source: &str) -> String {
		let output = self.command(source).output().unwrap();
		assert!(
			output.status.success(),
			"{source}\n{}",
			String::from_utf8_lossy(&output.stderr)
		);
		String::from_utf8(output.stdout).unwrap()
	}
	fn yes(&self, expression: &str) {
		assert_eq!(self.eval(expression).trim(), "true", "{expression}");
	}
	fn err(&self, expression: &str, path: &str) -> String {
		let source =
			format!("match {expression} {{ Ok(_) => panic!(\"expected refusal\"), Err(e) => e }}");
		let out = self.eval(&source);
		assert!(out.contains(path), "{out}");
		assert!(out.contains("cannot "), "{out}");
		out
	}
}
impl Drop for Tree {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.0);
	}
}
fn bounded(mut command: Command) -> Output {
	let mut child = command
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();
	let start = Instant::now();
	while child.try_wait().unwrap().is_none() {
		if start.elapsed() > Duration::from_secs(5) {
			let _ = child.kill();
			let _ = child.wait();
			panic!("filesystem gate hung");
		}
		std::thread::sleep(Duration::from_millis(5));
	}
	child.wait_with_output().unwrap()
}
#[test]
fn read_bounds_utf8_and_bytes() {
	let t = Tree::new();
	std::fs::write(t.path("limit"), vec![b'x'; 8 * 1024 * 1024]).unwrap();
	t.yes("fs::read(\"limit\")?.len() == 8388608 && fs::read_bytes(\"limit\")?.len() == 8388608");
	std::fs::write(t.path("limit"), vec![b'x'; 8 * 1024 * 1024 + 1]).unwrap();
	for name in ["read", "read_bytes"] {
		assert!(
			t.err(&format!("fs::{name}(\"limit\")"), "limit")
				.contains("8 MiB")
		);
	}
	std::fs::write(t.path("bad"), [b'a', 255]).unwrap();
	let e = t.err("fs::read(\"bad\")", "bad");
	assert!(e.contains("byte 1") && e.contains("fs::read_bytes"));
	t.yes("fs::read_bytes(\"bad\")? == b\"a\\xff\"");
}
#[test]
fn writes_accept_only_text_or_bytes_and_preserve_existing_identity() {
	let t = Tree::new();
	t.eval("fs::write_new(\"file\", \"first\")?");
	t.err("fs::write_new(\"file\", \"second\")", "file");
	assert_eq!(std::fs::read(t.path("file")).unwrap(), b"first");
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		std::fs::set_permissions(t.path("file"), std::fs::Permissions::from_mode(0o640)).unwrap();
		std::fs::hard_link(t.path("file"), t.path("linked")).unwrap();
	}
	t.eval("fs::write(\"file\", b\"short\")?; fs::append(\"file\", \"er\")?; fs::append(\"new\", b\"new\")?; fs::write_new(\"bytes\", b\"bytes\")?");
	assert_eq!(std::fs::read(t.path("file")).unwrap(), b"shorter");
	assert_eq!(std::fs::read(t.path("new")).unwrap(), b"new");
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		assert_eq!(
			std::fs::metadata(t.path("file"))
				.unwrap()
				.permissions()
				.mode() & 0o777,
			0o640
		);
		assert_eq!(std::fs::read(t.path("linked")).unwrap(), b"shorter");
	}
	for name in ["write", "write_new", "append"] {
		assert!(
			t.err(&format!("fs::{name}(\"absent\", 42)"), "absent")
				.contains("String or Bytes")
		);
		assert!(!t.path("absent").exists());
		t.err(&format!("fs::{name}(\"file\", 42)"), "file");
		assert_eq!(std::fs::read(t.path("file")).unwrap(), b"shorter");
	}
}
#[test]
fn queries_names_and_all_path_errors() {
	let t = Tree::new();
	t.eval("fs::mkdir(\"dir\")?; fs::write_new(\"file\",\"abc\")?");
	t.yes("fs::exists(\"file\")? && fs::exists(\"dir\")? && !fs::exists(\"absent\")?");
	t.yes("let m = fs::metadata(\"file\")?; m.kind == \"file\" && m.size == 3 && !m.symlink");
	let ms = t
		.eval("fs::metadata(\"file\")?.modified_ms")
		.trim()
		.parse::<i64>()
		.unwrap();
	let now = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.unwrap()
		.as_millis() as i64;
	assert!((now - ms).abs() < 5000);
	t.yes("fs::metadata(\"dir\")?.kind == \"dir\" && fs::absolute(\".\")? == fs::cwd()?");
	assert!(!t.eval("fs::temp_dir()?").is_empty());
	for expr in [
		"fs::read(\"absent\")",
		"fs::read_bytes(\"absent\")",
		"fs::metadata(\"absent\")",
		"fs::absolute(\"absent\")",
		"fs::read_dir(\"absent\")",
		"fs::remove_file(\"absent\")",
		"fs::remove_dir(\"absent\")",
		"fs::remove_dir_all(\"absent\")",
		"fs::mkdir(\"absent/child\")",
		"fs::copy(\"absent\",\"out\")",
		"fs::rename(\"absent\",\"out\")",
	] {
		t.err(expr, "absent");
	}
	for name in ["write", "write_new", "append"] {
		t.err(
			&format!("fs::{name}(\"absent/child\",\"x\")"),
			"absent/child",
		);
	}
	t.err("fs::mkdir_all(\"file/child\")", "file/child");
	t.err("fs::read_dir(\"file\")", "file");
	t.err("fs::read(\"dir\")", "dir");
	let mut permissions = std::fs::metadata(t.path("file")).unwrap().permissions();
	permissions.set_readonly(true);
	std::fs::set_permissions(t.path("file"), permissions).unwrap();
	t.yes("fs::metadata(\"file\")?.readonly");
	// Restore so Windows can remove the fixture.
	let mut permissions = std::fs::metadata(t.path("file")).unwrap().permissions();
	#[allow(clippy::permissions_set_readonly_false)]
	permissions.set_readonly(false);
	std::fs::set_permissions(t.path("file"), permissions).unwrap();
}
#[test]
fn exclusive_copy_rename_and_directory_changes() {
	let t = Tree::new();
	t.eval("fs::write_new(\"from\",\"source\")?; fs::write_new(\"to\",\"target\")?");
	let before = [
		std::fs::metadata(t.path("from"))
			.unwrap()
			.modified()
			.unwrap(),
		std::fs::metadata(t.path("to")).unwrap().modified().unwrap(),
	];
	for name in ["copy", "rename"] {
		assert!(
			t.err(&format!("fs::{name}(\"from\",\"to\")"), "from")
				.contains("to")
		);
		assert_eq!(std::fs::read(t.path("from")).unwrap(), b"source");
		assert_eq!(std::fs::read(t.path("to")).unwrap(), b"target");
		assert_eq!(
			[
				std::fs::metadata(t.path("from"))
					.unwrap()
					.modified()
					.unwrap(),
				std::fs::metadata(t.path("to")).unwrap().modified().unwrap()
			],
			before
		);
	}
	t.eval("fs::copy(\"from\",\"copied\")?; fs::rename(\"copied\",\"renamed\")?; fs::mkdir_all(\"a/b/c\")?; fs::mkdir_all(\"a/b/c\")?");
	assert_eq!(std::fs::read(t.path("renamed")).unwrap(), b"source");
	assert!(!t.path("copied").exists());
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		assert_eq!(
			std::fs::metadata(t.path("from"))
				.unwrap()
				.permissions()
				.mode(),
			std::fs::metadata(t.path("renamed"))
				.unwrap()
				.permissions()
				.mode()
		);
	}
	t.err("fs::copy(\"a\",\"not-created\")", "a");
	assert!(!t.path("not-created").exists());
	t.err("fs::remove_dir(\"a\")", "a");
	t.eval("fs::remove_file(\"renamed\")?; fs::remove_dir(\"a/b/c\")?; fs::remove_dir_all(\"a\")?");
	assert!(!t.path("a").exists());
	assert!(
		t.err("fs::remove_dir_all(\".\")", ".")
			.contains("working directory")
	);
	assert!(
		t.err("fs::remove_dir_all(\"..\")", "..")
			.contains("ancestor")
	);
	let root = t.0.ancestors().last().unwrap().to_str().unwrap();
	assert!(
		t.err(&format!("fs::remove_dir_all({root:?})"), root)
			.contains("root")
	);
}
#[test]
fn directory_listing_is_sorted_and_bounded() {
	let t = Tree::new();
	for name in ["z", "é", "A"] {
		std::fs::write(t.path(name), b"").unwrap();
	}
	t.yes("fs::read_dir(\".\")? == [\"A\",\"z\",\"é\"]");
	std::fs::create_dir(t.path("many")).unwrap();
	for i in 0..100_000 {
		std::fs::File::create(t.path("many").join(i.to_string())).unwrap();
	}
	t.yes("fs::read_dir(\"many\")?.len() == 100000");
	std::fs::File::create(t.path("many/extra")).unwrap();
	assert!(t.err("fs::read_dir(\"many\")", "many").contains("100000"));
}
#[test]
#[cfg(unix)]
fn links_nonunicode_fifo_and_permission_refusals() {
	use std::os::unix::{
		ffi::OsStringExt,
		fs::{PermissionsExt, symlink},
	};
	let t = Tree::new();
	std::fs::write(t.path("file"), b"x").unwrap();
	symlink("file", t.path("link")).unwrap();
	symlink("nothing", t.path("dangling")).unwrap();
	t.yes("fs::metadata(\"link\")?.symlink && !fs::exists(\"dangling\")?");
	assert!(
		t.err("fs::metadata(\"dangling\")", "dangling")
			.contains("target is missing")
	);
	for name in ["copy", "rename"] {
		t.err(&format!("fs::{name}(\"file\",\"dangling\")"), "dangling");
		assert!(
			std::fs::symlink_metadata(t.path("dangling"))
				.unwrap()
				.is_symlink()
		);
	}
	t.eval("fs::remove_file(\"link\")?; fs::remove_dir_all(\"dangling\")?");
	assert!(t.path("file").exists());
	symlink(".", t.path("self-link")).unwrap();
	t.eval("fs::remove_dir_all(\"self-link\")?");
	assert!(t.0.exists());
	std::fs::create_dir(t.path("tree")).unwrap();
	symlink("../file", t.path("tree/link")).unwrap();
	t.eval("fs::remove_dir_all(\"tree\")?");
	assert!(t.path("file").exists());
	let fifo = std::ffi::CString::new(t.path("fifo").as_os_str().as_encoded_bytes()).unwrap();
	assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
	for expr in [
		"fs::read(\"fifo\")",
		"fs::read_bytes(\"fifo\")",
		"fs::copy(\"fifo\",\"not-created\")",
	] {
		let out = bounded(t.command(&format!(
			"match {expr} {{ Ok(_) => panic!(\"expected refusal\"), Err(e) => e }}"
		)));
		assert!(out.status.success());
		assert!(String::from_utf8_lossy(&out.stdout).contains("FIFO"));
	}
	assert!(!t.path("not-created").exists());
	std::fs::create_dir(t.path("denied")).unwrap();
	std::fs::set_permissions(t.path("denied"), std::fs::Permissions::from_mode(0)).unwrap();
	if unsafe { libc::geteuid() } != 0 {
		t.err("fs::exists(\"denied/child\")", "denied/child");
	}
	std::fs::set_permissions(t.path("denied"), std::fs::Permissions::from_mode(0o700)).unwrap();
	let name = std::ffi::OsString::from_vec(vec![b'x', 255]);
	std::fs::write(t.0.join(name), b"").unwrap();
	let e = t.err("fs::read_dir(\".\")", ".");
	assert!(e.contains("non-Unicode") && e.contains("FF"), "{e}");
	// Unix timestamps immediately before epoch must floor, not truncate to zero.
	let p = std::ffi::CString::new(t.path("file").as_os_str().as_encoded_bytes()).unwrap();
	let times = [libc::timespec {
		tv_sec: -1,
		tv_nsec: 999_999_999,
	}; 2];
	assert_eq!(
		unsafe { libc::utimensat(libc::AT_FDCWD, p.as_ptr(), times.as_ptr(), 0) },
		0
	);
	t.yes("fs::metadata(\"file\")?.modified_ms == -1");
}
#[test]
#[cfg(feature = "test-support")]
fn failed_copy_leaves_exact_partial_destination() {
	let t = Tree::new();
	std::fs::write(t.path("from"), b"0123456789").unwrap();
	let mut c = t.command(
		"match fs::copy(\"from\",\"to\") { Ok(_) => panic!(\"expected error\"), Err(e) => e }",
	);
	c.env("RNX_TEST_COPY_FAIL_AFTER", "4");
	let out = c.output().unwrap();
	assert!(out.status.success());
	assert_eq!(std::fs::read(t.path("to")).unwrap(), b"0123");
	let e = String::from_utf8_lossy(&out.stdout);
	assert!(
		e.contains("from to to") && e.contains("destination was created and left"),
		"{e}"
	);
}
#[test]
fn module_names_migrate_and_run_and_session_work() {
	let t = Tree::new();
	for name in ["read", "write_new", "mkdir", "absolute"] {
		let o = t.command(&format!("host::{name}")).output().unwrap();
		assert!(!o.status.success());
	}
	let script = t.path("run.rn");
	std::fs::write(
		&script,
		"pub fn main(_) { fs::write_new(\"made\",\"yes\")?; fs::read(\"made\") }",
	)
	.unwrap();
	let out = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.current_dir(&t.0)
		.arg("run")
		.arg(script)
		.output()
		.unwrap();
	assert!(out.status.success());
	assert!(String::from_utf8_lossy(&out.stdout).contains("yes"));
	use std::io::Write;
	let mut c = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.current_dir(&t.0)
		.env("TERM", "dumb")
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();
	c.stdin
		.take()
		.unwrap()
		.write_all(b"fs::read(\"made\")?\n:help fs::copy\n:quit\n")
		.unwrap();
	let out = c.wait_with_output().unwrap();
	assert!(out.status.success());
	let text = String::from_utf8_lossy(&out.stdout);
	assert!(
		text.contains("yes") && text.contains("exclusively"),
		"{text}"
	);
}
