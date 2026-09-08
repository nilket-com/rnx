//! A standalone reproducer for an isolation defect in Rune 0.14.1, kept as a
//! test so that it also tells us when upstream fixes it.
//!
//! Calling a function value made by one unit from a second unit corrupts the
//! stack when the two virtual machines share a runtime context. In
//! `runtime/function.rs`, `FnOffset::call_with_vm` decides isolation from the
//! context alone:
//!
//! ```ignore
//! let same_unit = matches!(self.call, Call::Immediate if vm.is_same_unit(&self.unit));
//! let same_context = matches!(self.call, Call::Immediate if vm.is_same_context(&self.context));
//! vm_try!(vm.push_call_frame(self.offset, addr, args, Isolated::new(!same_context), out));
//! if same_context && same_unit { return VmResult::Ok(None); }
//! ```
//!
//! while the fast path that keeps running on the current frame requires both
//! the context and the unit to match. With a runtime context built per unit
//! the two conditions never disagree, because the contexts differ whenever
//! the units do. With one shared context they come apart at "same context,
//! different unit": the frame is not isolated, the unit switches anyway, and
//! the callee reads stack slots laid out for the caller.
//!
//! This costs rnx the saving recorded in
//! `plans/0006_what_a_retained_value_pins.md`, so the session builds a
//! runtime context per input instead.
use rune::runtime::{RuntimeContext, Value};
use rune::{Context, Diagnostics, Source, Sources, Unit, Vm};
use std::sync::Arc;

fn compile(context: &Context, source: &str) -> Arc<Unit> {
	let mut sources = Sources::new();
	sources.insert(Source::memory(source).unwrap()).unwrap();
	let mut diagnostics = Diagnostics::new();
	let unit = rune::prepare(&mut sources)
		.with_context(context)
		.with_diagnostics(&mut diagnostics)
		.build()
		.expect("the sources compile");
	Arc::new(unit)
}

/// Make a function value in one unit and call it from another. `share` says
/// whether the two virtual machines get the same runtime context.
fn call_across_units(share: bool) -> Result<i64, String> {
	let context = Context::with_default_modules().unwrap();
	let maker = compile(&context, "pub fn main() { || 41 + 1 }");
	let caller = compile(&context, "pub fn main(f) { f() }");

	let first = Arc::new(context.runtime().unwrap());
	let second: Arc<RuntimeContext> = if share {
		first.clone()
	} else {
		Arc::new(context.runtime().unwrap())
	};

	let mut vm = Vm::new(first, maker);
	let function: Value = vm.call(["main"], ()).expect("the maker runs");
	let mut vm = Vm::new(second, caller);
	match vm.call(["main"], (function,)) {
		Ok(value) => Ok(rune::from_value(value).expect("an integer")),
		Err(error) => Err(error.to_string()),
	}
}

#[test]
fn a_function_value_crosses_units_when_the_contexts_differ() {
	// The control. This is what rnx does, and it is why rnx builds a runtime
	// context per input.
	assert_eq!(call_across_units(false), Ok(42));
}

#[test]
fn a_function_value_crosses_units_wrongly_when_the_context_is_shared() {
	// The defect. When this starts returning Ok(42), upstream has changed
	// something here. That is a trigger to revisit record 0006, which measured
	// the saving at about 463 KB for every retained function value; it is not
	// on its own permission to share a runtime context. One reproducer passing
	// says this call shape works, not that the session's semantics survive.
	let result = call_across_units(true);
	assert!(
		result.is_err(),
		"upstream may be fixed: sharing the runtime context now returns {result:?}. \
		 Revisit plans/0006_what_a_retained_value_pins.md, and re-run the full \
		 session-semantic gates of records 0001 and 0002 before enabling sharing."
	);
	let message = result.unwrap_err();
	// The corruption shows differently depending on how the two units are
	// shaped: this minimal pair reports an out-of-bounds instruction pointer,
	// while the session reported an out-of-bounds stack entry. Both are the
	// callee running against the caller's frame.
	assert!(
		message.contains("out-of-bounds"),
		"a different failure than the one recorded: {message}"
	);
}
