//! What `host::json_stringify` does, and what it refuses. Record 0019
//! decision 3: JSON is explicit, bounded, has one user-facing implementation,
//! and says where it failed.
use std::process::Command;

struct Ran {
	stdout: String,
	stderr: String,
	code: i32,
	signalled: bool,
}

fn evaluated(source: &str) -> Ran {
	let output = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("eval")
		.arg(source)
		.output()
		.unwrap();
	Ran {
		stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
		stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
		code: output.status.code().unwrap_or(-1),
		// A stack overflow aborts, which arrives as a signal and no code.
		signalled: output.status.code().is_none(),
	}
}

/// A value nested `containers` deep: `[]` is one, and each wrap adds another.
fn nested(containers: usize) -> String {
	format!("let v = []; for i in 0..{} {{ v = [v] }}", containers - 1)
}

#[test]
fn a_cycle_is_refused_rather_than_aborting_the_process() {
	// Before this cut, Rune's serializer recursed until the stack ran out and
	// the process died with a signal and no diagnostic. This gate fails on
	// that binary, which is what makes it a control rather than a claim.
	let r = evaluated("let v = [1]; v.push(v); host::json_stringify(v)");
	assert!(!r.signalled, "the process died on a signal: {}", r.stderr);
	assert_eq!(r.code, 1, "{}", r.stderr);
	assert!(
		r.stderr.contains("cannot serialize a cycle"),
		"{}",
		r.stderr
	);
}

#[test]
fn repeated_shared_data_is_not_a_cycle() {
	// One allocation referenced twice by the same sequence is repeated data.
	// A guard that tracked values seen rather than the path currently being
	// descended would refuse this, so it is pinned beside the cycle.
	let r = evaluated("let a = [1]; host::json_stringify([a, a])");
	assert_eq!(r.code, 0, "{}", r.stderr);
	assert_eq!(r.stdout, "\"[[1],[1]]\"\n");
}

#[test]
fn the_depth_bound_is_shared_with_the_renderer() {
	let inside = evaluated(&format!("{} host::json_stringify(v)", nested(256)));
	assert_eq!(inside.code, 0, "at the bound: {}", inside.stderr);
	let past = evaluated(&format!("{} host::json_stringify(v)", nested(257)));
	assert!(!past.signalled, "the process died on a signal");
	assert_eq!(past.code, 1, "{}", past.stderr);
	assert!(
		past.stderr.contains("nested deeper than 256 levels"),
		"{}",
		past.stderr
	);
}

#[test]
fn a_refusal_is_catchable_and_the_script_carries_on() {
	// Cleanup is part of the bound: at a depth far past it the script catches
	// the error, does more work, and the process exits normally with the
	// value dropped. 32,768 is inside what Rune can itself build and drop.
	let dir = std::env::temp_dir().join(format!("rnx-json-catch-{}", std::process::id()));
	std::fs::create_dir_all(&dir).unwrap();
	let path = dir.join("catch.rn");
	std::fs::write(
		&path,
		format!(
			"pub fn main(_) {{\n\t{}\n\tlet outcome = host::json_stringify(v);\n\tprintln!(\"refused: {{}}\", outcome.is_err());\n\tprintln!(\"still running\");\n\tOk(())\n}}\n",
			nested(32_768)
		),
	)
	.unwrap();
	let output = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("run")
		.arg(&path)
		.output()
		.unwrap();
	let _ = std::fs::remove_dir_all(&dir);
	assert!(
		output.status.code().is_some(),
		"the process died on a signal instead of exiting"
	);
	assert_eq!(output.status.code(), Some(0));
	assert_eq!(
		String::from_utf8_lossy(&output.stdout),
		"refused: true\nstill running\n"
	);
}

#[test]
fn every_arm_of_the_serializer_is_mirrored() {
	// Read off `rune-0.14.1/src/runtime/value/serde.rs`, not assumed. Four
	// shapes descend and the rest are leaves; a result is refused, which is
	// the arm most easily got wrong by supposing it behaves like an option.
	for (source, expected) in [
		("host::json_stringify(())", "\"null\"\n"),
		("host::json_stringify(true)", "\"true\"\n"),
		("host::json_stringify('x')", "\"\\\"x\\\"\"\n"),
		("host::json_stringify(7)", "\"7\"\n"),
		("host::json_stringify(1.5)", "\"1.5\"\n"),
		("host::json_stringify(\"hi\")", "\"\\\"hi\\\"\"\n"),
		("host::json_stringify(None)", "\"null\"\n"),
		// An option is serialized as its inside, so it collapses: JSON does
		// not round-trip one.
		("host::json_stringify(Some(Some(1)))", "\"1\"\n"),
		("host::json_stringify([1, 2])", "\"[1,2]\"\n"),
		("host::json_stringify((1, 2))", "\"[1,2]\"\n"),
		("host::json_stringify(#{a: 1})", "\"{\\\"a\\\":1}\"\n"),
	] {
		let r = evaluated(source);
		assert_eq!(r.code, 0, "{source}: {}", r.stderr);
		assert_eq!(r.stdout, expected, "{source}");
	}
	for (source, refusal) in [
		("host::json_stringify(Ok(1))", "cannot serialize a result"),
		("host::json_stringify(Err(1))", "cannot serialize a result"),
		("host::json_stringify(|| 1)", "cannot serialize a function"),
		(
			"fn h() {} host::json_stringify(h)",
			"cannot serialize a function",
		),
		("host::json_stringify(0..3)", "an external reference"),
		(
			"struct P { c } host::json_stringify(P { c: 1 })",
			"cannot serialize struct P",
		),
		(
			"enum E { A } host::json_stringify(E::A)",
			"cannot serialize empty struct E::A",
		),
	] {
		let r = evaluated(source);
		assert!(!r.signalled, "{source}: died on a signal");
		assert_eq!(r.code, 1, "{source}: {}", r.stdout);
		assert!(r.stderr.contains(refusal), "{source}: {}", r.stderr);
	}
}

#[test]
fn a_refusal_names_where_it_happened() {
	for (source, path) in [
		(
			"host::json_stringify(#{a: #{b: #{c: #{d: #{e: #{f: #{g: [1, || 1]}}}}}}})",
			".a.b.c.d.e.f.g[1]",
		),
		("host::json_stringify([[[|| 1]]])", "[0][0][0]"),
		("host::json_stringify(|| 1)", "the value itself"),
	] {
		let r = evaluated(source);
		assert_eq!(r.code, 1, "{source}");
		assert!(
			r.stderr.trim_end().ends_with(path),
			"{source}: {}",
			r.stderr
		);
	}
}

#[test]
fn a_path_cannot_be_confused_with_one_that_looks_like_it() {
	// The case that decides the syntax. A key containing a dot, a bracket, a
	// quote, or a control character is quoted and escaped, so it cannot read
	// as the nesting it imitates, and a numeric key cannot read as an index.
	let dotted = evaluated("host::json_stringify(#{\"a.b\": || 1})");
	let nested_keys = evaluated("host::json_stringify(#{a: #{b: || 1}})");
	assert!(
		dotted.stderr.trim_end().ends_with("[\"a.b\"]"),
		"{}",
		dotted.stderr
	);
	assert!(
		nested_keys.stderr.trim_end().ends_with(".a.b"),
		"{}",
		nested_keys.stderr
	);
	assert_ne!(dotted.stderr, nested_keys.stderr, "two paths collided");

	let numeric_key = evaluated("host::json_stringify(#{\"3\": || 1})");
	let index = evaluated("host::json_stringify([0, 1, 2, || 1])");
	assert!(
		numeric_key.stderr.trim_end().ends_with("[\"3\"]"),
		"{}",
		numeric_key.stderr
	);
	assert!(index.stderr.trim_end().ends_with("[3]"), "{}", index.stderr);
	assert_ne!(numeric_key.stderr, index.stderr, "a key read as an index");

	let quoted = evaluated("host::json_stringify(#{\"say \\\"hi\\\"\": || 1})");
	assert!(
		quoted.stderr.contains("[\"say \\\"hi\\\"\"]"),
		"{}",
		quoted.stderr
	);
	let controlled = evaluated("host::json_stringify(#{\"a\\u{1b}b\": || 1})");
	assert!(
		controlled.stderr.contains("\\u{1b}"),
		"{}",
		controlled.stderr
	);
	assert!(
		!controlled.stderr.bytes().any(|b| b == 0x1b),
		"an escape reached the terminal through a path"
	);
}

#[test]
fn one_user_facing_serializer() {
	// Behaviourally: a returned value takes the renderer's path, so `None` is
	// `None` and not `null`. The writer `run` used to own is gone.
	let r = evaluated("None");
	assert_eq!(r.stdout, "None\n");

	// Structurally: no second serializer of a script value is introduced by
	// the obvious route. The other `serde_json` users in the crate are
	// internal and cannot receive an arbitrary script value — `inspect` and
	// `session` quote a filesystem path, `main` encodes the argument vector,
	// and `host` builds a process result it shapes itself.
	let mut writers = Vec::new();
	for entry in std::fs::read_dir("src").unwrap() {
		let path = entry.unwrap().path();
		if path.extension().is_none_or(|e| e != "rs") {
			continue;
		}
		let text = std::fs::read_to_string(&path).unwrap();
		if text.contains("serde_json::to_string(value)")
			|| text.contains("serde_json::to_string(&value)")
		{
			writers.push(path.file_name().unwrap().to_string_lossy().into_owned());
		}
	}
	assert_eq!(
		writers,
		vec!["json.rs"],
		"another place serializes a script value to JSON"
	);
}

/// A chain of `Some` `deep` levels tall around an integer. It adds no
/// container, which is how it slipped past a bound that only counted them.
fn options(deep: usize) -> String {
	format!("let v = 1; for i in 0..{deep} {{ v = Some(v) }}")
}

#[test]
fn the_depth_bound_covers_a_path_with_no_container_on_it() {
	// The hole: `walk` recursed through an option without checking the depth,
	// so 257 of them serialized and 8,192 of them aborted the process. The
	// bound is now enforced on entry to every value, at the same threshold
	// the renderer uses, which is why the two agree case for case below.
	let inside = evaluated(&format!("{} host::json_stringify(v)", options(256)));
	assert_eq!(inside.code, 0, "at the bound: {}", inside.stderr);
	assert_eq!(inside.stdout, "\"1\"\n");

	let past = evaluated(&format!("{} host::json_stringify(v)", options(257)));
	assert!(!past.signalled, "the process died on a signal");
	assert_eq!(past.code, 1, "{}", past.stdout);
	assert!(
		past.stderr.contains("nested deeper than 256 levels"),
		"{}",
		past.stderr
	);

	// Mixed: a container and an option alternating, neither counting alone.
	let mixed = evaluated("let v = 1; for i in 0..200 { v = [Some(v)] } host::json_stringify(v)");
	assert!(!mixed.signalled, "the process died on a signal");
	assert_eq!(mixed.code, 1, "{}", mixed.stdout);
	assert!(
		mixed.stderr.contains("nested deeper than 256 levels"),
		"{}",
		mixed.stderr
	);
}

#[test]
fn the_two_walks_agree_on_what_is_too_deep() {
	// One constant, one threshold, whichever path reaches it: a rendering and
	// a serialization refuse the same chains at the same depth.
	for deep in [256, 257] {
		let rendered = evaluated(&format!("{} v", options(deep)));
		let serialized = evaluated(&format!("{} host::json_stringify(v)", options(deep)));
		assert_eq!(
			rendered.code, serialized.code,
			"at {deep}: rendering exits {} and serializing {}",
			rendered.code, serialized.code
		);
	}
}

#[test]
fn a_deep_option_chain_is_catchable_and_cleans_up() {
	// The same guarantee gate 3 makes for containers, on the path that had no
	// bound at all: far past the bound, the refusal is caught, the script goes
	// on, and the process exits normally with the value dropped.
	let dir = std::env::temp_dir().join(format!("rnx-json-opt-{}", std::process::id()));
	std::fs::create_dir_all(&dir).unwrap();
	let path = dir.join("catch.rn");
	std::fs::write(
		&path,
		format!(
			"pub fn main(_) {{\n\t{}\n\tlet outcome = host::json_stringify(v);\n\tprintln!(\"refused: {{}}\", outcome.is_err());\n\tprintln!(\"still running\");\n\tOk(())\n}}\n",
			options(8_192)
		),
	)
	.unwrap();
	let output = std::process::Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("run")
		.arg(&path)
		.output()
		.unwrap();
	let _ = std::fs::remove_dir_all(&dir);
	assert!(
		output.status.code().is_some(),
		"the process died on a signal instead of exiting"
	);
	assert_eq!(output.status.code(), Some(0));
	assert_eq!(
		String::from_utf8_lossy(&output.stdout),
		"refused: true\nstill running\n"
	);
}
