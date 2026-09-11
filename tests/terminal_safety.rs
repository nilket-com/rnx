//! Record 0019 decision 4: nothing rnx prints can move a cursor, and a caret
//! lands on the token it points at.
//!
//! Every surface is asserted in one test, so a sixth surface cannot be added
//! without meeting the rule.
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Cases run in parallel, and two of them use the same source: a directory
/// named after the source alone would let one case delete another's script.
static NEXT: AtomicUsize = AtomicUsize::new(0);

fn eval(source: &str) -> (String, String) {
	let output = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("eval")
		.arg(source)
		.output()
		.unwrap();
	(
		String::from_utf8_lossy(&output.stdout).into_owned(),
		String::from_utf8_lossy(&output.stderr).into_owned(),
	)
}

struct Ran {
	stderr: String,
	/// Read by surface 6, which exists only where a path can carry an escape.
	#[cfg_attr(windows, allow(dead_code))]
	path: String,
}

/// Run a script, optionally from a directory whose name carries `marker`.
fn run_in(source: &str, marker: &str) -> Ran {
	let dir = std::env::temp_dir().join(format!(
		"rnx-safety-{}-{marker}{}",
		std::process::id(),
		NEXT.fetch_add(1, Ordering::Relaxed)
	));
	std::fs::create_dir_all(&dir).unwrap();
	let path = dir.join("script.rn");
	std::fs::write(&path, source).unwrap();
	let output = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("run")
		.arg(&path)
		.output()
		.unwrap();
	let ran = Ran {
		stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
		path: path.to_string_lossy().into_owned(),
	};
	let _ = std::fs::remove_dir_all(&dir);
	ran
}

fn run(source: &str) -> Ran {
	run_in(source, "plain")
}

/// What must never reach a terminal: an escape, or a carriage return that is
/// not part of anything a reader asked for.
fn scan(what: &str, bytes: &str) {
	assert!(
		!bytes.contains('\u{1b}'),
		"{what}: an escape reached the terminal: {bytes:?}"
	);
	assert!(
		!bytes.contains('\r'),
		"{what}: a carriage return reached the terminal: {bytes:?}"
	);
	assert!(
		!bytes.contains('\u{7}'),
		"{what}: a bell reached the terminal: {bytes:?}"
	);
}

#[test]
fn nothing_rnx_prints_can_move_a_cursor() {
	// 1. A value rnx renders.
	let (out, _) = eval("[\"a\\u{1b}b\\rc\\u{7}d\"]");
	scan("a rendered value", &out);
	assert!(out.contains("\\u{1b}"), "the escape was dropped: {out}");

	// 2. A returned error that is a string, which prints bare. Bare is not
	// unescaped.
	let (_, err) = eval("Err(\"\\u{1b}[31mRED\\u{1b}[0m\")");
	scan("a string error", &err);
	assert!(err.starts_with("error: \\u{1b}[31mRED"), "{err}");
	assert!(!err.contains('"'), "a string error grew quotes: {err}");

	// 3. A returned error carrying a value.
	let (_, err) = eval("Err(#{message: \"\\u{1b}[31m\"})");
	scan("an error carrying a value", &err);

	// 4. A source excerpt, which is the file as written.
	let ran = run("pub fn main(_) { let x = ; } // \u{1b}[31mESC");
	scan("a source excerpt", &ran.stderr);

	// 5. A diagnostic message, which may quote a script's own text.
	let ran = run("pub fn main(_) { panic!(\"\\u{1b}[31mX\") }");
	scan("a diagnostic message", &ran.stderr);
	assert!(ran.stderr.contains("Panicked: \\u{1b}["), "{}", ran.stderr);

	// 6. The file path in a diagnostic, which is text from outside too.
	//
	// The surface exists only where the operating system lets a path carry an
	// escape. Win32 does not, so on Windows the refusal IS the assertion: the
	// guarantee is stronger there because the name cannot be made, and this
	// gate has to fail, and go back to scanning a diagnostic, the day Windows
	// accepts the name. Skipping the surface would say nothing either way.
	#[cfg(unix)]
	{
		let ran = run_in("pub fn main(_) { let x = ; }", "esc\u{1b}dir");
		scan("a file path", &ran.stderr);
		assert!(ran.stderr.contains("\\u{1b}"), "{}", ran.stderr);
		assert!(ran.path.contains('\u{1b}'), "the case did not set up");
	}
	#[cfg(windows)]
	{
		let why = refusing_a_name_that_carries_an_escape();
		assert_eq!(
			why.raw_os_error(),
			Some(123),
			"the name was refused, but not as ERROR_INVALID_NAME: {why}"
		);
	}
}

/// What Windows says about the directory `run_in` would have made for surface
/// 6, with nothing left behind either way.
///
/// Asked here rather than through `run_in`, which unwraps: a gate that records
/// a platform's refusal has to read it rather than die of it.
#[cfg(windows)]
fn refusing_a_name_that_carries_an_escape() -> std::io::Error {
	let dir = std::env::temp_dir().join(format!(
		"rnx-safety-{}-esc\u{1b}dir{}",
		std::process::id(),
		NEXT.fetch_add(1, Ordering::Relaxed)
	));
	match std::fs::create_dir_all(&dir) {
		Err(why) => why,
		Ok(()) => {
			let _ = std::fs::remove_dir_all(&dir);
			panic!(
				"Windows made {}, so the sixth surface exists here and this gate must scan it rather than record a refusal",
				dir.display()
			)
		}
	}
}

/// The column the caret sits at, and the excerpt line it sits under, from a
/// diagnostic's last two lines.
fn caret_and_excerpt(stderr: &str) -> (usize, String) {
	let lines: Vec<&str> = stderr.lines().collect();
	let caret_line = lines.iter().rev().find(|l| l.contains('^')).unwrap();
	let excerpt = lines[lines.iter().position(|l| l == caret_line).unwrap() - 1];
	let indent = 2;
	let column = caret_line.find('^').unwrap() - indent;
	(column, excerpt[indent..].to_owned())
}

#[test]
fn the_caret_lands_on_the_token() {
	// The property, for a line containing a tab and a line containing a wide
	// character: the caret's column equals the display width of the excerpt
	// printed above it, up to the token it points at. Computed with the same
	// width rule the renderer uses, not counted by eye.
	for (source, token) in [
		("pub fn main(_) {\n\tlet x = ;\n}\n", ";"),
		("pub fn main(_) { let s = \"漢字\"; let x = ; }\n", "; }"),
	] {
		let ran = run(source);
		let (column, excerpt) = caret_and_excerpt(&ran.stderr);
		let at = excerpt.find(token).unwrap();
		let expected = unicode_width::UnicodeWidthStr::width(&excerpt[..at]);
		assert_eq!(
			column, expected,
			"caret at {column}, token at {expected} columns, in {excerpt:?}"
		);
	}
}

#[test]
fn the_reported_column_is_a_source_position_not_a_display_column() {
	// The two kinds of column are not the same thing and are not conflated:
	// the number in the message locates the place in the file, so a person or
	// an editor can go to it, while the caret is a display artifact.
	let ran = run("pub fn main(_) { let s = \"漢字\"; let x = ; }\n");
	// The `;` is the 40th character of the line, and the reported column says
	// so however wide those characters are on a terminal.
	assert!(
		ran.stderr.contains("column 40:"),
		"the reported column moved with the display width: {}",
		ran.stderr
	);
	// The caret sits two columns further along, which is exactly what the two
	// wide characters before it add. The two numbers differing is the point:
	// one locates a character, the other a column.
	let (column, _) = caret_and_excerpt(&ran.stderr);
	assert_eq!(column, 41, "the caret is not where the escaped prefix ends");
	assert_ne!(
		column, 39,
		"the caret used the source position instead of the display width"
	);
}
