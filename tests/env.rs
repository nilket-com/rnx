//! Record 0036. Children receive explicit arguments and controlled environments.
use std::{
	path::PathBuf,
	process::{Command, Output, Stdio},
	sync::atomic::{AtomicUsize, Ordering},
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
	fn new() -> Self {
		let path = std::env::temp_dir().join(format!(
			"rnx-env-{}-{}",
			std::process::id(),
			NEXT.fetch_add(1, Ordering::Relaxed)
		));
		std::fs::create_dir(&path).unwrap();
		Self(path)
	}
	fn command(&self) -> Command {
		let mut c = Command::new(env!("CARGO_BIN_EXE_rnx"));
		c.env_clear().current_dir(&self.0);
		c
	}
	fn eval(&self, source: &str) -> Command {
		let mut c = self.command();
		c.args(["eval", source]);
		c
	}
}
impl Drop for Fixture {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.0);
	}
}
fn output(c: &mut Command) -> String {
	let o = c.output().unwrap();
	assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
	String::from_utf8(o.stdout).unwrap()
}
fn yes(c: &mut Command) {
	assert_eq!(output(c).trim(), "true");
}
#[test]
fn arguments_agree_and_never_share_mutable_storage() {
	let f = Fixture::new();
	let script = f.0.join("args.rn");
	std::fs::write(&script, "pub fn main(a) { a == env::args() }").unwrap();
	for args in [
		vec![],
		vec!["one"],
		vec!["two words", "--flag", "run"],
		vec![""],
	] {
		yes(f.command().arg("run").arg(&script).args(args));
	}
	std::fs::write(
		&script,
		r#"pub fn main(a) {
        a[0].push('!');
        let first = env::args();
        first[0].push('?');
        a.pop(); first.pop();
        env::args() == ["original", "second"]
    }"#,
	)
	.unwrap();
	yes(f
		.command()
		.args(["run", "--budget", "10000"])
		.arg(&script)
		.args(["original", "second"]));
	yes(&mut f.eval("env::args() == []"));
	let mut child = f
		.command()
		.env("TERM", "dumb")
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();
	use std::io::Write;
	child.stdin.take().unwrap().write_all(b"env::args()\nlet a = env::args(); a.push(\"x\");\n:reset\nenv::args()\n:help env::args\n:quit\n").unwrap();
	let o = child.wait_with_output().unwrap();
	assert!(o.status.success());
	let s = String::from_utf8_lossy(&o.stdout);
	assert!(
		s.matches("[]").count() >= 2 && s.contains("independent copies"),
		"{s}"
	);
}
#[test]
fn environment_distinguishes_missing_empty_and_invalid_names() {
	let f = Fixture::new();
	yes(f
		.eval(
			r#"env::var("A")? == Some("1") && env::var("E")? == Some("") && env::var("M")? == None"#,
		)
		.env("A", "1")
		.env("E", ""));
	yes(f
		.eval(r#"json::stringify(env::vars()?)? == json::stringify(#{A:"1", E:""})?"#)
		.env("A", "1")
		.env("E", ""));
	for name in ["", "A=B", "A\0"] {
		let literal = serde_json::to_string(name)
			.unwrap()
			.replace("\\u0000", "\\0");
		let s = output(&mut f.eval(&format!(
			"match env::var({literal}) {{ Ok(_) => panic!(\"expected error\"), Err(e) => e }}"
		)));
		assert!(
			s.contains("name must be nonempty") && s.contains("cannot read environment variable"),
			"{s}"
		);
	}
	#[cfg(windows)]
	yes(f.eval(r#"env::var("a")? == Some("1")"#).env("A", "1"));
}
#[test]
fn home_directory_uses_explicit_platform_variable() {
	let f = Fixture::new();
	#[cfg(windows)]
	let variable = "USERPROFILE";
	#[cfg(not(windows))]
	let variable = "HOME";
	let expected = serde_json::to_string(f.0.to_str().unwrap()).unwrap();
	yes(f
		.eval(&format!("env::home_dir()? == Some({expected})"))
		.env(variable, &f.0));
}
#[test]
fn module_is_read_only_and_ceiling_still_applies() {
	let f = Fixture::new();
	for name in ["set_var", "remove_var", "set"] {
		let o = f.eval(&format!("env::{name}")).output().unwrap();
		assert!(!o.status.success());
	}
	let mut child = f
		.command()
		.env("TERM", "dumb")
		.env("RNX_MEMORY_CEILING", "1")
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();
	use std::io::Write;
	child
		.stdin
		.take()
		.unwrap()
		.write_all(b":memory\n42\n:quit\n")
		.unwrap();
	let o = child.wait_with_output().unwrap();
	assert!(o.status.success());
	let text = format!(
		"{}{}",
		String::from_utf8_lossy(&o.stdout),
		String::from_utf8_lossy(&o.stderr)
	);
	assert!(
		text.contains("ceiling of 1")
			&& text.contains("are at or above the ceiling of 1; :reset to continue"),
		"{text}"
	);
	yes(f
		.eval(r#"env::var("RNX_MEMORY_CEILING")? == Some("536870912")"#)
		.env("RNX_MEMORY_CEILING", "536870912"));
}
#[cfg(unix)]
mod unix {
	use super::*;
	use std::{
		ffi::{CString, OsString},
		os::unix::{ffi::OsStringExt, process::CommandExt},
	};
	fn refused(o: Output, position: usize) {
		assert_eq!(o.status.code(), Some(2));
		assert!(o.stdout.is_empty());
		assert_eq!(
			String::from_utf8(o.stderr).unwrap(),
			format!("rnx: argument {position} is not Unicode: \"\\xFF\\xFE\"\n")
		);
	}
	#[test]
	fn every_bad_argument_is_refused_before_dispatch_but_arg_zero_is_not_decoded() {
		let f = Fixture::new();
		let bad = || OsString::from_vec(vec![255, 254]);
		refused(f.command().arg(bad()).output().unwrap(), 1);
		refused(f.command().arg("run").arg(bad()).output().unwrap(), 2);
		refused(
			f.command()
				.args(["run", "not-read.rn"])
				.arg(bad())
				.output()
				.unwrap(),
			3,
		);
		for command in ["eval", "repl", "help", "version", "unknown"] {
			refused(f.command().arg(command).arg(bad()).output().unwrap(), 2);
		}
		let o = f.command().arg0(bad()).arg("version").output().unwrap();
		assert!(o.status.success());
		assert!(o.stderr.is_empty());
	}
	#[test]
	fn nonunicode_values_are_refused_without_poisoning_other_lookups() {
		let f = Fixture::new();
		for expr in [r#"env::var("B")"#, "env::vars()"] {
			let s = output(
				f.eval(&format!(
					"match {expr} {{ Ok(_) => panic!(\"expected refusal\"), Err(e) => e }}"
				))
				.env("B", OsString::from_vec(vec![255])),
			);
			assert!(s.contains("B") && s.contains("xFF"), "{s}");
		}
		yes(f
			.eval(r#"env::var("A")? == Some("1")"#)
			.env("A", "1")
			.env("B", OsString::from_vec(vec![255])));
		let s = output(
			f.eval(r#"match env::home_dir() { Ok(_) => panic!("expected refusal"), Err(e) => e }"#)
				.env("HOME", OsString::from_vec(vec![255])),
		);
		assert!(s.contains("HOME") && s.contains("xFF"), "{s}");
	}
	/// Allocate every string before fork. The pre-exec hook only assembles
	/// fixed-size pointer arrays and calls execve; no allocator or env mutation.
	fn raw_environment(f: &Fixture, source: &str, entries: &[&[u8]]) -> Output {
		assert!(entries.len() < 8);
		let program = CString::new(env!("CARGO_BIN_EXE_rnx")).unwrap();
		let args = [
			program.clone(),
			CString::new("eval").unwrap(),
			CString::new(source).unwrap(),
		];
		let entries: Vec<CString> = entries.iter().map(|e| CString::new(*e).unwrap()).collect();
		let mut c = f.command();
		unsafe {
			c.pre_exec(move || {
				let argv = [
					args[0].as_ptr(),
					args[1].as_ptr(),
					args[2].as_ptr(),
					std::ptr::null(),
				];
				let mut envp = [std::ptr::null(); 8];
				for (slot, entry) in envp.iter_mut().zip(&entries) {
					*slot = entry.as_ptr();
				}
				libc::execve(program.as_ptr(), argv.as_ptr(), envp.as_ptr());
				Err(std::io::Error::last_os_error())
			});
		}
		c.output().unwrap()
	}
	#[test]
	fn duplicate_entries_and_bad_names_are_not_hidden_by_a_map() {
		let f = Fixture::new();
		let o = raw_environment(
			&f,
			r#"env::var("D")? == Some("first") && env::vars()?.D == "first""#,
			&[b"D=first", b"D=second"],
		);
		assert!(o.status.success());
		assert_eq!(String::from_utf8_lossy(&o.stdout).trim(), "true");
		let o = raw_environment(
			&f,
			r#"env::var("D")? == Some("first") && match env::vars() { Ok(_) => false, Err(e) => e.contains("D") && e.contains("xFF") }"#,
			&[b"D=first", b"D=\xff"],
		);
		assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
		assert_eq!(String::from_utf8_lossy(&o.stdout).trim(), "true");
		let o = raw_environment(
			&f,
			r#"match env::vars() { Ok(_) => false, Err(e) => e.contains("name") && e.contains("xFF") }"#,
			&[b"\xff=value"],
		);
		assert!(o.status.success());
		assert_eq!(String::from_utf8_lossy(&o.stdout).trim(), "true");
	}
	fn account_home() -> Option<OsString> {
		let mut storage = vec![0u8; 16384];
		loop {
			let mut pwd = std::mem::MaybeUninit::<libc::passwd>::uninit();
			let mut result = std::ptr::null_mut();
			let rc = unsafe {
				libc::getpwuid_r(
					libc::getuid(),
					pwd.as_mut_ptr(),
					storage.as_mut_ptr().cast(),
					storage.len(),
					&mut result,
				)
			};
			if rc == libc::ERANGE {
				storage.resize(storage.len() * 2, 0);
				continue;
			}
			if rc != 0 || result.is_null() {
				return None;
			}
			// SAFETY: getpwuid_r succeeded and its strings live in storage.
			return unsafe {
				let pwd = pwd.assume_init();
				if pwd.pw_dir.is_null() {
					None
				} else {
					Some(OsString::from_vec(
						std::ffi::CStr::from_ptr(pwd.pw_dir).to_bytes().to_vec(),
					))
				}
			};
		}
	}
	#[test]
	fn empty_home_uses_account_database_and_unset_is_observed() {
		let f = Fixture::new();
		match account_home().map(OsString::into_string) {
			None => yes(f.eval("env::home_dir()? == None").env("HOME", "")),
			Some(Ok(path)) => yes(f
				.eval(&format!(
					"env::home_dir()? == Some({})",
					serde_json::to_string(&path).unwrap()
				))
				.env("HOME", "")),
			Some(Err(_)) => yes(f
				.eval("match env::home_dir() { Ok(_) => false, Err(_) => true }")
				.env("HOME", "")),
		}
		let observed = output(&mut f.eval("env::home_dir()"));
		println!("unset HOME: {}", observed.trim());
	}
}
