//! The interactive session: a line editor over the persistent session.
use crate::complete::{Completion, Names, complete};
use crate::format::{Limits, render};
use crate::host::HostFunction;
use crate::inspect::{self, InspectLimits};
use crate::session::{Completeness, Session, completeness};
use rune::Context;
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::history::FileHistory;
use rustyline::line_buffer::LineBuffer;
use rustyline::validate::{ValidationContext, ValidationResult, Validator};
use rustyline::{Changeset, CompletionType, Config, Editor, Helper, Highlighter, Hinter};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

const PROMPT: &str = "rnx> ";

/// The session's commands and their one-line descriptions, which are what
/// `:help` shows: the description lives with the command, not in a second
/// catalogue.
pub const COMMANDS: [(&str, &str); 6] = [
	(":quit", "end the session"),
	(
		":reset",
		"empty the session: bindings, declarations, and retained units",
	),
	(
		":memory",
		"report source and map storage against the session's bound",
	),
	(":debug", "show the source generated for the last input"),
	(":vars", "list the bindings with their types and values"),
	(
		":help",
		"describe one name, or with no argument list these commands",
	),
];

#[derive(Helper, Highlighter, Hinter)]
struct RnxHelper {
	/// Names the completer draws on, refreshed by the loop after every
	/// input that can change them. Reading them runs nothing.
	names: Rc<RefCell<Names>>,
}
impl Validator for RnxHelper {
	/// Enter accepts an input when it is complete, or when the person has
	/// abandoned it with two blank lines; otherwise Enter inserts a newline
	/// and the editor keeps waiting, which is the continuation prompt.
	fn validate(&self, ctx: &mut ValidationContext) -> rustyline::Result<ValidationResult> {
		let input = ctx.input();
		if abandoned(input) {
			return Ok(ValidationResult::Valid(None));
		}
		Ok(match completeness(input) {
			Completeness::Complete => ValidationResult::Valid(None),
			Completeness::Incomplete => ValidationResult::Incomplete,
		})
	}
}
impl Completer for RnxHelper {
	type Candidate = Pair;
	fn complete(
		&self,
		line: &str,
		pos: usize,
		_ctx: &rustyline::Context<'_>,
	) -> rustyline::Result<(usize, Vec<Pair>)> {
		let names = self.names.borrow();
		Ok(match complete(line, pos, &names) {
			Some(Completion {
				start, candidates, ..
			}) => (
				start,
				candidates
					.into_iter()
					.map(|c| Pair {
						display: c.clone(),
						replacement: c,
					})
					.collect(),
			),
			None => (pos, Vec::new()),
		})
	}
	/// Replace the whole token containing the cursor, not only the part
	/// before it, and leave the cursor after the replacement.
	fn update(&self, line: &mut LineBuffer, start: usize, elected: &str, cl: &mut Changeset) {
		let end = complete(line.as_str(), line.pos(), &self.names.borrow())
			.map(|c| c.end)
			.unwrap_or(line.pos());
		line.replace(start..end, elected, cl);
		line.set_pos(start + elected.len());
	}
}
/// An open input ended by two blank lines in a row.
fn abandoned(input: &str) -> bool {
	input.ends_with("\n\n") && completeness(input) == Completeness::Incomplete
}

/// Where history lives: `RNX_HISTORY`, else `$XDG_STATE_HOME/rnx/history`,
/// else `~/.local/state/rnx/history`.
pub fn history_path() -> Option<PathBuf> {
	if let Some(path) = std::env::var_os("RNX_HISTORY") {
		return Some(PathBuf::from(path));
	}
	let base = std::env::var_os("XDG_STATE_HOME")
		.map(PathBuf::from)
		.or_else(|| {
			std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("state"))
		})?;
	Some(base.join("rnx").join("history"))
}

fn snapshot(session: &Session, host: &[HostFunction]) -> Names {
	Names {
		bindings: session.binding_names(),
		declarations: session.declaration_names(),
		host: host.iter().map(|f| f.path.clone()).collect(),
		commands: COMMANDS.iter().map(|(c, _)| c.to_string()).collect(),
	}
}

/// What handling one input asks the loop to do next.
enum Outcome {
	Continue,
	Reset,
	Quit,
}

/// The ceiling on tracked live allocation request bytes: `RNX_MEMORY_CEILING`
/// if it parses, else the session's default.
fn ceiling() -> usize {
	std::env::var("RNX_MEMORY_CEILING")
		.ok()
		.and_then(|value| value.trim().parse().ok())
		.unwrap_or(crate::session::DEFAULT_CEILING)
}

/// What `:memory` prints. A build with accounting compiled out says so rather
/// than reporting a figure of zero or a ceiling it does not enforce.
fn memory_report(session: &Session) -> String {
	let source = format!(
		"source and map storage: {} bytes (inputs, declarations, and {} units with their source maps)",
		session.retained_bytes(),
		session.retained_units()
	);
	let Some(live) = crate::memory::live() else {
		return format!(
			"allocation accounting is disabled in this build: no figure is tracked and no ceiling is enforced.\n{source}\n"
		);
	};
	let baseline = crate::memory::baseline();
	let net = baseline.map(|b| live as i128 - b as i128);
	format!(
		"tracked live allocation request bytes: {live} of a ceiling of {}{}.\n{}{}\n{source}\nThe figure counts requests through Rust's global allocator only. It excludes native allocations outside it, memory mappings, thread stacks, and child processes, it is not resident memory, and it is not the session's share of anything.\n",
		session.ceiling(),
		if session.over_ceiling() {
			"; evaluation is refused until :reset samples below it"
		} else {
			""
		},
		baseline
			.map(|b| format!("startup reference point: {b} bytes"))
			.unwrap_or_else(|| "startup reference point: not recorded".to_owned()),
		net.map(|n| format!("; net change since: {n:+} bytes"))
			.unwrap_or_default(),
	)
}

/// Handle one input. Every disposable thing it makes is dropped when it
/// returns, which is what lets the loop sample afterwards.
fn handle(
	session: &mut Session,
	context: &Context,
	host: &[HostFunction],
	input: &str,
	limits: &Limits,
	inspect_limits: &InspectLimits,
) -> Outcome {
	if abandoned(input) {
		println!("(input abandoned; it is in history)");
		return Outcome::Continue;
	}
	// A command is the first word; `:help` takes the rest as its argument.
	let trimmed = input.trim();
	let (command, argument) = match trimmed.split_once(char::is_whitespace) {
		Some((command, argument)) => (command, Some(argument)),
		None => (trimmed, None),
	};
	match command {
		":quit" => return Outcome::Quit,
		":reset" => {
			*session = Session::with_ceiling(session.ceiling());
			println!("session reset");
			return Outcome::Reset;
		}
		":memory" => {
			print!("{}", memory_report(session));
			return Outcome::Continue;
		}
		":debug" => {
			println!("{}", session.last_generated());
			return Outcome::Continue;
		}
		// Both read the session and run nothing, so both answer even when the
		// ceiling has been reached and evaluation is refused.
		":vars" => {
			print!("{}", inspect::vars(session, inspect_limits));
			return Outcome::Continue;
		}
		":help" => {
			print!(
				"{}",
				inspect::help(session, host, &COMMANDS, argument, inspect_limits)
			);
			return Outcome::Continue;
		}
		_ => {}
	}
	match session.eval(context, input) {
		Ok(value) => {
			let text = render(&value, Some(session), limits);
			if text != "()" {
				println!("{text}");
			}
		}
		Err(failure) => eprintln!("{failure}"),
	}
	Outcome::Continue
}

pub fn run(context: &Context, host: Vec<HostFunction>) -> crate::Result<()> {
	let config = Config::builder()
		.auto_add_history(false)
		.completion_type(CompletionType::List)
		.build();
	let mut editor: Editor<RnxHelper, FileHistory> = Editor::with_config(config)?;
	let mut session = Session::with_ceiling(ceiling());
	let names = Rc::new(RefCell::new(snapshot(&session, &host)));
	editor.set_helper(Some(RnxHelper {
		names: names.clone(),
	}));
	let history = history_path();
	if let Some(path) = &history {
		if let Some(dir) = path.parent() {
			let _ = std::fs::create_dir_all(dir);
		}
		// Restoring history loads text only; nothing here evaluates it.
		let _ = editor.load_history(path);
	}
	let limits = Limits::default();
	let inspect_limits = InspectLimits::default();
	println!("rnx: a Rune session. :help lists the commands, :quit ends it.");
	// The reference point, and the first sample, after initialization and
	// history loading and before the first evaluation is admitted.
	crate::memory::record_baseline();
	session.sample();
	loop {
		let input = match editor.readline(PROMPT) {
			Ok(line) => line,
			Err(ReadlineError::Interrupted) => continue,
			Err(ReadlineError::Eof) => break,
			Err(e) => return Err(e.into()),
		};
		if input.trim().is_empty() {
			continue;
		}
		let _ = editor.add_history_entry(input.trim_end_matches('\n'));
		if let Some(path) = &history {
			let _ = editor.append_history(path);
		}
		let outcome = handle(
			&mut session,
			context,
			&host,
			&input,
			&limits,
			&inspect_limits,
		);
		// The input buffer is disposable too, and the record excludes it from
		// the sample, so it goes before the sample rather than at the end of
		// the iteration.
		drop(input);
		// Everything else disposable the input made is gone by here. The completion
		// snapshot is refreshed first, because it is state the session keeps,
		// and then one sample covers this command, whichever kind it was: an
		// evaluation, an inspection command, a reset, or an abandoned input.
		// A failed input published nothing, so the snapshot is the same either
		// way.
		*names.borrow_mut() = snapshot(&session, &host);
		session.sample();
		match outcome {
			Outcome::Quit => break,
			Outcome::Reset if session.over_ceiling() => println!(
				"still at or above the ceiling after the reset; evaluation stays refused and restarting rnx may be necessary"
			),
			_ => {}
		}
	}
	Ok(())
}
