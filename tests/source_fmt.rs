use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Dir(std::path::PathBuf);
impl Dir {
	fn new() -> Self {
		let p = std::env::temp_dir().join(format!(
			"rnx-fmt-test-{}-{}",
			std::process::id(),
			NEXT.fetch_add(1, Ordering::Relaxed)
		));
		fs::create_dir(&p).unwrap();
		Self(p)
	}
}
impl Drop for Dir {
	fn drop(&mut self) {
		let _ = fs::remove_dir_all(&self.0);
	}
}
fn command() -> Command {
	let mut c = Command::new(env!("CARGO_BIN_EXE_rnx"));
	c.arg("fmt");
	c
}
#[test]
#[cfg(target_os = "linux")]
fn files_check_and_stdin_are_bounded_and_preserve_source() {
	let d = Dir::new();
	let p = d.0.join("dense.rn");
	let dense = "fn a(){let s=\"first\n    second\";println!(\"{}\",s);}fn b(){a();}\n";
	fs::write(&p, dense).unwrap();
	let check = command().arg("--check").arg(&p).output().unwrap();
	assert_eq!(check.status.code(), Some(1));
	assert_eq!(fs::read_to_string(&p).unwrap(), dense);
	assert!(command().arg(&p).status().unwrap().success());
	let formatted = fs::read_to_string(&p).unwrap();
	assert!(formatted.contains("\n\tlet s"));
	assert!(formatted.contains("}\n\nfn b"));
	assert!(formatted.contains("first\n    second"));
	assert!(command().arg("--check").arg(&p).status().unwrap().success());
	let mut child = command()
		.arg("--stdin")
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();
	child
		.stdin
		.take()
		.unwrap()
		.write_all(dense.as_bytes())
		.unwrap();
	let out = child.wait_with_output().unwrap();
	assert!(out.status.success());
	assert!(out.stderr.is_empty());
	assert_eq!(out.stdout, formatted.as_bytes());
}
#[test]
#[cfg(target_os = "linux")]
fn later_invalid_file_never_changes_earlier_file() {
	let d = Dir::new();
	let a = d.0.join("a.rn");
	let b = d.0.join("b.rn");
	fs::write(&a, "fn a(){let x=1;}").unwrap();
	fs::write(&b, "fn b(){println!(\"x\",").unwrap();
	let before = fs::read(&a).unwrap();
	let result = command().arg(&a).arg(&b).output().unwrap();
	assert_eq!(result.status.code(), Some(2));
	assert_eq!(fs::read(&a).unwrap(), before);
	assert_eq!(fs::read_dir(&d.0).unwrap().count(), 2);
}
#[test]
#[cfg(target_os = "linux")]
fn refused_inputs_and_aliases_leave_files_unchanged() {
	use std::os::unix::fs::{PermissionsExt, symlink};
	let d = Dir::new();
	let p = d.0.join("a.rn");
	fs::write(&p, "fn a(){}").unwrap();
	fs::set_permissions(&p, fs::Permissions::from_mode(0o640)).unwrap();
	assert_eq!(command().arg(&p).arg(&p).status().unwrap().code(), Some(2));
	let link = d.0.join("link.rn");
	symlink(&p, &link).unwrap();
	assert_eq!(command().arg(&link).status().unwrap().code(), Some(2));
	let hard = d.0.join("hard.rn");
	fs::hard_link(&p, &hard).unwrap();
	assert_eq!(command().arg(&p).status().unwrap().code(), Some(2));
	fs::remove_file(&hard).unwrap();
	assert!(command().arg(&p).status().unwrap().success());
	assert_eq!(
		fs::metadata(&p).unwrap().permissions().mode() & 0o777,
		0o640
	);
	let bad = d.0.join("bad.rn");
	fs::write(&bad, [0xff]).unwrap();
	assert_eq!(command().arg(&bad).status().unwrap().code(), Some(2));
	fs::write(&bad, vec![b' '; 1024 * 1024 + 1]).unwrap();
	assert_eq!(command().arg(&bad).status().unwrap().code(), Some(2));
	for args in [
		vec![],
		vec!["--stdin", "--check"],
		vec!["--check", "--check"],
		vec!["--unknown"],
	] {
		assert_eq!(command().args(args).status().unwrap().code(), Some(2));
	}
}

#[test]
#[cfg(all(target_os = "linux", feature = "test-support"))]
fn worker_controls_are_lean_and_fail_closed() {
	let bin = env!("CARGO_BIN_EXE_rnx");
	let result = Command::new(bin)
		.args(["--rnx-fmt-test", "lean"])
		.output()
		.unwrap();
	assert!(result.status.success());
	let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
	assert_eq!(report["threads"], 1);
	assert_eq!(report["contexts"], 0);
	for mode in ["panic", "overflow", "timeout", "malformed"] {
		let start = std::time::Instant::now();
		let result = Command::new(bin)
			.args(["--rnx-fmt-test", mode])
			.output()
			.unwrap();
		assert_eq!(result.status.code(), Some(2), "{mode}");
		assert!(start.elapsed() < std::time::Duration::from_secs(7));
		assert!(result.stdout.is_empty());
		if mode == "timeout" {
			assert!(String::from_utf8_lossy(&result.stderr).contains("killed and reaped"));
		}
	}
}

#[test]
#[cfg(all(target_os = "linux", feature = "test-support"))]
fn publication_failures_are_honest_and_stages_are_cleaned() {
	let d = Dir::new();
	let a = d.0.join("a.rn");
	let b = d.0.join("b.rn");
	let before = "fn a(){let x=1;}";
	for p in [&a, &b] {
		fs::write(p, before).unwrap();
	}
	let result = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.args(["--rnx-fmt-test", "fail-publish"])
		.arg(&a)
		.arg(&b)
		.output()
		.unwrap();
	assert_eq!(result.status.code(), Some(2));
	assert_ne!(fs::read_to_string(&a).unwrap(), before);
	assert_eq!(fs::read_to_string(&b).unwrap(), before);
	assert!(String::from_utf8_lossy(&result.stdout).contains("formatted:"));
	assert!(String::from_utf8_lossy(&result.stderr).contains("after 1 replacements"));
	assert_eq!(fs::read_dir(&d.0).unwrap().count(), 2);
	fs::write(&a, before).unwrap();
	let result = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.args(["--rnx-fmt-test", "edit-target"])
		.arg(&a)
		.arg(&b)
		.output()
		.unwrap();
	assert_eq!(result.status.code(), Some(2));
	assert_eq!(
		fs::read_to_string(&a).unwrap(),
		format!("{before}\n// concurrent edit\n")
	);
	assert_eq!(fs::read_to_string(&b).unwrap(), before);
	assert_eq!(fs::read_dir(&d.0).unwrap().count(), 2);
}

#[test]
#[cfg(target_os = "linux")]
fn fifo_directory_dash_paths_and_unchanged_inode() {
	use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};
	let d = Dir::new();
	let fifo = d.0.join("fifo.rn");
	let c = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
	assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
	let began = std::time::Instant::now();
	assert_eq!(command().arg(&fifo).status().unwrap().code(), Some(2));
	assert!(began.elapsed() < std::time::Duration::from_secs(1));
	assert_eq!(command().arg(&d.0).status().unwrap().code(), Some(2));
	let dash = d.0.join("--check");
	fs::write(&dash, "fn a(){let x=1;}").unwrap();
	let result = command()
		.current_dir(&d.0)
		.args(["--", "--check"])
		.status()
		.unwrap();
	assert!(result.success());
	let ino = fs::metadata(&dash).unwrap().ino();
	assert!(command().arg(&dash).status().unwrap().success());
	assert_eq!(ino, fs::metadata(&dash).unwrap().ino());
}

#[test]
#[cfg(target_os = "linux")]
fn collection_caps_precede_any_formatter_attempt() {
	let d = Dir::new();
	let paths: Vec<_> = (0..17).map(|i| d.0.join(format!("{i}.rn"))).collect();
	for p in &paths {
		fs::File::create(p).unwrap().set_len(1024 * 1024).unwrap();
	}
	let result = command().arg("--check").args(&paths).output().unwrap();
	assert_eq!(result.status.code(), Some(2));
	assert!(String::from_utf8_lossy(&result.stderr).contains("total input exceeds"));
	let result = command()
		.args(std::iter::repeat_n("missing.rn", 129))
		.output()
		.unwrap();
	assert_eq!(result.status.code(), Some(2));
	assert!(String::from_utf8_lossy(&result.stderr).contains("1 to 128"));
}

#[test]
#[cfg(target_os = "linux")]
fn known_upstream_corruptions_are_refused_before_publication() {
	let d = Dir::new();
	for (i, source) in [
		"pub fn main(){let x=`a { b`;}",
		"pub fn main(){let x=`a } b`;}",
		"pub fn main(){let x=`a \\` b`;}",
		"a\nb",
		"pub fn main(){let x=a\n// c\nb;}",
		"# !",
	]
	.into_iter()
	.enumerate()
	{
		let path = d.0.join(format!("corruption-{i}.rn"));
		fs::write(&path, source).unwrap();
		let result = command().arg(&path).output().unwrap();
		assert_eq!(result.status.code(), Some(2));
		assert!(result.stdout.is_empty());
		assert!(String::from_utf8_lossy(&result.stderr).contains("formatter worker"));
		assert_eq!(fs::read_to_string(path).unwrap(), source);
	}
	assert_eq!(fs::read_dir(&d.0).unwrap().count(), 6);
}
