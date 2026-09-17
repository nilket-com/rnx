//! Record 0057's explicit run-only source-map boundary.
#![cfg(feature = "project-sources")]
use std::{
	io::Write,
	path::{Path, PathBuf},
	process::{Command, Output, Stdio},
	sync::atomic::{AtomicUsize, Ordering},
};
struct Tree(PathBuf);
impl Tree {
	fn new() -> Self {
		static NEXT: AtomicUsize = AtomicUsize::new(0);
		let root = std::env::temp_dir().join(format!(
			"rnx-project-cli-{}-{}",
			std::process::id(),
			NEXT.fetch_add(1, Ordering::Relaxed)
		));
		std::fs::create_dir_all(&root).unwrap();
		Self(root.canonicalize().unwrap())
	}
	fn write(&self, name: &str, text: &str) -> PathBuf {
		let path = self.0.join(name);
		std::fs::create_dir_all(path.parent().unwrap()).unwrap();
		std::fs::write(&path, text).unwrap();
		path
	}
	fn command(&self) -> Command {
		let mut c = Command::new(env!("CARGO_BIN_EXE_rnx"));
		c.current_dir(&self.0)
			.env("TERM", "xterm")
			.env("NO_COLOR", "1")
			.env("RNX_CONFIG", self.0.join("absent"))
			.env("RNX_HISTORY", self.0.join("history"));
		c
	}
	fn map(&self, entry: &Path) -> PathBuf {
		self.write("map.json",&serde_json::json!({"format":1,"entry":entry,"mounts":[{"prefix":["pkg"],"root":self.0.join("external")},{"prefix":["pkg","dep"],"root":self.0.join("transitive")}]}).to_string())
	}
}
impl Drop for Tree {
	fn drop(&mut self) {
		std::fs::remove_dir_all(&self.0).unwrap();
	}
}
fn ok(output: Output) -> String {
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert!(
		output.stderr.is_empty(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	String::from_utf8(output.stdout).unwrap()
}
#[test]
fn capability_is_early_and_machine_readable() {
	let t = Tree::new();
	let config = t.write("config.rn", "this is not valid Rune");
	let result = t
		.command()
		.env("RNX_CONFIG", config)
		.arg("project-source-version")
		.output()
		.unwrap();
	assert_eq!(ok(result), "{\"format\":1}\n");
	#[cfg(feature = "test-support")]
	{
		let count = t.0.join("reads");
		let result = t
			.command()
			.env("RNX_TEST_CONFIG_READS", &count)
			.arg("project-source-version")
			.output()
			.unwrap();
		ok(result);
		assert_eq!(std::fs::read_to_string(count).unwrap(), "0");
	}
	let result = t
		.command()
		.args(["project-source-version", "extra"])
		.output()
		.unwrap();
	assert!(!result.status.success());
}
#[test]
fn mapped_transitives_arguments_diagnostics_and_flag_positions() {
	let t = Tree::new();
	let entry = t.write(
		"app/main.rn",
		"pub mod pkg; pub fn main(args) { [pkg::value(), args] }",
	);
	t.write(
		"external/mod.rn",
		"pub mod dep; pub fn value() { self::dep::value() }",
	);
	t.write("transitive/mod.rn", "pub fn value() { 42 }");
	let map = t.map(&entry);
	let text = ok(t
		.command()
		.arg("run")
		.arg("--budget")
		.arg("200000")
		.arg("--source-map")
		.arg(&map)
		.arg(&entry)
		.args(["--source-map", "literal"])
		.output()
		.unwrap());
	assert!(text.contains("42"));
	assert!(text.contains("\"--source-map\", \"literal\""), "{text}");
	let result = t
		.command()
		.arg("run")
		.arg("--source-map")
		.arg(&map)
		.arg("--debug-source")
		.arg(&entry)
		.output()
		.unwrap();
	assert!(result.status.success());
	assert!(String::from_utf8_lossy(&result.stderr).contains("transitive"));
	t.write("transitive/mod.rn", "pub fn value() {\n  1.missing()\n}");
	let result = t
		.command()
		.arg("run")
		.arg("--source-map")
		.arg(&map)
		.arg(&entry)
		.output()
		.unwrap();
	let error = String::from_utf8_lossy(&result.stderr);
	assert!(!result.status.success());
	assert!(
		error.contains("transitive")
			&& error.contains("line 2, column 3")
			&& error.contains("no method `missing`"),
		"{error}"
	);
	assert!(error.contains("  ^"));
	t.write("transitive/mod.rn", "pub fn value() {\n let x = ;\n}");
	let result = t
		.command()
		.arg("run")
		.arg("--source-map")
		.arg(&map)
		.arg(&entry)
		.output()
		.unwrap();
	let error = String::from_utf8_lossy(&result.stderr);
	assert!(!result.status.success());
	assert!(
		error.contains("transitive") && error.contains("line 2") && error.contains('^'),
		"{error}"
	);
	for args in [
		vec!["run", "--source-map"],
		vec!["run", "--source-map", "x", "--source-map", "y", "z"],
	] {
		assert!(!t.command().args(args).output().unwrap().status.success());
	}
}
#[test]
fn refusals_precede_script_and_contexts_do_not_inherit_a_map() {
	let t = Tree::new();
	let entry = t.write("main.rn", "pub fn main(_) { print!(\"SCRIPT RAN\"); }");
	let map = t.map(&entry);
	let positive = t
		.command()
		.arg("run")
		.arg("--source-map")
		.arg(&map)
		.arg(&entry)
		.output()
		.unwrap();
	assert_eq!(ok(positive), "SCRIPT RAN");
	for text in ["{}", "{\"format\":2}", "{\"format\":1,\"format\":1}"] {
		t.write("map.json", text);
		let out = t
			.command()
			.arg("run")
			.arg("--source-map")
			.arg(&map)
			.arg(&entry)
			.output()
			.unwrap();
		assert!(!out.status.success());
		assert!(out.stdout.is_empty());
		assert!(String::from_utf8_lossy(&out.stderr).contains("source map"));
	}
	let map = t.map(&entry);
	let other = t.write("other.rn", "pub fn main(_) { print!(\"SCRIPT RAN\"); }");
	let out = t
		.command()
		.arg("run")
		.arg("--source-map")
		.arg(&map)
		.arg(other)
		.output()
		.unwrap();
	assert!(!out.status.success());
	assert!(out.stdout.is_empty());
	let out = t
		.command()
		.args(["eval", "mod pkg;"])
		.env("RNX_SOURCE_MAP", &map)
		.output()
		.unwrap();
	assert!(!out.status.success());
	assert!(String::from_utf8_lossy(&out.stderr).contains("modules"));
	let mut child = t
		.command()
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.env("RNX_SOURCE_MAP", &map)
		.spawn()
		.unwrap();
	child
		.stdin
		.take()
		.unwrap()
		.write_all(b"mod pkg;\n:q\n")
		.unwrap();
	let out = child.wait_with_output().unwrap();
	assert!(String::from_utf8_lossy(&out.stderr).contains("modules"));
	#[cfg(unix)]
	{
		use std::{ffi::CString, os::unix::ffi::OsStrExt};
		let fifo = t.0.join("fifo");
		let c = CString::new(fifo.as_os_str().as_bytes()).unwrap();
		assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
		let out = t
			.command()
			.arg("run")
			.arg("--source-map")
			.arg(fifo)
			.arg(entry)
			.output()
			.unwrap();
		assert!(!out.status.success());
		assert!(String::from_utf8_lossy(&out.stderr).contains("regular file"));
	}
}
