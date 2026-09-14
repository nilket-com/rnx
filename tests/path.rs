//! Record 0037: explicit lexical answers, independent of filesystem existence.
use std::{
	io::Write,
	process::{Command, Stdio},
};
fn literal(s: &str) -> String {
	serde_json::to_string(s).unwrap().replace("\\u0000", "\\0")
}
fn yes(source: &str) {
	let o = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.env_clear()
		.args(["eval", source])
		.output()
		.unwrap();
	assert!(
		o.status.success(),
		"{source}: {}",
		String::from_utf8_lossy(&o.stderr)
	);
	assert_eq!(
		String::from_utf8_lossy(&o.stdout).trim(),
		"true",
		"{source}"
	);
}
fn option(name: &str, p: &str, expected: Option<&str>) {
	let expected = expected
		.map(|s| format!("Some({})", literal(s)))
		.unwrap_or("None".into());
	yes(&format!("path::{name}({}) == {expected}", literal(p)));
}
fn joined(base: &str, part: &str, expected: &str) {
	yes(&format!(
		"path::join({}, {}) == {}",
		literal(base),
		literal(part),
		literal(expected)
	));
}
#[test]
fn lexical_table() {
	for (name, p, expected) in [
		("parent", "a", Some("")),
		("parent", "", None),
		("parent", "/", None),
		("parent", "a/b/", Some("a")),
		("file_name", "a/b/", Some("b")),
		("file_name", "..", None),
		("file_name", "a/.", Some("a")),
		("extension", "a.tar.gz", Some("gz")),
		("extension", ".bashrc", None),
		("extension", "a.", Some("")),
		("file_stem", "a.tar.gz", Some("a.tar")),
	] {
		option(name, p, expected);
	}
	joined("a", "b", if cfg!(windows) { "a\\b" } else { "a/b" });
	joined("a/", "b", "a/b");
	joined("a", "/b", "/b");
	joined("a", "", if cfg!(windows) { "a\\" } else { "a/" });
	yes(
		r#"path::with_extension("a.tar.gz", "")? == "a.tar" && path::with_extension("a", "x.y")? == "a.x.y""#,
	);
	yes(&format!(
		"path::is_absolute({}) == {}",
		literal("C:\\a"),
		cfg!(windows)
	));
	yes(&format!(
		"path::separator() == {}",
		literal(if cfg!(windows) { "\\" } else { "/" })
	));
}
#[test]
fn unicode_reassembly_preserves_names_not_spelling_or_file_identity() {
	for p in ["é/文", "é//文", "é/./文", "é/文/"] {
		let expected = if cfg!(windows) { "é\\文" } else { "é/文" };
		yes(&format!(
			"let p = {}; path::join(path::parent(p).unwrap(), path::file_name(p).unwrap()) == {}",
			literal(p),
			literal(expected)
		));
	}
	yes(&format!(
		"let p = \"/é/文\"; path::join(path::parent(p).unwrap(), path::file_name(p).unwrap()) == {}",
		literal(if cfg!(windows) { "/é\\文" } else { "/é/文" })
	));
	yes(r#"let p = path::with_extension("é.文", "")?; p == "é" && path::extension(p) == None"#);
	yes(
		r#"let p = path::with_extension("é.文.雪", "")?; p == "é.文" && path::extension(p) == Some("文")"#,
	);
}
#[test]
fn separators_are_catchable_and_nul_is_lexical() {
	for p in ["é", "", "/", ".."] {
		for ext in ["文/雪", "文\\雪"] {
			if ext.contains('/') || cfg!(windows) {
				yes(&format!(
					"match path::with_extension({}, {}) {{ Ok(_) => false, Err(e) => e.contains(\"contains a path separator\") && e.contains(\"文\") && e.contains(\"雪\") }}",
					literal(p),
					literal(ext)
				));
			} else {
				// Backslashes in Unix names are ordinary characters.
				if p == "é" {
					yes(r#"path::with_extension("é", "文\\雪")? == "é.文\\雪""#);
				}
			}
		}
	}
	for p in ["", "/", ".."] {
		yes(&format!(
			"path::with_extension({}, \"文\")? == {}",
			literal(p),
			literal(p)
		));
	}
	yes(
		r#"path::file_name("é\0文") == Some("é\0文") && path::with_extension("é", "\0文")? == "é.\0文""#,
	);
	#[cfg(unix)]
	joined("é", "../文", "é/../文");
}
#[cfg(windows)]
#[test]
fn windows_roots_drives_and_verbatim_join() {
	joined(r"C:\é", r"\文", r"C:\文");
	joined(r"C:\é", r"D:文", r"D:文");
	joined(r"C:\é", r"D:\文", r"D:\文");
	joined(r"\\?\C:\é", r".\雪\..\文", r"\\?\C:\é\文");
	joined(r"\\?\C:\é", "", r"\\?\C:\é\");
	for (p, expected) in [
		(r"\é", false),
		(r"C:é", false),
		(r"C:\é", true),
		(r"\\server\share\é", true),
	] {
		yes(&format!("path::is_absolute({}) == {expected}", literal(p)));
	}
}
#[test]
fn answers_do_not_depend_on_existence() {
	let dir = std::env::temp_dir().join(format!("rnx-path-{}", std::process::id()));
	std::fs::create_dir(&dir).unwrap();
	let p = dir.join("é.文");
	let source = format!(
		"let p = {}; let before = [path::parent(p), path::file_name(p), path::extension(p)]; fs::write_new(p, \"x\")?; before == [path::parent(p), path::file_name(p), path::extension(p)]",
		literal(p.to_str().unwrap())
	);
	yes(&source);
	std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn session_help_and_reset_keep_the_surface() {
	let mut child = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.env_clear()
		.env("TERM", "dumb")
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();
	let mut input = String::new();
	for name in [
		"join",
		"parent",
		"file_name",
		"file_stem",
		"extension",
		"with_extension",
		"is_absolute",
		"separator",
	] {
		input.push_str(&format!(":help path::{name}\n"));
	}
	input.push_str("path::file_name(\"é/文\")\n:reset\npath::file_name(\"é/文\")\n:quit\n");
	child
		.stdin
		.take()
		.unwrap()
		.write_all(input.as_bytes())
		.unwrap();
	let o = child.wait_with_output().unwrap();
	assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
	let s = String::from_utf8(o.stdout).unwrap();
	for name in [
		"join",
		"parent",
		"file_name",
		"file_stem",
		"extension",
		"with_extension",
		"is_absolute",
		"separator",
	] {
		assert!(s.contains(&format!("{name}(")), "{s}");
	}
	assert!(s.contains("an absolute part replaces the base"), "{s}");
	assert!(s.matches("Some(\"文\")").count() >= 2, "{s}");
}
