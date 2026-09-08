//! `rnx run`: compile one file and execute its `main`.
//!
//! Diagnostics have the shape the session's do, because the first port
//! measured what their absence costs. `run` compiles the file as written,
//! with no wrapper and no prelude, so a position in the compiled unit is a
//! position in the file and no mapping is needed beyond the file's own text.
//!
//! Everything rnx says goes to standard error. What the script prints goes
//! to standard output, which is a guarantee this module preserves rather
//! than introduces.
use crate::format::{Limits, render};
use crate::session::position;
use rune::runtime::{Unit, Value, VmError, budget};
use rune::{Context, Diagnostics, Source, Sources, Vm};
use std::sync::Arc;

/// Instructions one script may spend. This is the limit `run` has always
/// had; the session's is far larger because Ctrl-C can stop an input there
/// and nothing can stop one here.
pub const BUDGET: usize = 2_000_000;

/// The flag that asks for the compiled source. It is read only before the
/// script path; everything after the path belongs to the script.
pub const DEBUG_SOURCE: &str = "--debug-source";

/// Report a failure with a place in the file, in the shape the session uses.
fn located(kind: &str, path: &str, text: &str, offset: usize, message: &str) {
	let (line, column, line_text) = position(text, offset);
	eprintln!("{kind} at {path}, line {line}, column {column}: {message}");
	eprintln!("  {line_text}");
	eprintln!("  {}^", " ".repeat(column.saturating_sub(1)));
}

/// Report something that has no place in any source and never could: a file
/// that could not be read, or an error a script returned. Nothing is
/// invented for it, and it does not claim a position was looked for.
fn unplaced(kind: &str, message: &str) {
	eprintln!("{kind}: {message}");
}

/// Report a fault the runtime or the compiler raised whose place could not
/// be resolved. Unlike the above, a position was expected here, so the
/// absence is stated rather than passed over in silence.
fn unlocated(kind: &str, message: &str) {
	eprintln!("{kind} (no source position is available for this fault): {message}");
}

fn compile(context: &Context, path: &str, text: &str, debug_source: bool) -> Option<Unit> {
	let mut sources = Sources::new();
	let source = match Source::memory(text) {
		Ok(source) => source,
		Err(error) => {
			unplaced("error", &error.to_string());
			return None;
		}
	};
	if sources.insert(source).is_err() {
		unplaced("error", "the source could not be held for compilation");
		return None;
	}
	let mut diagnostics = Diagnostics::new();
	let built = rune::prepare(&mut sources)
		.with_context(context)
		.with_diagnostics(&mut diagnostics)
		.build();
	match built {
		Ok(unit) => Some(unit),
		Err(error) => {
			let first = diagnostics.diagnostics().iter().find_map(|d| match d {
				rune::diagnostics::Diagnostic::Fatal(fatal) => match fatal.kind() {
					rune::diagnostics::FatalDiagnosticKind::CompileError(e) => {
						use rune::ast::Spanned;
						Some((e.to_string(), e.span().range().start))
					}
					_ => None,
				},
				_ => None,
			});
			match first {
				Some((message, offset)) => located("error", path, text, offset, &message),
				None => unlocated("error", &error.to_string()),
			}
			if debug_source {
				eprintln!("--- compiled source of {path}");
				eprint!("{text}");
				eprintln!("--- end of compiled source");
			}
			None
		}
	}
}

/// Where a runtime error happened, as an offset into the compiled source.
/// A failure the runtime raises carries the instruction that raised it, so
/// an error inside a called function reports that function's expression
/// rather than the line of the call.
fn fault_offset(error: &VmError, unit: &Arc<Unit>) -> Option<usize> {
	let location = error.first_location()?;
	if !Arc::ptr_eq(&location.unit, unit) {
		return None;
	}
	let instruction = location.unit.debug_info()?.instruction_at(location.ip)?;
	Some(instruction.span.range().start)
}

/// Print the value a script returned. Unit is recognised by its own type,
/// not by what it serialises to, because `None` serialises the same way and
/// is not unit.
fn show(value: &Value) {
	if matches!(value.as_type_value(), Ok(rune::runtime::TypeValue::Unit)) {
		return;
	}
	println!("{}", crate::display(value));
}

/// Print an error a script returned. A string prints as it was written;
/// anything else prints through the bounded renderer, with no formatting
/// protocol invoked through the virtual machine. A returned error is a
/// value rather than a fault at an instruction, so it carries no position.
fn show_error(value: &Value) {
	match value.borrow_string_ref() {
		Ok(text) => unplaced("error", &text),
		Err(_) => unplaced("error", &render(value, None, &Limits::default())),
	}
}

/// Run one file. Returns the process exit code, and prints everything it
/// has to say itself: diagnostics to standard error, script output to
/// standard output.
pub fn run(context: &Context, path: &str, arguments: Value, debug_source: bool) -> i32 {
	crate::host::running_a_script();
	let text = match std::fs::read_to_string(path) {
		Ok(text) => text,
		Err(error) => {
			// No source exists, so there is no line, column, or caret to give.
			unplaced("error", &format!("cannot read {path}: {error}"));
			return 1;
		}
	};
	let Some(unit) = compile(context, path, &text, debug_source) else {
		return 1;
	};
	let unit = Arc::new(unit);
	if debug_source {
		eprintln!("--- compiled source of {path}");
		eprint!("{text}");
		eprintln!("--- end of compiled source");
	}
	let runtime = match context.runtime() {
		Ok(runtime) => Arc::new(runtime),
		Err(error) => {
			unplaced("error", &error.to_string());
			return 1;
		}
	};
	let mut vm = Vm::new(runtime, unit.clone());
	// The budget `run` has always had. Without it a script that loops for
	// ever runs until something outside kills it.
	let (outcome, exhausted) = budget::with(BUDGET, || {
		let outcome = vm.call(["main"], (arguments,));
		let exhausted = !budget::acquire().take();
		(outcome, exhausted)
	})
	.call();
	let value = match outcome {
		Ok(value) => value,
		Err(error) => {
			let message = error.to_string();
			match fault_offset(&error, &unit) {
				Some(offset) => located("runtime error", path, &text, offset, &message),
				// A halt for want of budget carries no location, and saying so
				// is less useful than saying the script ran out of budget.
				None if exhausted => {
					unplaced("halted", &format!("{BUDGET} instructions exceeded"));
				}
				None => unlocated("runtime error", &message),
			}
			return 1;
		}
	};
	// A script may return its value directly or wrapped in a result.
	match rune::from_value::<std::result::Result<Value, Value>>(value.clone()) {
		Ok(Ok(value)) => {
			show(&value);
			0
		}
		Ok(Err(error)) => {
			show_error(&error);
			1
		}
		Err(_) => {
			show(&value);
			0
		}
	}
}
