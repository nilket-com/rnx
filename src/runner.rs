//! `rnx run`: compile a file and its modules, then execute its `main`.
//!
//! Diagnostics have the shape the session's do, because the first port
//! measured what their absence costs. `run` compiles the file as written,
//! with no wrapper and no prelude. Instruction source identities select the
//! file whose text and path give a diagnostic its place.
//!
//! Everything rnx says goes to standard error. What the script prints goes
//! to standard output, which is a guarantee this module preserves rather
//! than introduces.
use crate::declared::Fields;
use crate::format::{display_width, render_complete_styled, terminal_safe};
use crate::presentation;
use crate::program::{Loader, Text};
use crate::session::position;
use rune::runtime::{Unit, Value, VmError};
use rune::{Context, Diagnostics, Sources, Vm};
use std::sync::Arc;

/// Instructions one script may spend unless the command line says otherwise.
/// This is the limit `run` has always had; the session's is far larger
/// because Ctrl-C can stop an input there and nothing can stop one here.
///
/// Record 0021 made it choosable because a real script needed more: the graft
/// verifier port completes 110 commits of a 1,346-commit job and halts at
/// 115. What it did not do is let a script choose — a script that could lift
/// its own ceiling would not have one — or allow the bound to be removed,
/// which waits on a file run being reliably interruptible.
pub const BUDGET: usize = 2_000_000;

/// The flag that sets the budget, taking the count as the argument after it.
/// Read only before the script path, like `--debug-source`.
pub const BUDGET_FLAG: &str = "--budget";

/// The largest number that is still a budget.
///
/// `usize::MAX` is Rune's sentinel for having no budget at all:
/// `BudgetGuard::take` returns true without decrementing when the value
/// equals it (`rune-0.14.1/src/runtime/budget.rs`, byte-identical in 0.14.2).
/// Accepting it would remove
/// the bound record 0021 decision 2 keeps, quietly and by way of an arbitrary
/// number, so it is refused. The boundary is derived from the platform's
/// `usize` rather than written as a 64-bit literal, because the sentinel is
/// whatever `usize::MAX` is here.
pub const LARGEST_BUDGET: usize = usize::MAX - 1;

/// The flag that asks for the compiled source. It is read only before the
/// script path; everything after the path belongs to the script.
pub const DEBUG_SOURCE: &str = "--debug-source";

/// Report a failure with a place in the file, in the shape the session uses.
fn located(kind: &str, path: &str, text: &str, offset: usize, message: &str) {
	let (line, column, line_text) = position(text, offset);
	// The reported column stays a position in the source, counted in
	// characters, because that is what a person or an editor uses to find the
	// place. The caret is a display artifact and is placed separately.
	eprintln!(
		"{}",
		presentation::error(&format!(
			"{kind} at {}, line {line}, column {column}: {}",
			terminal_safe(path),
			terminal_safe(message)
		))
	);
	eprintln!("  {}", terminal_safe(&line_text));
	// Placed by the display width of the escaped prefix that was actually
	// printed above: an escape, a tab and a wide character each occupy
	// something other than the one column a character count assumes.
	let prefix: String = line_text.chars().take(column.saturating_sub(1)).collect();
	eprintln!(
		"{}",
		presentation::caret(&format!(
			"  {}^",
			" ".repeat(display_width(&terminal_safe(&prefix)))
		))
	);
}

/// Report something that has no place in any source and never could: a file
/// that could not be read, or an error a script returned. Nothing is
/// invented for it, and it does not claim a position was looked for.
fn unplaced(kind: &str, message: &str) {
	eprintln!(
		"{}",
		presentation::error(&format!("{kind}: {}", terminal_safe(message)))
	);
}

/// Report a fault the runtime or the compiler raised whose place could not
/// be resolved. Unlike the above, a position was expected here, so the
/// absence is stated rather than passed over in silence.
fn unlocated(kind: &str, message: &str) {
	eprintln!(
		"{}",
		presentation::error(&format!(
			"{kind} (no source position is available for this fault): {}",
			terminal_safe(message)
		))
	);
}

fn compile(context: &Context, sources: &mut Sources, loader: &mut Loader) -> Option<Unit> {
	let mut diagnostics = Diagnostics::new();
	let built = rune::prepare(sources)
		.with_context(context)
		.with_diagnostics(&mut diagnostics)
		.with_source_loader(loader)
		.build();
	match built {
		Ok(unit) => Some(unit),
		Err(error) => {
			let first = diagnostics.diagnostics().iter().find_map(|d| match d {
				rune::diagnostics::Diagnostic::Fatal(fatal) => match fatal.kind() {
					rune::diagnostics::FatalDiagnosticKind::CompileError(e) => {
						use rune::ast::Spanned;
						Some((e.to_string(), fatal.source_id(), e.span().range().start))
					}
					_ => None,
				},
				_ => None,
			});
			match first {
				Some((message, id, offset)) => match loader.get(sources, id) {
					Some(source) => located(
						"error",
						&source.path.to_string_lossy(),
						&source.text,
						offset,
						&message,
					),
					None => unlocated("error", &message),
				},
				None => unlocated("error", &error.to_string()),
			}
			None
		}
	}
}

fn debug_sources(loader: &Loader) {
	for source in &loader.texts {
		eprintln!("--- compiled source of {}", source.path.display());
		eprint!("{}", source.text);
		eprintln!("--- end of compiled source");
	}
}

/// The source and offset of a runtime error in this compiled unit.
/// A failure the runtime raises carries the instruction that raised it, so
/// an error inside a called function reports that function's expression
/// rather than the line of the call.
fn fault_source<'a>(
	error: &VmError,
	unit: &Arc<Unit>,
	sources: &Sources,
	loader: &'a Loader,
) -> Option<(&'a Text, usize)> {
	let location = error.first_location()?;
	if !Arc::ptr_eq(&location.unit, unit) {
		return None;
	}
	let instruction = location.unit.debug_info()?.instruction_at(location.ip)?;
	Some((
		loader.get(sources, instruction.source_id)?,
		instruction.span.range().start,
	))
}

/// Print the value a script returned, and report whether it could be shown.
/// Unit is recognised by its own type, not by what it serialises to, because
/// `None` serialises the same way and is not unit.
///
/// A value this cannot render is a failure of the run rather than a value:
/// the reason goes to standard error and the status is 1. Printed on standard
/// output it would be indistinguishable from the value the script meant to
/// return, which is the shape of defect this record exists to remove. Which
/// shapes those are, and how they ought to render, is the renderer question
/// decision 4 defers.
fn show(value: &Value, fields: &Fields) -> i32 {
	if is_unit(value) {
		return 0;
	}
	match render_complete_styled(value, Some(fields), presentation::stdout()) {
		Ok(text) => {
			println!("{text}");
			0
		}
		Err(reason) => {
			unplaced("error", &cannot_show(&reason));
			1
		}
	}
}

/// Why a value could not be shown, in one sentence both entry points use,
/// so a caller cannot tell them apart by the wording of a failure.
pub fn cannot_show(reason: &str) -> String {
	format!("the value cannot be shown: {reason}")
}

/// Whether a returned value is nothing at all. A script or an expression that
/// produced no value should print no value, and both entry points ask here so
/// that one of them cannot start printing `()` while the other does not.
pub fn is_unit(value: &Value) -> bool {
	matches!(value.as_type_value(), Ok(rune::runtime::TypeValue::Unit))
}

/// Report an error a script or an expression returned. Both entry points come
/// here, so neither can grow a policy of its own; what the text looks like
/// and what it is allowed to cost are `format::error_text`'s. A returned
/// error is a value rather than a fault at an instruction, so it carries no
/// position, and no formatting protocol is invoked through the virtual
/// machine to produce it.
pub fn report_error(value: &Value, fields: Option<&Fields>) {
	unplaced("error", &crate::format::error_text(value, fields));
}

/// Run one file. Returns the process exit code, and prints everything it
/// has to say itself: diagnostics to standard error, script output to
/// standard output.
pub fn run(
	context: &Context,
	path: &str,
	arguments: Value,
	debug_source: bool,
	budget: usize,
) -> i32 {
	crate::host::running_a_script();
	let mut loader = Loader::new();
	let source = match loader.entry(std::path::Path::new(path)) {
		Ok(source) => source,
		Err(error) => {
			unplaced("error", &error);
			return 1;
		}
	};
	let mut sources = Sources::new();
	if sources.insert(source).is_err() {
		unplaced("error", "the source could not be held for compilation");
		return 1;
	}
	let compiled = compile(context, &mut sources, &mut loader);
	if debug_source {
		debug_sources(&loader);
	}
	let Some(unit) = compiled else {
		return 1;
	};
	let unit = Arc::new(unit);
	let runtime = match context.runtime() {
		Ok(runtime) => Arc::new(runtime),
		Err(error) => {
			unplaced("error", &error.to_string());
			return 1;
		}
	};
	let mut vm = Vm::new(runtime, unit.clone());
	// The budget, from the command line or the default. Without one a script
	// that loops for ever runs until something outside kills it. Record 0032
	// runs every file through the one driver that copes with a `main` that
	// awaits and one that does not, because which it is cannot be read off
	// the source: an alias can make an async function the entry point.
	let driver = match crate::execute::Runtime::new() {
		Ok(driver) => driver,
		Err(error) => {
			unplaced("error", &format!("cannot start the runtime: {error}"));
			return 1;
		}
	};
	let outcome = crate::execute::drive_async(
		&driver,
		&mut vm,
		["main"],
		(arguments,),
		budget,
		// A script that ran to its own end has ended, whatever arrived while
		// it ran; record 0023 leaves a cancelled child to the script.
		crate::execute::WhenInterrupted::Finish,
	);
	drop(vm);
	// Runtime drop cancels async work without waiting on started system DNS.
	// Cleanup must not replace a completed script's own exit status.
	drop(driver);
	let value = match outcome {
		crate::execute::Outcome::Complete(value) => value,
		crate::execute::Outcome::Interrupted => {
			eprintln!("interrupted");
			return 130;
		}
		// A halt for want of budget carries no location, and saying so
		// is less useful than saying the script ran out of budget.
		crate::execute::Outcome::Budget => {
			// The reader of this line is the person who can raise it, so
			// it names the flag. It does not offer to remove the bound,
			// because the bound cannot be removed.
			unplaced(
				"halted",
				&format!("{budget} instructions exceeded; {BUDGET_FLAG} N raises it"),
			);
			return 1;
		}
		crate::execute::Outcome::Yielded => {
			unlocated("runtime error", "unexpected yield");
			return 1;
		}
		crate::execute::Outcome::Failed { error, exhausted } => {
			// Rune reports a missing method by hash. Record 0014 recovers
			// the name when it can be proved, and leaves the message alone
			// when it cannot.
			let message = error.to_string();
			let origin = fault_source(&error, &unit, &sources, &loader);
			let message = named_fault(message, origin);
			match origin {
				Some((source, offset)) => located(
					"runtime error",
					&source.path.to_string_lossy(),
					&source.text,
					offset,
					&message,
				),
				None => unlocated("runtime error", &message),
			}
			// Said after the error and never instead of it: the budget was
			// spent to its last instruction at the same moment, which is the
			// reason for a halt raised inside an async function and is
			// context for anything else.
			if exhausted {
				unplaced(
					"halted",
					&format!(
						"the budget of {budget} instructions was exhausted at that point; {BUDGET_FLAG} N raises it"
					),
				);
			}
			return 1;
		}
	};
	// The program's loaded declarations are candidates for its returned value.
	// They are only candidates: each is verified against the value, because a
	// name and a shape are two different claims.
	let mut fields = Fields::default();
	for source in &loader.texts {
		crate::declared::into_fields(&source.text, &mut fields);
	}
	match returned(&value) {
		Ok(value) => show(&value, &fields),
		Err(error) => {
			report_error(&error, Some(&fields));
			1
		}
	}
}

/// What a returned value means: something to show, or a failure to report.
///
/// A script may return its value directly or wrapped in a result, and both
/// entry points read that the same way, so a failure is a failure whichever
/// one produced it. The shape of the value is what decides; nothing inside it
/// is inspected, so a child that ran and failed is still a call that worked.
pub fn returned(value: &Value) -> std::result::Result<Value, Value> {
	match rune::from_value::<std::result::Result<Value, Value>>(value.clone()) {
		Ok(inner) => inner,
		Err(_) => Ok(value.clone()),
	}
}

// The origin is the only candidate source, including when attribution fails.
fn named_fault(message: String, origin: Option<(&Text, usize)>) -> String {
	origin
		.and_then(|(source, _)| crate::method::named(&message, &source.text))
		.unwrap_or(message)
}

#[cfg(test)]
mod module_tests {
	use super::*;
	#[test]
	fn unavailable_origin_keeps_the_hash_despite_a_matching_entry_candidate() {
		let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
			.join("tests/fixtures/modules/fault/main.rn");
		let mut loader = Loader::new();
		let mut sources = Sources::new();
		sources.insert(loader.entry(&path).unwrap()).unwrap();
		let context = Context::with_default_modules().unwrap();
		let unit = Arc::new(compile(&context, &mut sources, &mut loader).unwrap());
		let mut vm = Vm::new(Arc::new(context.runtime().unwrap()), unit.clone());
		let error = vm.call(["main"], ((),)).unwrap_err();
		let original = error.to_string();
		assert!(original.contains("Missing instance function"));
		assert!(crate::method::named(&original, &loader.texts[0].text).is_some());
		let origin = fault_source(&error, &unit, &sources, &loader).unwrap();
		assert!(origin.0.path.ends_with("broken.rn"));
		assert!(named_fault(original.clone(), Some(origin)).contains("no method `missing`"));
		let empty = Sources::new();
		assert_eq!(
			named_fault(
				original.clone(),
				fault_source(&error, &unit, &empty, &loader)
			),
			original
		);
		let different = Arc::new(
			rune::prepare(&mut Sources::new())
				.with_context(&context)
				.build()
				.unwrap(),
		);
		assert!(fault_source(&error, &different, &sources, &loader).is_none());
	}
}
