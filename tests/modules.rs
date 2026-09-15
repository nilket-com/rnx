//! Record 0050: file modules share a root and an allowance, not source offsets.
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
mod harness;

struct App(PathBuf);
impl App {
	fn new() -> Self {
		Self(harness::scratch("modules"))
	}
	fn write(&self, path: &str, text: impl AsRef<[u8]>) {
		let path = self.0.join(path);
		std::fs::create_dir_all(path.parent().unwrap()).unwrap();
		std::fs::write(path, text).unwrap();
	}
	fn run(&self, debug: bool) -> Output {
		let mut command = command();
		command.arg("run");
		if debug {
			command.arg("--debug-source");
		}
		command.arg(self.0.join("main.rn")).output().unwrap()
	}
}
impl Drop for App {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.0);
	}
}
fn command() -> Command {
	let mut c = Command::new(env!("CARGO_BIN_EXE_rnx"));
	c.env("TERM", "xterm")
		.env("RNX_CONFIG", "")
		.env("NO_COLOR", "1");
	c
}
fn stderr(out: &Output) -> String {
	String::from_utf8(out.stderr.clone()).unwrap()
}

#[test]
fn mixed_layout_uses_the_entry_root_from_either_working_directory() {
	let root = Path::new(env!("CARGO_MANIFEST_DIR"));
	let directory = root.join("tests/fixtures/modules/mixed");
	for (cwd, entry) in [
		(directory.clone(), PathBuf::from("main.rn")),
		(
			root.to_owned(),
			PathBuf::from("tests/fixtures/modules/mixed/main.rn"),
		),
		(std::env::temp_dir(), directory.join("main.rn")),
	] {
		let out = command()
			.current_dir(cwd)
			.arg("run")
			.arg(entry)
			.output()
			.unwrap();
		assert!(out.status.success(), "{}", stderr(&out));
		assert_eq!(out.stdout, b"21111\n");
		assert!(out.stderr.is_empty());
	}
}

#[test]
fn nested_compile_and_runtime_errors_keep_their_source_and_caret() {
	let app = App::new();
	app.write("main.rn", "mod a;\npub fn main(_) { a::b::fail() }\n");
	app.write("a.rn", "pub mod b;\n");
	app.write("a/b.rn", "pub fn fail() {\n    let x = ;\n}\n");
	let out = app.run(false);
	assert_eq!(out.status.code(), Some(1));
	let error = stderr(&out);
	assert!(
		error.starts_with(&format!(
			"error at {}, line 2, column 13:",
			app.0.join("a/b.rn").display()
		)),
		"{error}"
	);
	assert!(
		error.ends_with("      let x = ;\n              ^\n"),
		"{error}"
	);
	app.write("a/b.rn", "pub fn fail() {\n    42.only_in_module()\n}\n");
	let out = app.run(true);
	let error = stderr(&out);
	assert_eq!(out.status.code(), Some(1));
	assert!(
		error.contains(&format!(
			"runtime error at {}, line 2, column 5: no method `only_in_module` on `::std::i64`",
			app.0.join("a/b.rn").display()
		)),
		"{error}"
	);
	assert!(
		error.ends_with("      42.only_in_module()\n      ^\n"),
		"{error}"
	);
	let headers: Vec<_> = error
		.lines()
		.filter(|line| line.starts_with("--- compiled source of "))
		.collect();
	assert_eq!(
		headers,
		["main.rn", "a.rn", "a/b.rn"]
			.map(|p| format!("--- compiled source of {}", app.0.join(p).display()))
	);
}

#[test]
fn imported_structs_and_named_variants_keep_their_field_names() {
	let app = App::new();
	app.write("a.rn", "pub struct Point { x, y }\npub enum E { V { label } }\npub fn point() { Point { x: 3, y: 4 } }\npub fn variant() { E::V { label: 5 } }\n");
	for (call, expected) in [
		("point", "Point {x: 3, y: 4}\n"),
		("variant", "V {label: 5}\n"),
	] {
		app.write(
			"main.rn",
			format!("mod a; pub fn main(_) {{ a::{call}() }}"),
		);
		let out = app.run(false);
		assert!(out.status.success(), "{}", stderr(&out));
		assert_eq!(String::from_utf8(out.stdout).unwrap(), expected);
	}
}

#[test]
fn module_refusals_name_the_declaring_file_and_line() {
	let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/modules");
	for (name, file, line, message) in [
		(
			"misplaced",
			"a.rn",
			1,
			"File not found, expected a module file like",
		),
		(
			"missing",
			"main.rn",
			1,
			"File not found, expected a module file like",
		),
		(
			"duplicate",
			"main.rn",
			2,
			"Module `a` has already been loaded",
		),
	] {
		let out = command()
			.current_dir(&root)
			.arg("run")
			.arg(format!("{name}/main.rn"))
			.output()
			.unwrap();
		let error = stderr(&out);
		assert_eq!(out.status.code(), Some(1));
		assert!(
			error.starts_with(&format!(
				"error at {}, line {line}, column 1:",
				Path::new(name).join(file).display()
			)),
			"{error}"
		);
		assert!(error.contains(message), "{error}");
		assert!(!error.contains(".rn.rn"), "{error}");
	}
}

#[test]
fn real_allowance_and_invalid_utf8_name_the_crossing_module() {
	let app = App::new();
	app.write("main.rn", "mod a; pub fn main(_) { 42 }");
	app.write("a.rn", vec![b' '; 8 * 1024 * 1024 + 1]);
	let out = app.run(false);
	assert_eq!(out.status.code(), Some(1));
	assert!(stderr(&out).contains("8388608 bytes"), "{}", stderr(&out));
	assert!(stderr(&out).contains(&app.0.join("a.rn").display().to_string()));
	app.write("a.rn", [0xff, 0xfe]);
	let out = app.run(false);
	assert_eq!(out.status.code(), Some(1));
	assert!(stderr(&out).contains("valid UTF-8"));
	assert!(stderr(&out).contains(&app.0.join("a.rn").display().to_string()));
}

#[test]
fn sessions_eval_and_config_gain_no_module_loading() {
	let app = App::new();
	app.write("config.rn", "mod a; pub fn main() { #{} }");
	app.write("a.rn", "pub fn value() { 42 }");
	for module in ["mod a;", "mod a { pub fn value() { 42 } }"] {
		let out = command().arg("eval").arg(module).output().unwrap();
		assert!(!out.status.success());
		assert!(
			stderr(&out)
				.contains("modules, imports, macro declarations, and impl blocks work in files")
		);
		let mut child = command()
			.arg("--no-splash")
			.arg("repl")
			.env("RNX_CONFIG", app.0.join("config.rn"))
			.env("RNX_HISTORY", app.0.join("history"))
			.stdin(Stdio::piped())
			.stdout(Stdio::piped())
			.stderr(Stdio::piped())
			.spawn()
			.unwrap();
		writeln!(
			child.stdin.take().unwrap(),
			"{module}\n:reset\n{module}\n42\n:q"
		)
		.unwrap();
		let out = child.wait_with_output().unwrap();
		assert!(out.status.success(), "{}", stderr(&out));
		assert_eq!(
			stderr(&out)
				.matches("modules, imports, macro declarations, and impl blocks work in files")
				.count(),
			2
		);
		assert!(
			stderr(&out).contains("Cannot load modules using a source without an associated URL"),
			"{}",
			stderr(&out)
		);
		assert!(String::from_utf8(out.stdout).unwrap().contains("42"));
	}
}
