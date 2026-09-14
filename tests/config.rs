//! Record 0043: isolated config discovery, validation, and presentation.
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::Duration;
mod harness;
fn run(path: &Path, args: &[&str], input: &str, env: &[(&str, &str)]) -> Output {
	let dir = harness::scratch("config-run");
	let mut c = Command::new(env!("CARGO_BIN_EXE_rnx"));
	c.args(args)
		.env("RNX_CONFIG", path)
		.env("RNX_HISTORY", dir.join("history"))
		.env("TERM", "xterm")
		.env_remove("NO_COLOR");
	for key in [
		"XDG_CONFIG_HOME",
		"HOME",
		"APPDATA",
		"LOCALAPPDATA",
		"RNX_TEST_CONFIG_READS",
	] {
		c.env_remove(key);
	}
	for (k, v) in env {
		c.env(k, v);
	}
	let mut child = c
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();
	child
		.stdin
		.take()
		.unwrap()
		.write_all(input.as_bytes())
		.unwrap();
	let mut stdout = child.stdout.take().unwrap();
	let mut stderr = child.stderr.take().unwrap();
	let out = std::thread::spawn(move || {
		let mut b = Vec::new();
		stdout.read_to_end(&mut b).unwrap();
		b
	});
	let err = std::thread::spawn(move || {
		let mut b = Vec::new();
		stderr.read_to_end(&mut b).unwrap();
		b
	});
	let ended = harness::reaped_within(&mut child, Duration::from_secs(10));
	if ended.is_none() {
		let _ = child.kill();
		let _ = child.wait();
	}
	let status = child.wait().unwrap();
	let stdout = out.join().unwrap();
	let stderr = err.join().unwrap();
	std::fs::remove_dir_all(dir).unwrap();
	assert!(ended.is_some(), "config process did not finish");
	Output {
		status,
		stdout,
		stderr,
	}
}
fn fixture(text: &str) -> std::path::PathBuf {
	let dir = harness::scratch("config");
	let file = dir.join("config.rn");
	std::fs::write(&file, text).unwrap();
	file
}
fn done(path: &Path) {
	std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}
#[test]
fn missing_blank_and_empty_settings_are_silent() {
	let p = fixture("");
	for source in ["", " \n\t", "#{}"] {
		std::fs::write(&p, source).unwrap();
		let out = run(&p, &[], ":q\n", &[]);
		assert!(out.status.success());
		assert!(out.stderr.is_empty(), "{:?}", out);
	}
	std::fs::remove_file(&p).unwrap();
	let out = run(&p, &[], ":q\n", &[]);
	assert!(out.stderr.is_empty());
	let out = run(p.parent().unwrap(), &[], ":q\n", &[]);
	assert!(out.status.success());
	assert!(
		String::from_utf8(out.stderr)
			.unwrap()
			.contains("not a regular file")
	);
	done(&p);
}
#[test]
fn content_failures_warn_once_and_reach_the_prompt_without_io() {
	for source in [
		"let x = ;",
		"panic!(\"failure\")",
		"loop {}",
		"42",
		"()",
		"fs::read(\"x\")",
		"http::get(\"http://localhost\")",
		"println!(\"CONFIG OUTPUT\")",
		"dbg!(1)",
		"mod external; #{}",
		"async { 1 }.await",
	] {
		let p = fixture(source);
		let out = run(&p, &[], ":q\n", &[]);
		assert!(out.status.success(), "{source}: {out:?}");
		let err = String::from_utf8(out.stderr).unwrap();
		assert_eq!(err.matches("rnx: config `").count(), 1, "{source}: {err}");
		assert!(err.contains(p.to_str().unwrap()));
		assert_eq!(
			out.stdout, b"rnx: a Rune session. :help lists the commands, :quit ends it.\n",
			"{source}"
		);
		done(&p);
	}
	let p = fixture(&" ".repeat(65537));
	let out = run(&p, &[], ":q\n", &[]);
	assert!(String::from_utf8(out.stderr).unwrap().contains("64 KiB"));
	done(&p);
}
#[test]
fn computed_settings_keep_valid_neighbours_and_warn_in_key_order() {
	let p = fixture(
		"fn choose() { \"#123456\" } #{ splash: false, color: \"always\", palette: #{ number: choose(), string: \"bad\" }, zebra: 1 }",
	);
	let out = run(&p, &[], "42\n:q\n", &[]);
	assert!(out.status.success());
	assert_eq!(out.stdout, b"\x1b[38;2;18;52;86m42\x1b[0m\n");
	let err = String::from_utf8(out.stderr).unwrap();
	assert_eq!(err.matches("rnx: config").count(), 2);
	assert!(err.find("palette.string").unwrap() < err.find("zebra").unwrap());
	done(&p);
}
#[test]
fn palette_foregrounds_preserve_attributes_and_plain_text() {
	let p = fixture(
		"#{color: \"always\", palette: #{number: \"#123456\", string: \"bright-magenta\", error: \"#00ff00\"}}",
	);
	for source in ["[42, \"\\u{1b}[2J\", \"é界\\t\"]", "let 界 = ;"] {
		let input = format!("{source}\n:q\n");
		let coloured = run(&p, &[], &input, &[]);
		let plain = run(&p, &["--color=never"], &input, &[]);
		let strip = |bytes: Vec<u8>| {
			let mut t = String::from_utf8(bytes).unwrap();
			for s in [
				"\x1b[38;2;18;52;86m",
				"\x1b[95m",
				"\x1b[1;38;2;0;255;0m",
				"\x1b[38;2;0;255;0m",
				"\x1b[0m",
			] {
				t = t.replace(s, "");
			}
			assert!(!t.contains('\x1b'), "{t:?}");
			t
		};
		assert_eq!(strip(coloured.stdout).as_bytes(), plain.stdout);
		assert_eq!(strip(coloured.stderr).as_bytes(), plain.stderr);
	}
	let out = run(&p, &[], "1.missing()\n:q\n", &[]);
	assert!(
		String::from_utf8(out.stderr)
			.unwrap()
			.starts_with("\x1b[1;38;2;0;255;0m")
	);
	done(&p);
}
#[test]
fn cli_overrides_saved_mode_and_splash_and_no_color_still_controls_auto() {
	let p = fixture("#{color: \"always\", splash: true}");
	let out = run(&p, &["--color=never", "--no-splash"], "42\n:q\n", &[]);
	assert_eq!(out.stdout, b"42\n");
	std::fs::write(&p, "#{color: \"auto\", splash: false}").unwrap();
	let out = run(&p, &[], "42\n:q\n", &[("NO_COLOR", "1")]);
	assert_eq!(out.stdout, b"42\n");
	let out = run(&p, &["--color=always"], "42\n:q\n", &[("NO_COLOR", "1")]);
	assert!(out.stdout.contains(&27));
	done(&p);
}
#[test]
fn discovery_uses_only_the_selected_absolute_location() {
	let p = fixture("#{splash:false}");
	let out = run(&p, &[], ":q\n", &[]);
	assert!(out.stdout.is_empty());
	let out = run(Path::new("relative.rn"), &[], ":q\n", &[]);
	assert!(
		String::from_utf8(out.stderr)
			.unwrap()
			.contains("absolute path")
	);
	#[cfg(unix)]
	{
		let base = p.parent().unwrap();
		std::fs::create_dir(base.join("rnx")).unwrap();
		std::fs::copy(&p, base.join("rnx/config.rn")).unwrap();
		let out = run(
			Path::new(""),
			&[],
			":q\n",
			&[("XDG_CONFIG_HOME", base.to_str().unwrap())],
		);
		assert!(out.stdout.is_empty());
		assert!(out.stderr.is_empty());
		std::fs::create_dir_all(base.join(".config/rnx")).unwrap();
		std::fs::copy(&p, base.join(".config/rnx/config.rn")).unwrap();
		let out = run(
			Path::new(""),
			&[],
			":q\n",
			&[
				("XDG_CONFIG_HOME", "relative"),
				("HOME", base.to_str().unwrap()),
			],
		);
		assert!(out.stdout.is_empty());
		assert!(out.stderr.is_empty());
	}
	done(&p);
}
#[cfg(unix)]
#[test]
fn fifo_config_is_refused_without_waiting_for_a_writer() {
	use std::os::unix::ffi::OsStrExt;
	let p = fixture("");
	std::fs::remove_file(&p).unwrap();
	let name = std::ffi::CString::new(p.as_os_str().as_bytes()).unwrap();
	assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
	let out = run(&p, &[], ":q\n", &[]);
	assert!(out.status.success());
	assert!(String::from_utf8(out.stderr).unwrap().contains("FIFO"));
	done(&p);
}
#[cfg(feature = "test-support")]
#[test]
fn only_session_entry_points_attempt_to_open_config() {
	let p = fixture("#{splash:false}");
	let count = p.with_extension("count");
	let script = p.with_extension("script.rn");
	std::fs::write(&script, "pub fn main(_) { 42 }").unwrap();
	for (args, input, expected) in [
		(&["version"][..], "", "0"),
		(&["help"][..], "", "0"),
		(&[][..], ":q\n", "1"),
		(&["eval", "1"][..], "", "0"),
		(&["run", script.to_str().unwrap()][..], "", "0"),
		(&["repl"][..], ":q\n", "1"),
	] {
		let out = run(
			&p,
			args,
			input,
			&[("RNX_TEST_CONFIG_READS", count.to_str().unwrap())],
		);
		assert!(out.status.success());
		assert_eq!(std::fs::read_to_string(&count).unwrap(), expected);
	}
	done(&p);
}
#[test]
fn one_shot_commands_ignore_valid_invalid_and_missing_config() {
	let p = fixture("#{}");
	let missing = p.with_extension("absent");
	let script = p.with_extension("script.rn");
	std::fs::write(&script, "pub fn main(_) { 1.missing() }").unwrap();
	for args in [
		vec!["run", script.to_str().unwrap()],
		vec!["eval", "1.missing()"],
		vec!["version"],
		vec!["help"],
	] {
		for flag in ["--color=auto", "--color=always", "--color=never"] {
			let mut command = vec![flag];
			command.extend_from_slice(&args);
			let reference = run(&missing, &command, "", &[]);
			for source in [
				"#{color: \"always\", splash:false,palette:#{error:\"#123456\"}}",
				"panic!(\"config must not execute\")",
				"loop {}",
			] {
				std::fs::write(&p, source).unwrap();
				let actual = run(&p, &command, "", &[]);
				assert_eq!(actual.status.code(), reference.status.code());
				assert_eq!(actual.stdout, reference.stdout);
				assert_eq!(actual.stderr, reference.stderr);
			}
		}
	}
	done(&p);
}
#[cfg(unix)]
#[test]
fn non_unicode_override_warns_and_permission_refusal_recovers() {
	use std::os::unix::ffi::OsStringExt;
	use std::os::unix::fs::PermissionsExt;
	let p = fixture("#{}");
	let invalid = std::path::PathBuf::from(std::ffi::OsString::from_vec(vec![b'/', 255]));
	let out = run(&invalid, &[], ":q\n", &[]);
	assert!(out.status.success());
	assert!(
		String::from_utf8(out.stderr)
			.unwrap()
			.contains("not Unicode")
	);
	if unsafe { libc::geteuid() } != 0 {
		std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0)).unwrap();
		let out = run(&p, &[], ":q\n", &[]);
		assert!(out.status.success());
		assert_eq!(
			String::from_utf8(out.stderr)
				.unwrap()
				.matches("rnx: config")
				.count(),
			1
		);
		std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).unwrap();
	}
	done(&p);
}
#[cfg(windows)]
#[test]
fn windows_discovery_uses_appdata_then_localappdata() {
	let p = fixture("#{splash:false}");
	let base = p.parent().unwrap();
	std::fs::create_dir(base.join("rnx")).unwrap();
	std::fs::copy(&p, base.join("rnx/config.rn")).unwrap();
	for vars in [
		vec![("APPDATA", base.to_str().unwrap())],
		vec![("APPDATA", ""), ("LOCALAPPDATA", base.to_str().unwrap())],
	] {
		let out = run(Path::new(""), &[], ":q\n", &vars);
		assert!(out.stdout.is_empty());
		assert!(out.stderr.is_empty());
	}
	done(&p);
}

#[test]
fn marker_palette_keys_accept_colours_and_refuse_invalid_values() {
	let config = fixture("#{palette:#{result_number: \"bright-blue\",prompt_frame: \"#777777\"}}");
	let out = run(&config, &["repl"], ":q\n", &[]);
	assert!(out.status.success());
	assert!(out.stderr.is_empty(), "{:?}", out.stderr);
	let config = fixture("#{palette:#{result_number: \"not-blue\",prompt_frame: 42}}");
	let out = run(&config, &["repl"], ":q\n", &[]);
	assert!(out.status.success());
	let err = String::from_utf8_lossy(&out.stderr);
	assert!(
		err.contains("palette.result_number") && err.contains("palette.prompt_frame"),
		"{err}"
	);
}
