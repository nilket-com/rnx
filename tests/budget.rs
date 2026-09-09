//! Record 0021: the budget a script may spend is the operator's to choose,
//! two million unless they say otherwise, and never the script's.
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Ran {
	stdout: String,
	stderr: String,
	code: i32,
}

/// Run a script with the given flags before its path, and the given arguments
/// after it.
fn run_with(source: &str, flags: &[&str], arguments: &[&str]) -> Ran {
	let dir = std::env::temp_dir().join(format!(
		"rnx-budget-{}-{}",
		std::process::id(),
		NEXT.fetch_add(1, Ordering::Relaxed)
	));
	std::fs::create_dir_all(&dir).unwrap();
	let path = dir.join("script.rn");
	std::fs::write(&path, source).unwrap();
	let output = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("run")
		.args(flags)
		.arg(&path)
		.args(arguments)
		.output()
		.unwrap();
	let _ = std::fs::remove_dir_all(&dir);
	Ran {
		stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
		stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
		code: output.status.code().unwrap_or(-1),
	}
}

/// A script that spends about six million instructions: more than the
/// default allows and less than a raised budget.
const SPENDS_MORE_THAN_THE_DEFAULT: &str = "pub fn main(_) {\n\tlet n = 0;\n\tfor i in 0..2000000 { n += i }\n\tprintln!(\"spent {}\", n);\n\tOk(())\n}\n";

/// A script that spends very little — about twenty instructions, measured.
const SPENDS_ALMOST_NOTHING: &str = "pub fn main(_) { println!(\"cheap\"); Ok(()) }\n";

/// A script that spends more than a lowered budget and far less than the
/// default, so the flag can be shown to lower a ceiling as well as raise one.
const SPENDS_A_LITTLE: &str = "pub fn main(_) {\n\tlet n = 0;\n\tfor i in 0..5000 { n += i }\n\tprintln!(\"spent {}\", n);\n\tOk(())\n}\n";

#[test]
fn the_default_is_two_million_and_exhausting_it_still_fails() {
	let ran = run_with(SPENDS_MORE_THAN_THE_DEFAULT, &[], &[]);
	assert_eq!(ran.code, 1, "exhaustion reported success");
	assert_eq!(ran.stdout, "", "a halted script printed its result");
	assert!(
		ran.stderr
			.starts_with("halted: 2000000 instructions exceeded"),
		"{}",
		ran.stderr
	);
	// The reader of that line is the person who can raise it.
	assert!(ran.stderr.contains("--budget"), "{}", ran.stderr);
	// And it does not offer to remove a bound that cannot be removed.
	for word in ["unlimited", "no limit", "disable"] {
		assert!(!ran.stderr.contains(word), "{}: {}", word, ran.stderr);
	}
}

#[test]
fn a_chosen_budget_moves_the_ceiling_in_both_directions() {
	// Up: what halts at the default completes when the operator asks for more.
	let ran = run_with(SPENDS_MORE_THAN_THE_DEFAULT, &["--budget", "40000000"], &[]);
	assert_eq!(ran.code, 0, "{}", ran.stderr);
	assert!(ran.stdout.starts_with("spent "), "{}", ran.stdout);

	// Down: what completes at the default halts when they ask for less. Both
	// directions, so the flag is shown to set the budget rather than merely
	// to be accepted.
	let ran = run_with(SPENDS_A_LITTLE, &[], &[]);
	assert_eq!(ran.code, 0, "{}", ran.stderr);
	let ran = run_with(SPENDS_A_LITTLE, &["--budget", "1000"], &[]);
	assert_eq!(ran.code, 1, "a lowered budget was ignored");
	assert!(
		ran.stderr.starts_with("halted: 1000 instructions exceeded"),
		"{}",
		ran.stderr
	);
}

#[test]
fn a_bad_value_is_refused_before_the_script_starts() {
	for value in ["0", "-5", "lots", "1.5", ""] {
		let ran = run_with(SPENDS_ALMOST_NOTHING, &["--budget", value], &[]);
		assert_ne!(ran.code, 0, "`{value}` was accepted");
		assert!(ran.stderr.contains("--budget"), "`{value}`: {}", ran.stderr);
		// Nothing of the script ran: it prints on its first line, and did not.
		assert_eq!(ran.stdout, "", "`{value}` started the script");
	}
	// And the flag with nothing after it at all.
	let output = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("run")
		.arg("--budget")
		.output()
		.unwrap();
	assert_ne!(output.status.code(), Some(0));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("needs a count"), "{stderr}");
}

#[test]
fn the_sentinel_that_means_no_budget_is_refused() {
	// `usize::MAX` is Rune's sentinel for having no budget: `BudgetGuard::take`
	// returns true without decrementing when the value equals it. Accepting it
	// would remove the bound rather than raise it, quietly and by way of an
	// arbitrary-looking number, so it is refused — and the boundary comes from
	// the platform's `usize`, not from a 64-bit literal, because the sentinel
	// is whatever `usize::MAX` is here.
	let sentinel = usize::MAX.to_string();
	let ran = run_with(SPENDS_MORE_THAN_THE_DEFAULT, &["--budget", &sentinel], &[]);
	assert_ne!(ran.code, 0, "the sentinel was accepted as a budget");
	assert_eq!(ran.stdout, "", "the script ran with no budget");
	assert!(ran.stderr.contains("--budget"), "{}", ran.stderr);

	// One less is a budget, and a large one: accepted, so the refusal above
	// is of the sentinel and not merely of large numbers.
	let largest = (usize::MAX - 1).to_string();
	let ran = run_with(SPENDS_MORE_THAN_THE_DEFAULT, &["--budget", &largest], &[]);
	assert_eq!(ran.code, 0, "{}", ran.stderr);
	assert!(ran.stdout.starts_with("spent "), "{}", ran.stdout);

	// And one more than the sentinel does not fit a `usize` at all. Written
	// by widening, so the case stays the case on a platform where `usize` is
	// not 64 bits.
	let past = format!("{}", usize::MAX as u128 + 1);
	let ran = run_with(SPENDS_MORE_THAN_THE_DEFAULT, &["--budget", &past], &[]);
	assert_ne!(ran.code, 0, "a value too large for usize was accepted");
	assert_eq!(ran.stdout, "");

	// The message says what the range is, so a reader learns it from the
	// refusal rather than from the source.
	assert!(
		ran.stderr
			.contains(&format!("from 1 to {}", usize::MAX - 1)),
		"{}",
		ran.stderr
	);
}

#[test]
fn the_scripts_own_arguments_are_untouched() {
	// Flags are rnx's only before the path. A script that takes its own
	// `--budget` still receives it, and runs with the default.
	let ran = run_with(
		"pub fn main(args) { println!(\"{:?}\", args); Ok(()) }\n",
		&[],
		&["--budget", "5"],
	);
	assert_eq!(ran.code, 0, "{}", ran.stderr);
	assert_eq!(ran.stdout, "[\"--budget\", \"5\"]\n");
}

#[test]
fn both_flags_are_read_in_either_order() {
	for flags in [
		vec!["--budget", "40000000", "--debug-source"],
		vec!["--debug-source", "--budget", "40000000"],
	] {
		let ran = run_with(SPENDS_MORE_THAN_THE_DEFAULT, &flags, &[]);
		assert_eq!(ran.code, 0, "{flags:?}: {}", ran.stderr);
		assert!(ran.stdout.starts_with("spent "), "{flags:?}");
		assert!(
			ran.stderr.contains("compiled source"),
			"{flags:?}: --debug-source was swallowed"
		);
	}
}

#[test]
fn the_budget_is_written_in_one_place_in_the_source() {
	// A SOURCE-LAYOUT CHECK, and not access enforcement. It shows that the
	// value handed to `runner::run` is written in two places — the default and
	// the parsed flag — and that neither the host module nor the environment
	// appears among them. It does NOT prove that no host function could alter
	// Rune's budget: `budget::with` is reachable from anywhere in the crate,
	// and proving otherwise would mean enumerating the host surface, which
	// this does not do. What guards that today is that no host function calls
	// it, which this test would notice only if the call were written here.
	let host = std::fs::read_to_string("src/host.rs").unwrap();
	assert!(
		!host.contains("BUDGET"),
		"the host module can see the budget"
	);
	let main = std::fs::read_to_string("src/main.rs").unwrap();
	let sets: Vec<&str> = main
		.lines()
		.filter(|l| l.contains("budget =") && !l.trim_start().starts_with("//"))
		.collect();
	assert_eq!(
		sets.len(),
		2,
		"expected the default and the parsed flag, found {sets:?}"
	);
	assert!(sets[0].contains("runner::BUDGET"), "{sets:?}");
	// The other is the parsed command-line token, and nothing else.
	assert!(sets[1].contains("value"), "{sets:?}");
	// And no environment variable anywhere near it.
	assert!(
		!main.contains("var(\"RNX_BUDGET\")") && !main.contains("env::var"),
		"the budget can come from the environment"
	);
}
