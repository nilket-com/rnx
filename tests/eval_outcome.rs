//! What `rnx eval` does with what an expression returns, and whether `run`
//! agrees with it.
#[path = "harness/commands.rs"]
mod commands;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Ran {
	stdout: String,
	stderr: String,
	code: i32,
}

fn finish(output: std::process::Output) -> Ran {
	Ran {
		stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
		stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
		code: output.status.code().unwrap_or(-1),
	}
}

fn evaluated(source: &str) -> Ran {
	finish(
		Command::new(env!("CARGO_BIN_EXE_rnx"))
			.arg("eval")
			.arg(commands::expand(source))
			.output()
			.unwrap(),
	)
}

/// The same expression as a script's `main`, so the two can be compared.
fn ran(source: &str) -> Ran {
	let source = commands::expand(source);
	let dir = std::env::temp_dir().join(format!(
		"rnx-eval-{}-{}",
		std::process::id(),
		NEXT.fetch_add(1, Ordering::Relaxed)
	));
	std::fs::create_dir_all(&dir).unwrap();
	let path = dir.join("script.rn");
	std::fs::write(&path, format!("pub fn main(_) {{ {source} }}\n")).unwrap();
	let out = finish(
		Command::new(env!("CARGO_BIN_EXE_rnx"))
			.arg("run")
			.arg(&path)
			.output()
			.unwrap(),
	);
	let _ = std::fs::remove_dir_all(&dir);
	out
}

/// A script written out in full, rather than one expression wrapped in
/// `main`: a declaration in a module cannot live inside a function body.
fn ran_file(source: &str) -> Ran {
	let dir = std::env::temp_dir().join(format!(
		"rnx-eval-file-{}-{}",
		std::process::id(),
		NEXT.fetch_add(1, Ordering::Relaxed)
	));
	std::fs::create_dir_all(&dir).unwrap();
	let path = dir.join("script.rn");
	std::fs::write(&path, source).unwrap();
	let out = finish(
		Command::new(env!("CARGO_BIN_EXE_rnx"))
			.arg("run")
			.arg(&path)
			.output()
			.unwrap(),
	);
	let _ = std::fs::remove_dir_all(&dir);
	out
}

#[test]
fn a_returned_error_fails() {
	// The case that motivated the record: a read that did not happen, whose
	// status said it had.
	let ran = evaluated("host::read(\"no-such-file-24680\")");
	assert_eq!(ran.code, 1, "a failed read reported success");
	assert_eq!(ran.stdout, "");
	assert!(
		ran.stderr
			.starts_with("error: cannot read no-such-file-24680"),
		"{}",
		ran.stderr
	);
}

#[test]
fn a_string_error_prints_bare() {
	let ran = evaluated("Err(\"plain words\")");
	assert_eq!(ran.code, 1);
	assert_eq!(ran.stderr, "error: plain words\n");
	assert_eq!(ran.stdout, "");
}

#[test]
fn a_returned_ok_is_unwrapped() {
	assert_eq!(evaluated("Ok(5)").stdout, "5\n");
	assert_eq!(evaluated("Ok(5)").code, 0);
}

#[test]
fn a_value_that_is_not_a_result_is_unchanged() {
	let ran = evaluated("1 + 1");
	assert_eq!(ran.code, 0, "{}", ran.stderr);
	assert_eq!(ran.stdout, "2\n");
	assert_eq!(ran.stderr, "");
}

#[test]
fn a_child_that_failed_is_still_a_success() {
	// The call worked and the child did not. Only the shape of the returned
	// value decides, so nothing inside it turns this into a failure.
	let ran = evaluated("host::process(@FAIL@, 5000).map(|r| r.code)");
	assert_eq!(ran.code, 0, "{}", ran.stderr);
	assert_eq!(ran.stdout, "1\n");
	assert_eq!(ran.stderr, "");
}

#[test]
fn the_two_entry_points_agree_on_everything_they_show() {
	// Record 0018 measured nineteen shapes across the two entry points and
	// found four categories of difference. One renderer settles all of them,
	// so the table that named them is now a single expectation: identical.
	// The one case still outstanding is a struct's field names, which has its
	// own test below and is decision 5's.
	let cases = [
		"Err(\"plain words\")",
		"host::read(\"no-such-file-13579\")",
		"Err(#{code: 7})",
		"Err([1, 2, 3])",
		"Ok(5)",
		"1 + 1",
		"\"hi\"",
		"()",
		"host::process(@FAIL@, 5000).map(|r| r.code)",
		// Differed in spacing under two renderers.
		"[1, 2, 3]",
		"#{a: 1}",
		// Spelled differently under two renderers: JSON has no form for an
		// option, a tuple, or a character.
		"None",
		"Some(1)",
		"(1, 2)",
		"'x'",
		// `run` could not show these at all.
		"enum E { A } E::A",
		"|| 1",
		"0..3",
		"fn h() {} h",
	];
	for source in cases {
		let e = evaluated(source);
		let r = ran(source);
		assert_eq!(e.code, r.code, "{source}: eval {} run {}", e.code, r.code);
		assert_eq!(e.stdout, r.stdout, "{source}: standard output");
		assert_eq!(e.stderr, r.stderr, "{source}: standard error");
	}
}

#[test]
fn shapes_json_could_not_show_now_render() {
	// Record 0018 gate 7 asserted these five reported a failure, because JSON
	// had no form for them. This replaces it rather than deleting it: the
	// readable renderer has a form for each, so they are values again.
	for (source, expected) in [
		("enum E { A } E::A", "A\n"),
		("|| 1", "<function>\n"),
		("fn h() {} h", "<function>\n"),
		("0..3", "<::std::ops::Range>\n"),
	] {
		let r = ran(source);
		assert_eq!(r.code, 0, "{source}: {}", r.stderr);
		assert_eq!(r.stdout, expected, "{source}");
		assert_eq!(r.stderr, "", "{source}");
		assert!(
			!r.stdout.contains("serialization error"),
			"{source}: the old text survives"
		);
	}
}

#[test]
fn a_struct_names_its_fields_from_either_entry_point() {
	// The last difference record 0018 left. `run` reads the declarations in
	// the file it compiled and `eval` those the session retained, and both
	// verify a candidate against the value before using it, so the two now
	// agree. This replaces 0018's case asserting `Problem {0: 7}`, which was
	// a position dressed as a field name.
	let value = "struct P { code } P { code: 7 }";
	assert_eq!(evaluated(value).stdout, "P {code: 7}\n");
	assert_eq!(ran(value).stdout, "P {code: 7}\n");
	let error = "struct Problem { code } Err(Problem { code: 7 })";
	let e = evaluated(error);
	let r = ran(error);
	assert_eq!(e.code, 1, "{}", e.stderr);
	assert_eq!(r.code, 1, "{}", r.stderr);
	assert_eq!(e.stderr, "error: Problem {code: 7}\n");
	assert_eq!(r.stderr, e.stderr, "the two entry points still differ");
	assert!(
		!r.stderr.contains("{0:"),
		"a position is being named: {}",
		r.stderr
	);
}

#[test]
fn two_structs_of_one_name_each_render_their_own_fields() {
	// A short name is a candidate key and not an identity: one file can
	// declare `P` twice in different scopes. Each value verifies against the
	// candidate that fits it, so neither borrows the other's field names.
	let r = ran_file(
		"mod a { pub struct P { x } }\nmod b { pub struct P { y } }\npub fn main(_) { (a::P { x: 1 }, b::P { y: 2 }) }\n",
	);
	assert_eq!(r.code, 0, "{}", r.stderr);
	assert_eq!(r.stdout, "(P {x: 1}, P {y: 2})\n");
}

#[test]
fn a_struct_variant_names_its_fields_too() {
	// A struct variant carries names exactly as a struct does, and a value of
	// one is a struct to the renderer. This printed `C {0: 4}` before.
	let source = "enum E { A, B(v), C { w } } (E::A, E::B(3), E::C { w: 4 })";
	assert_eq!(evaluated(source).stdout, "(A, B(3), C {w: 4})\n");
	assert_eq!(ran(source).stdout, "(A, B(3), C {w: 4})\n");
}

#[test]
fn a_returned_value_is_not_elided_at_either_shell_entry_point() {
	// The prompt previews and marks what it cut; a shell entry point's output
	// is the product. A caller reading 64 of 4000 items has been told nothing
	// true, so both entry points print all 4000. The other half of this
	// split — that the prompt still previews the same value — is asserted in
	// `format.rs`, where both limit sets are defined.
	let source = "let v = []; for i in 0..4000 { v.push(1) } v";
	let e = evaluated(source);
	let r = ran(source);
	assert_eq!(e.code, 0, "{}", e.stderr);
	assert_eq!(e.stdout, r.stdout, "the two entry points disagree");
	assert_eq!(e.stdout.matches('1').count(), 4000, "items are missing");
	for marker in ["more)", "truncated", "…"] {
		assert!(
			!e.stdout.contains(marker),
			"the output was elided and marked `{marker}`"
		);
	}
}

/// A depth far past the renderer's 256 that the platform can still build and
/// drop for itself. What fails past it is Rune's own recursion, not a contract
/// of rnx's, so the number is a property of the machine and belongs here rather
/// than inline.
///
/// Record 0019 measured Linux's ceiling between 49,152 and 65,536, and 32,768
/// sits inside it.
#[cfg(unix)]
const FAR_PAST_THE_BOUND: usize = 32_768;

/// Windows' ceiling is about an order of magnitude lower — a 1 MB main thread
/// against Linux's 8 MB is the untested but consistent candidate. Measured
/// against the debug binary these tests run: this shape renders and drops at
/// 4,096 and overflows by 4,608, so 2,048 is eight times the bound under test
/// and less than half the depth that fails.
///
/// The entry point matters and the ceiling is `run`'s. `rnx eval` survives
/// 32,768 on the same binary, because a value returned from a script's `main`
/// is dropped deeper than one evaluated.
#[cfg(windows)]
const FAR_PAST_THE_BOUND: usize = 2_048;

/// A value nested `containers` deep: `[]` is one, and each wrap adds another.
fn nested(containers: usize) -> String {
	format!("let v = []; for i in 0..{} {{ v = [v] }} v", containers - 1)
}

#[test]
fn the_depth_bound_is_a_boundary() {
	// 256 is the bound the renderer's own recursion sets, measured rather
	// than guessed: with the limits lifted a debug build renders 2048 deep
	// and dies by 3072.
	for entry in ["eval", "run"] {
		let at = if entry == "eval" {
			evaluated(&nested(256))
		} else {
			ran(&nested(256))
		};
		assert_eq!(at.code, 0, "{entry} at the bound: {}", at.stderr);
		assert!(at.stdout.starts_with("[[["), "{entry}: {}", at.stdout);

		let past = if entry == "eval" {
			evaluated(&nested(257))
		} else {
			ran(&nested(257))
		};
		assert_eq!(past.code, 1, "{entry} past the bound reported success");
		assert_eq!(past.stdout, "", "{entry} printed part of the value");
		assert!(
			past.stderr.contains("nested deeper than 256"),
			"{entry}: {}",
			past.stderr
		);
	}
}

#[test]
fn a_deep_value_is_refused_with_a_status_and_never_a_signal() {
	// Cleanup is part of the bound. A value far past it is refused, and the
	// process still exits normally while dropping it. The depth is
	// `FAR_PAST_THE_BOUND`, because how deep a value can be built and dropped
	// at all is the platform's answer rather than rnx's — and past it the
	// abort is one no check on this side can prevent, which the record raises
	// upstream.
	let r = ran(&nested(FAR_PAST_THE_BOUND));
	assert_eq!(r.code, 1, "expected a refusal, got {}", r.code);
	assert_eq!(r.stdout, "");
	assert!(r.stderr.contains("nested deeper than 256"), "{}", r.stderr);
}

#[test]
fn a_failed_rendering_leaves_a_scripts_own_output_alone() {
	// The text is built whole before anything is written, so a failure adds
	// nothing to what the script already printed. This is a claim about
	// rendering, not about the stream: a write that fails part-way is a
	// different thing, and is not promised away here.
	let r =
		ran("println!(\"first\"); println!(\"second\"); let v = []; for i in 0..300 { v = [v] } v");
	assert_eq!(r.code, 1);
	assert_eq!(r.stdout, "first\nsecond\n", "output was added or lost");
	assert!(r.stderr.contains("nested deeper than 256"), "{}", r.stderr);
}

#[test]
fn nothing_else_about_eval_changed() {
	// Its diagnostics are what records 0002 and 0009 gave it.
	let ran = evaluated("let x = ;");
	assert_eq!(ran.code, 1);
	assert!(ran.stderr.contains("line 1, column 9"), "{}", ran.stderr);
	let ran = evaluated("let v = []; v[7]");
	assert_eq!(ran.code, 1);
	assert!(ran.stderr.contains("runtime error at"), "{}", ran.stderr);
	// And `host::exit` still chooses the status, as record 0013 decided.
	let ran = evaluated("host::exit(5)");
	assert_eq!(ran.code, 5);
	assert_eq!(ran.stderr, "");
}

#[test]
fn an_error_reads_the_same_and_costs_the_same_from_either_entry_point() {
	// One reporter for both. A string error used to skip the budget that a
	// rendered error obeyed, so the same failure cost twenty thousand bytes
	// through one shape and four thousand through the other.
	let long = "let s = \"\"; for i in 0..2000 { s += \"0123456789\" }";
	let e = evaluated(&format!("{long} Err(s)"));
	let r = ran(&format!("{long} Err(s)"));
	assert_eq!(e.code, 1);
	assert_eq!(r.code, 1);
	assert_eq!(e.stderr, r.stderr, "the two entry points differ");
	assert!(
		e.stderr.len() < 4_300,
		"a string error is {} bytes",
		e.stderr.len()
	);
	assert!(
		e.stderr.contains("…(+15904 bytes)"),
		"the omitted count is missing: {}",
		&e.stderr[e.stderr.len().saturating_sub(40)..]
	);
	// Bare still means bare: no quotes, no wrapper.
	assert!(
		e.stderr.starts_with("error: 0123456789"),
		"{}",
		&e.stderr[..40]
	);

	// And a string that is nothing but escapes, where every character costs
	// six bytes: the budget bounds the work, not the character count.
	let escapes = "let s = \"\"; for i in 0..2000 { s += \"\\u{1b}\" }";
	let e = evaluated(&format!("{escapes} Err(s)"));
	let r = ran(&format!("{escapes} Err(s)"));
	assert_eq!(e.stderr, r.stderr, "the two entry points differ");
	assert!(
		e.stderr.len() < 4_300,
		"an escaped error is {} bytes",
		e.stderr.len()
	);
	assert!(
		!e.stderr.contains('\u{1b}'),
		"an escape reached the terminal"
	);
}
