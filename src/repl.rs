//! The interactive session: a line editor over the persistent session.
use crate::format::{Limits, render};
use crate::session::{Completeness, Session, completeness};
use rune::Context;
use rustyline::error::ReadlineError;
use rustyline::history::FileHistory;
use rustyline::validate::{ValidationContext, ValidationResult, Validator};
use rustyline::{Completer, Config, Editor, Helper, Highlighter, Hinter};
use std::path::PathBuf;

const PROMPT: &str = "rnx> ";

#[derive(Completer, Helper, Highlighter, Hinter)]
struct RnxHelper;
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

pub fn run(context: &Context) -> crate::Result<()> {
	let config = Config::builder().auto_add_history(false).build();
	let mut editor: Editor<RnxHelper, FileHistory> = Editor::with_config(config)?;
	editor.set_helper(Some(RnxHelper));
	let history = history_path();
	if let Some(path) = &history {
		if let Some(dir) = path.parent() {
			let _ = std::fs::create_dir_all(dir);
		}
		// Restoring history loads text only; nothing here evaluates it.
		let _ = editor.load_history(path);
	}
	let mut session = Session::new();
	let limits = Limits::default();
	println!("rnx: a Rune session. :quit ends it, :reset clears it, :memory reports it.");
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
		if abandoned(&input) {
			println!("(input abandoned; it is in history)");
			continue;
		}
		match input.trim() {
			":quit" => break,
			":reset" => {
				session = Session::new();
				println!("session reset");
				continue;
			}
			":memory" => {
				println!(
					"source and map storage: {} bytes of {} (inputs, declarations, {} units and their source maps). Values held by bindings and compiled unit storage are not measured; the first release record's memory gate is still open.",
					session.retained_bytes(),
					session.bound(),
					session.retained_units()
				);
				continue;
			}
			":debug" => {
				println!("{}", session.last_generated());
				continue;
			}
			_ => {}
		}
		match session.eval(context, &input) {
			Ok(value) => {
				let text = render(&value, Some(&session), &limits);
				if text != "()" {
					println!("{text}");
				}
			}
			Err(failure) => eprintln!("{failure}"),
		}
	}
	Ok(())
}
