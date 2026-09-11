//! What `host::process` refuses, and what `host::process_bytes` preserves.
#[path = "harness/commands.rs"]
mod commands;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// The capture limit, which is where a stream stops being whole.
const LIMIT: usize = 2 * 1024 * 1024;

struct Ran {
	stdout: String,
	stderr: String,
	code: i32,
}

fn scratch() -> PathBuf {
	let dir = std::env::temp_dir().join(format!(
		"rnx-bytes-{}-{}",
		std::process::id(),
		NEXT.fetch_add(1, Ordering::Relaxed)
	));
	std::fs::create_dir_all(&dir).unwrap();
	dir
}

/// Run a script in `dir`, so a relative path in it means what it says.
fn run_in(dir: &PathBuf, source: &str) -> Ran {
	let source = commands::expand(source);
	let path = dir.join("script.rn");
	std::fs::write(&path, source).unwrap();
	let output = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("run")
		.arg(&path)
		.current_dir(dir)
		.output()
		.unwrap();
	Ran {
		stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
		stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
		code: output.status.code().unwrap_or(-1),
	}
}

const READ_TEXT: &str = "pub fn main(_) {\n\tlet r = host::process(@CAT_DATA@, 20000)?;\n\tprintln!(\"len={} truncated={}\", r.stdout.len(), r.truncated);\n\tOk(())\n}\n";

#[test]
fn output_that_is_not_utf8_is_refused_and_says_where_to_go() {
	let dir = scratch();
	std::fs::write(dir.join("data"), b"ok \xff\xfe bad\n").unwrap();
	let ran = run_in(&dir, READ_TEXT);
	assert_eq!(ran.code, 1);
	assert!(
		ran.stderr.contains("its standard output is not UTF-8"),
		"{}",
		ran.stderr
	);
	// It names where the fault is and what can read it.
	assert!(ran.stderr.contains("at byte 3"), "{}", ran.stderr);
	assert!(ran.stderr.contains("host::process_bytes"), "{}", ran.stderr);
	// Nothing plausible-looking was handed back.
	assert_eq!(ran.stdout, "");
	assert!(!ran.stderr.contains('\u{fffd}'), "{}", ran.stderr);
	let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn standard_error_that_is_not_utf8_is_refused_too() {
	let dir = scratch();
	std::fs::write(dir.join("data"), b"bad \xff\xfe\n").unwrap();
	let ran = run_in(
		&dir,
		"pub fn main(_) {\n\tlet r = host::process(@DATA_ON_STDERR@, 20000)?;\n\tprintln!(\"{}\", r.code);\n\tOk(())\n}\n",
	);
	assert_eq!(ran.code, 1);
	assert!(
		ran.stderr.contains("its standard error is not UTF-8"),
		"{}",
		ran.stderr
	);
	let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_truncated_capture_ending_mid_character_is_not_an_encoding_error() {
	// A three byte character repeated does not divide the limit, so the
	// capture stops inside one. That is the capture's doing, not the child's.
	let dir = scratch();
	let sign = "€".repeat(LIMIT / 2);
	assert!(sign.len() > LIMIT);
	assert_ne!(LIMIT % 3, 0, "the limit must fall inside a character");
	std::fs::write(dir.join("data"), sign.as_bytes()).unwrap();
	let ran = run_in(&dir, READ_TEXT);
	assert_eq!(ran.code, 0, "{}", ran.stderr);
	assert!(ran.stdout.contains("truncated=true"), "{}", ran.stdout);
	// The partial character is dropped and everything before it is intact.
	// Rune reports a string's length in bytes, so what comes back is the
	// largest multiple of three that fits: the two bytes of the character the
	// capture cut in half are gone, and nothing else is.
	let kept: usize = ran
		.stdout
		.trim_start_matches("len=")
		.split(' ')
		.next()
		.unwrap()
		.parse()
		.unwrap();
	assert_eq!(kept, LIMIT - (LIMIT % 3), "kept {kept} bytes");
	assert_eq!(LIMIT % 3, 2, "the capture cuts two bytes into a character");
	let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_same_tail_without_truncation_is_refused() {
	// The two cases differ only in whether the capture hit its limit, so they
	// must not behave alike. Here the child really did stop mid-character.
	let dir = scratch();
	let mut data = b"fine ".to_vec();
	data.extend_from_slice(&"€".as_bytes()[..2]);
	std::fs::write(dir.join("data"), &data).unwrap();
	let ran = run_in(&dir, READ_TEXT);
	assert_eq!(ran.code, 1, "{}", ran.stdout);
	assert!(
		ran.stderr.contains("its standard output is not UTF-8"),
		"{}",
		ran.stderr
	);
	let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bytes_wrong_where_they_stand_are_refused_even_when_truncated() {
	// Truncation excuses the end of a capture, never the middle of it.
	let dir = scratch();
	let mut data = b"start \xff\xfe here".to_vec();
	data.extend(std::iter::repeat_n(b'x', LIMIT + 1024));
	std::fs::write(dir.join("data"), &data).unwrap();
	let ran = run_in(&dir, READ_TEXT);
	assert_eq!(ran.code, 1, "{}", ran.stdout);
	assert!(
		ran.stderr.contains("its standard output is not UTF-8"),
		"{}",
		ran.stderr
	);
	assert!(ran.stderr.contains("at byte 6"), "{}", ran.stderr);
	let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn process_bytes_returns_the_bytes_exactly() {
	let dir = scratch();
	let every: Vec<u8> = (0..=255u8).collect();
	std::fs::write(dir.join("data"), &every).unwrap();
	let ran = run_in(
		&dir,
		"pub fn main(_) {\n\tlet r = host::process_bytes(@CAT_DATA@, 20000)?;\n\tlet i = 0;\n\tlet out = [];\n\twhile i < r.stdout.len() {\n\t\tout.push(r.stdout[i]);\n\t\ti += 1;\n\t}\n\tprintln!(\"{:?}\", out);\n\tOk(())\n}\n",
	);
	assert_eq!(ran.code, 0, "{}", ran.stderr);
	let expected = format!(
		"[{}]\n",
		every
			.iter()
			.map(|b| b.to_string())
			.collect::<Vec<_>>()
			.join(", ")
	);
	assert_eq!(ran.stdout, expected);
	let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn both_report_the_same_outcome_fields_for_the_same_child() {
	let dir = scratch();
	let ran = run_in(
		&dir,
		"fn show(r) { format!(\"code={:?} timed_out={} cancelled={} truncated={}\", r.code, r.timed_out, r.cancelled, r.truncated) }\npub fn main(_) {\n\tprintln!(\"{}\", show(host::process(@EXIT_7@, 20000)?));\n\tprintln!(\"{}\", show(host::process_bytes(@EXIT_7@, 20000)?));\n\tprintln!(\"{}\", show(host::process(@SLEEP_5@, 200)?));\n\tprintln!(\"{}\", show(host::process_bytes(@SLEEP_5@, 200)?));\n\tOk(())\n}\n",
	);
	assert_eq!(ran.code, 0, "{}", ran.stderr);
	let lines: Vec<&str> = ran.stdout.lines().collect();
	assert_eq!(lines.len(), 4, "{}", ran.stdout);
	// A child that exits, and a child that is killed by the timeout.
	assert_eq!(
		lines[0], lines[1],
		"the two disagree on a child that exited"
	);
	assert_eq!(lines[2], lines[3], "the two disagree on a killed child");
	assert!(lines[0].contains("code=7"), "{}", lines[0]);
	assert!(lines[2].contains("timed_out=true"), "{}", lines[2]);
	let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_missing_method_on_bytes_is_named_rather_than_hashed() {
	let dir = scratch();
	let ran = run_in(&dir, "pub fn main(_) {\n\tb\"abc\".frobnicate()\n}\n");
	assert_eq!(ran.code, 1);
	assert!(
		ran.stderr
			.contains("no method `frobnicate` on `::std::bytes::Bytes`"),
		"{}",
		ran.stderr
	);
	let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn one_streams_truncation_does_not_excuse_the_other() {
	// The two streams are captured separately and fall short separately, so
	// only a stream's own flag can excuse its last character. Combining them
	// before decoding lets a truncated standard output excuse a standard
	// error the child ended mid-character, which was captured whole.
	let dir = scratch();
	std::fs::write(dir.join("big"), vec![b'x'; LIMIT + 1]).unwrap();
	std::fs::write(dir.join("partial"), &"€".as_bytes()[..2]).unwrap();

	// Truncated standard output, malformed standard error.
	let ran = run_in(
		&dir,
		"pub fn main(_) {\n\tlet r = host::process(@BIG_AND_PARTIAL@, 20000)?;\n\tprintln!(\"accepted truncated={} stderr={:?}\", r.truncated, r.stderr);\n\tOk(())\n}\n",
	);
	assert_eq!(ran.code, 1, "accepted: {}", ran.stdout);
	assert!(
		ran.stderr.contains("its standard error is not UTF-8"),
		"{}",
		ran.stderr
	);
	// The bytes the child wrote are not quietly dropped.
	assert!(!ran.stdout.contains("stderr=\"\""), "{}", ran.stdout);

	// And the other way round: truncated standard error, malformed standard
	// output. The fault and the excuse swap streams.
	let ran = run_in(
		&dir,
		"pub fn main(_) {\n\tlet r = host::process(@PARTIAL_AND_BIG@, 20000)?;\n\tprintln!(\"accepted truncated={} stdout={:?}\", r.truncated, r.stdout);\n\tOk(())\n}\n",
	);
	assert_eq!(ran.code, 1, "accepted: {}", ran.stdout);
	assert!(
		ran.stderr.contains("its standard output is not UTF-8"),
		"{}",
		ran.stderr
	);
	let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_streams_own_truncation_still_excuses_its_own_tail() {
	// The pair above must not be fixed by refusing every truncated capture:
	// a stream cut mid-character by the limit is still fine, and the other
	// stream being whole does not change that.
	let dir = scratch();
	std::fs::write(dir.join("big"), "€".repeat(LIMIT / 2).as_bytes()).unwrap();
	let ran = run_in(
		&dir,
		"pub fn main(_) {\n\tlet r = host::process(@BIG_AND_FINE@, 20000)?;\n\tprintln!(\"len={} truncated={} err={:?}\", r.stdout.len(), r.truncated, r.stderr);\n\tOk(())\n}\n",
	);
	assert_eq!(ran.code, 0, "{}", ran.stderr);
	assert!(ran.stdout.contains("truncated=true"), "{}", ran.stdout);
	assert!(
		ran.stdout.contains(&format!("len={}", LIMIT - (LIMIT % 3))),
		"{}",
		ran.stdout
	);
	assert!(ran.stdout.contains("err=\"fine\\n\""), "{}", ran.stdout);
	let _ = std::fs::remove_dir_all(&dir);
}
