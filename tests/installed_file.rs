#![cfg(all(
	feature = "stock-management",
	target_arch = "x86_64",
	target_os = "linux",
	target_env = "gnu"
))]

#[path = "harness/scratch.rs"]
mod scratch;

use scratch::Scratch;
use std::ffi::{OsStr, OsString};
use std::fs::{self, Permissions};
use std::os::unix::{
	ffi::OsStringExt,
	fs::{PermissionsExt, symlink},
};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

struct Installed {
	root: Scratch,
}

impl Installed {
	fn new(source: &str) -> Self {
		let root = Scratch::dir(
			std::env::temp_dir().join(format!("rnx-installed-file-{}", std::process::id())),
		);
		fs::write(root.join("test.rn"), source).unwrap();
		Self { root }
	}

	fn command(&self, explicit: bool) -> Command {
		let mut cmd = Command::new(env!("CARGO_BIN_EXE_rnx"));
		cmd.current_dir(&*self.root)
			.env("XDG_CACHE_HOME", self.root.join("cache"))
			.env("GLIBC_TUNABLES", "glibc.rtld.nns=1")
			.env_remove("RNX_FILE_ROUTE_FD")
			.arg("run");
		if explicit {
			cmd.arg("--engine=legacy");
		}
		cmd.arg("test.rn");
		cmd
	}

	fn generation(&self) -> PathBuf {
		let entries = fs::read_dir(self.root.join("cache/rnx-file-images"))
			.unwrap()
			.map(|entry| entry.unwrap().path())
			.collect::<Vec<_>>();
		assert_eq!(entries.len(), 1, "{entries:?}");
		entries.into_iter().next().unwrap()
	}

	fn observed(&self, args: &[&OsStr]) -> (Output, Vec<serde_json::Value>) {
		// Reserve the loader's slot on entry. A route written to this fd
		// after exec proves the helper restored the inherited description.
		let file = self.root.join("routes");
		let out = Command::new("/bin/sh")
			.args(["-c", "exec 9>\"$1\"; shift; exec \"$@\"", "sh"])
			.arg(&file)
			.arg(env!("CARGO_BIN_EXE_rnx"))
			.arg("run")
			.args(args)
			.current_dir(&*self.root)
			.env("RNX_FILE_ROUTE_FD", "9")
			.env("XDG_CACHE_HOME", self.root.join("cache"))
			.output()
			.unwrap();
		let events = fs::read_to_string(file)
			.unwrap()
			.lines()
			.map(|line| serde_json::from_str(line).unwrap())
			.collect();
		(out, events)
	}
}

fn success(out: Output) {
	assert!(
		out.status.success(),
		"{}",
		String::from_utf8_lossy(&out.stderr)
	);
	assert_eq!(out.stdout, b"once\n42\n");
}

#[test]
fn simultaneous_cold_launch_and_warm_launch_use_one_generation_with_tight_tls() {
	let app = Installed::new("pub fn main(args){println!(\"once\");42}");
	let children = (0..4)
		.map(|_| {
			app.command(false)
				.stdout(Stdio::piped())
				.stderr(Stdio::piped())
				.spawn()
				.unwrap()
		})
		.collect::<Vec<_>>();
	for child in children {
		success(child.wait_with_output().unwrap());
	}
	let generation = app.generation();
	assert!(generation.join("engine").is_file());
	assert!(generation.join("helper").is_file());
	success(app.command(false).output().unwrap());
}

#[test]
fn installed_members_and_directory_are_checked_before_any_script_effect() {
	let app = Installed::new("pub fn main(args){println!(\"once\");42}");
	success(app.command(false).output().unwrap());
	let generation = app.generation();
	let refused = || {
		let out = app.command(false).output().unwrap();
		assert_eq!(out.status.code(), Some(1));
		assert!(out.stdout.is_empty());
		assert!(!out.stderr.is_empty());
	};
	for member in ["engine", "helper"] {
		let path = generation.join(member);
		let good = generation.join("saved");
		fs::rename(&path, &good).unwrap();
		symlink(&good, &path).unwrap();
		refused();
		fs::remove_file(&path).unwrap();
		fs::write(&path, b"truncated").unwrap();
		fs::set_permissions(&path, Permissions::from_mode(0o500)).unwrap();
		refused();
		let mut altered = fs::read(&good).unwrap();
		let last = altered.last_mut().unwrap();
		*last ^= 1;
		fs::remove_file(&path).unwrap();
		fs::write(&path, altered).unwrap();
		fs::set_permissions(&path, Permissions::from_mode(0o500)).unwrap();
		refused();
		fs::rename(&good, &path).unwrap();
	}
	fs::set_permissions(&generation, Permissions::from_mode(0o777)).unwrap();
	refused();
	success(app.command(true).output().unwrap());
}

#[test]
fn inherited_evidence_fd_distinguishes_routes_and_terminal_failures() {
	let app = Installed::new("pub fn main(args){println!(\"once\");42}");
	let path = Path::new("test.rn").as_os_str();
	let (out, events) = app.observed(&[path]);
	success(out);
	assert_eq!(events.len(), 1);
	assert_eq!(events[0]["route"], "execution");
	let (out, events) = app.observed(&[OsStr::new("--engine=legacy"), path]);
	success(out);
	assert_eq!(events.len(), 1);
	assert_eq!(events[0]["route"], "legacy_explicit");
	fs::write(
		app.root.join(path),
		"pub fn main(args){let b=b\"a\";println!(\"once\");42}",
	)
	.unwrap();
	let (out, events) = app.observed(&[path]);
	success(out);
	assert_eq!(events.len(), 1);
	assert_eq!(events[0]["route"], "legacy_refusal");
	assert!(
		events[0]["category"]
			.as_str()
			.is_some_and(|c| !c.is_empty())
	);
	assert!(events[0]["reason"].as_str().is_some_and(|c| !c.is_empty()));

	fs::write(
		app.root.join(path),
		"pub fn main(args){println!(\"once\");panic!(\"terminal\")}",
	)
	.unwrap();
	let (out, events) = app.observed(&[path]);
	assert_eq!(out.status.code(), Some(1));
	assert_eq!(out.stdout, b"once\n");
	assert_eq!(events.len(), 1);
	assert_eq!(events[0]["route"], "execution");
	let invalid = OsString::from_vec(vec![255]);
	for args in [vec![invalid.as_os_str()], vec![path, invalid.as_os_str()]] {
		let (out, events) = app.observed(&args);
		assert_eq!(out.status.code(), Some(2));
		assert!(events.is_empty());
	}
}
#[test]
fn changing_either_name_after_open_is_terminal_in_the_startup_helper() {
	let app = Installed::new("pub fn main(args){println!(\"once\");42}");
	success(app.command(false).output().unwrap());
	let dir = app.generation();
	for case in ["engine", "helper", "unsealed"] {
		// Plant the final's inherited bootstrap directory on fd 9. Bash
		// allocates the pinned fds; a saved redirection cannot reuse them.
		let out = Command::new("/bin/bash")
			.args([
				"-ec",
				r#"exec 9<"$1"
exec {directory}<"$1"
exec {helper}<"$1/helper"
exec 9<"$1/engine"
if [[ $2 == unsealed ]]; then
	# An ordinary file must be refused on the memory-file handoff.
	directory=-1
else
	# Both original members are pinned. Change only the cache name.
	mv -- "$1/$2" "$1/saved"
	cp -- "$1/saved" "$1/$2"
fi
exec "/proc/self/fd/$helper" "$directory,$helper,-1" run test.rn"#,
				"sh",
			])
			.arg(&dir)
			.arg(case)
			.current_dir(&*app.root)
			.output()
			.unwrap();
		let stderr = String::from_utf8_lossy(&out.stderr);
		assert_eq!(out.status.code(), Some(1), "{case}: {stderr}");
		assert!(out.stdout.is_empty(), "{case}: {out:?}");
		assert!(stderr.contains("mapping mismatch"), "{case}: {stderr}");
		if case != "unsealed" {
			fs::rename(dir.join("saved"), dir.join(case)).unwrap();
		}
	}
}

#[test]
fn cache_owned_by_another_uid_is_refused_before_execution() {
	let app = Installed::new("pub fn main(args){println!(\"once\");42}");
	let path = app.root.join("cache/rnx-file-images");
	fs::create_dir_all(&path).unwrap();
	let foreign = if unsafe { libc::geteuid() } == 0 {
		let dir = app.root.join("foreign");
		fs::create_dir(&dir).unwrap();
		std::os::unix::fs::chown(&dir, Some(65534), None).unwrap();
		dir
	} else {
		PathBuf::from("/usr")
	};
	let out = Command::new("/usr/bin/bwrap")
		.args(["--ro-bind", "/", "/", "--proc", "/proc", "--ro-bind"])
		.arg(foreign)
		.arg(path)
		.arg(env!("CARGO_BIN_EXE_rnx"))
		.args(["run", "test.rn"])
		.current_dir(&*app.root)
		.env("XDG_CACHE_HOME", app.root.join("cache"))
		.output()
		.unwrap();

	assert_eq!(out.status.code(), Some(1));
	assert!(out.stdout.is_empty());
	assert!(
		String::from_utf8_lossy(&out.stderr).contains("unsafe installed engine cache directory")
	);
}

#[test]
fn warm_verification_binds_inode_and_ctime_and_rechecks_a_stale_stamp() {
	use std::io::{Seek, SeekFrom, Write};
	use std::os::unix::fs::MetadataExt;

	let app = Installed::new("pub fn main(args){println!(\"once\");42}");
	success(app.command(false).output().unwrap());
	let dir = app.generation();
	let stamp = dir.join("verified");
	let before = fs::metadata(&stamp).unwrap().ino();
	success(app.command(false).output().unwrap());
	assert_eq!(fs::metadata(&stamp).unwrap().ino(), before);
	let image = dir.join("engine");
	let copy = dir.join("copy");
	fs::copy(&image, &copy).unwrap();
	fs::rename(copy, &image).unwrap();
	success(app.command(false).output().unwrap());
	assert_ne!(fs::metadata(&stamp).unwrap().ino(), before);
	fs::set_permissions(&image, Permissions::from_mode(0o600)).unwrap();
	let mut f = fs::OpenOptions::new().write(true).open(&image).unwrap();
	f.seek(SeekFrom::Start(0)).unwrap();
	f.write_all(b"broken").unwrap();
	drop(f);
	fs::set_permissions(image, Permissions::from_mode(0o400)).unwrap();
	let output = app.command(false).output().unwrap();
	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty());
	assert!(String::from_utf8_lossy(&output.stderr).contains("content mismatch"));
}

#[test]
fn sealed_image_runs_without_a_writable_cache_or_a_resolvable_home() {
	let app = Installed::new("pub fn main(args){println!(\"once\");42}");
	let base = app.root.join("readonly");
	fs::create_dir(&base).unwrap();
	fs::set_permissions(&base, Permissions::from_mode(0o555)).unwrap();
	// Simulate an NSS backend with no entry for the real uid. The actual
	// stock binary performs home discovery, materialization and exec.
	let shim_source = app.root.join("missing-passwd.c");
	let shim = app.root.join("missing-passwd.so");
	fs::write(
		&shim_source,
		r#"#include <pwd.h>
#include <stdlib.h>
#include <unistd.h>
#include <fcntl.h>
int getpwuid_r(uid_t uid, struct passwd *p, char *buf, size_t size, struct passwd **result) {
	(void)uid; (void)p; (void)buf; (void)size;
	int fd = open("nss-looked-up", O_WRONLY | O_CREAT, 0600);
	if (fd >= 0) close(fd);
	*result = NULL;
	return 0;
}
"#,
	)
	.unwrap();
	assert!(
		Command::new("cc")
			.args(["-shared", "-fPIC", "-o"])
			.arg(&shim)
			.arg(shim_source)
			.status()
			.unwrap()
			.success()
	);
	for xdg in [
		Some(if unsafe { libc::geteuid() } == 0 {
			PathBuf::from("/sys")
		} else {
			base.clone()
		}),
		Some(PathBuf::from("/proc/rnx-no-cache/base")),
		None,
	] {
		let mut cmd = Command::new("/bin/sh");
		cmd.args(["-ec", r#"exec 9>&2; exec "$@""#, "sh"])
			.arg(env!("CARGO_BIN_EXE_rnx"))
			.args(["run", "test.rn"])
			.env_remove("HOME")
			.env_remove("XDG_CACHE_HOME")
			.env("GLIBC_TUNABLES", "glibc.rtld.nns=1")
			.env("RNX_FILE_ROUTE_FD", "9")
			.current_dir(&*app.root);
		if let Some(path) = &xdg {
			cmd.env("XDG_CACHE_HOME", path);
		} else {
			cmd.env("LD_PRELOAD", &shim);
		}
		let out = cmd.output().unwrap();
		assert!(
			out.status.success(),
			"{xdg:?}: {}",
			String::from_utf8_lossy(&out.stderr)
		);
		let event: serde_json::Value = serde_json::from_slice(&out.stderr).unwrap();
		assert_eq!(event["route"], "execution");
		assert_eq!(event["storage"], "memfd");
		success(out);
	}
	assert!(app.root.join("nss-looked-up").is_file());
	fs::set_permissions(&base, Permissions::from_mode(0o700)).unwrap();
	assert!(!base.join("rnx-file-images").exists());
}

#[test]
fn publishing_retires_old_generations_while_a_mapped_script_finishes() {
	use std::io::{BufRead, BufReader, Read};
	let app = Installed::new(
		"pub fn main(args){println!(\"mapped\");io::stdin()?;println!(\"finished\");Ok(())}",
	);
	let mut child = app
		.command(false)
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.spawn()
		.unwrap();
	let mut stdout = BufReader::new(child.stdout.take().unwrap());
	let mut line = String::new();
	stdout.read_line(&mut line).unwrap();
	assert_eq!(line, "mapped\n");
	let live = app.generation();
	let root = live.parent().unwrap();
	let old = root.join(format!("{}-{}", "a".repeat(64), "b".repeat(64)));
	fs::rename(&live, &old).unwrap();
	// The launched process still maps these inodes, exactly as it would
	// across an upgrade. A second older generation and abandoned work
	// exercise both levels of the sweep.
	let other = root.join(format!("{}-{}", "c".repeat(64), "d".repeat(64)));
	fs::create_dir(&other).unwrap();
	fs::set_permissions(&other, Permissions::from_mode(0o700)).unwrap();
	fs::write(other.join(".verifying-12-0"), b"incomplete").unwrap();
	let partial = root.join(".creating-13-0");
	fs::create_dir(&partial).unwrap();
	fs::set_permissions(&partial, Permissions::from_mode(0o700)).unwrap();
	let sentinel = app.root.join("outside-cache");
	fs::write(&sentinel, "keep").unwrap();
	symlink(&sentinel, partial.join("engine")).unwrap();
	fs::write(root.join(".verifying-14-0"), b"partial").unwrap();
	fs::write(
		app.root.join("test.rn"),
		"pub fn main(args){println!(\"once\");42}",
	)
	.unwrap();
	success(app.command(false).output().unwrap());
	assert_eq!(app.generation(), live);
	assert_eq!(fs::read(sentinel).unwrap(), b"keep");
	drop(child.stdin.take());
	assert!(child.wait().unwrap().success());
	stdout.read_to_string(&mut line).unwrap();
	assert!(line.contains("finished\n"), "{line}");
}

#[test]
fn closed_stdout_reports_the_script_error() {
	let app = Installed::new(r#"pub fn main(args){println!("pipe control");42}"#);
	let run = |legacy| {
		let (reader, writer) = std::io::pipe().unwrap();
		drop(reader);
		app.command(legacy).stdout(writer).output().unwrap()
	};
	let installed = run(false);
	let legacy = run(true);
	assert_eq!(installed.status.code(), Some(1));
	assert!(String::from_utf8_lossy(&installed.stderr).contains("Broken pipe"));
	assert_eq!(installed, legacy);
}

#[test]
fn closed_child_stdin_preserves_the_childs_output_and_status() {
	let app = Installed::new(
		r#"pub fn main(args){
			let s = fs::read("input")?;
			let r = process::run_bytes("sh", ["-c", "exec 0<&-; printf closed; exit 7"],
				#{input: s.as_bytes(), timeout_ms: 10000})?;
			println!("child={:?} output={}", r.code, String::from_utf8(r.stdout)?);
			Ok(())
		}"#,
	);
	// The child never reads. With more than a pipeful of input, the writer
	// must encounter the closed reader regardless of which process runs first.
	fs::write(app.root.join("input"), vec![b'X'; 2 * 1024 * 1024]).unwrap();
	let installed = app.command(false).output().unwrap();
	let legacy = app.command(true).output().unwrap();
	assert_eq!(installed.status.code(), Some(0), "{installed:?}");
	assert_eq!(installed.stdout, b"child=7 output=closed\n");
	assert!(installed.stderr.is_empty());
	assert_eq!(installed, legacy);
}
