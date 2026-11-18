use std::num::NonZeroUsize;

use rune::experimental_core::{
	execute::{Failure, PreparedInvocation, SliceOutcome},
	value::Root,
};

use crate::execute::{Runtime, host_wait_interrupted};

const SLICE: NonZeroUsize = NonZeroUsize::new(1024).unwrap();

/// A file entry uses one budget and one loan of its runtime. `None`
/// means the invocation was interrupted before it settled.
pub(super) fn run(
	driver: &mut Runtime,
	prepared: PreparedInvocation<'_>,
	budget: usize,
) -> Result<Option<Result<Root, Failure>>, String> {
	driver.with_host_wait(|| drive(prepared, budget, crate::host::interrupted))
}

fn drive(
	prepared: PreparedInvocation<'_>,
	budget: usize,
	mut interrupted: impl FnMut() -> bool,
) -> Option<Result<Root, Failure>> {
	let mut invocation = match prepared.start_sliced(budget) {
		Ok(invocation) => invocation,
		Err(error) => return Some(Err(error)),
	};
	loop {
		if interrupted() {
			let _ = invocation.cancel();
			return None;
		}
		match invocation.resume(SLICE) {
			SliceOutcome::Limited => {}
			SliceOutcome::Complete(root) => return Some(Ok(root)),
			SliceOutcome::Failed(_) if host_wait_interrupted() => return None,
			SliceOutcome::Failed(error) => return Some(Err(error)),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rune::experimental_core::{
		error::{ErrorKind, Refusal},
		execute::Executor,
		frontend,
		heap::{Limits, Store},
	};
	use rune::{Context, Diagnostics, Hash, Source, Sources};

	fn sources(text: &str) -> Sources {
		let mut sources = Sources::new();
		sources.insert(Source::memory(text).unwrap()).unwrap();
		sources
	}

	#[test]
	fn the_file_policy_is_required_and_does_not_admit_a_private_async_main() {
		let context = Context::with_default_modules().unwrap();
		let mut diagnostics = Diagnostics::new();
		let error = frontend::compile(
			&mut sources("pub async fn main() { 7 }"),
			&context,
			&mut diagnostics,
			1,
		)
		.err()
		.unwrap();
		assert_eq!(error.refusal, Some(Refusal::AsyncFunction));

		let mut diagnostics = Diagnostics::new();
		let error = frontend::compile_file(
			&mut sources("async fn main() { 7 }"),
			&context,
			&mut diagnostics,
			1,
		)
		.err()
		.unwrap();
		assert_eq!(error.refusal, Some(Refusal::AsyncFunction));
	}

	#[test]
	fn a_settled_value_or_unrelated_error_wins_over_a_late_interrupt() {
		let context = Context::with_default_modules().unwrap();
		for (source, fails) in [
			("pub async fn main() { 7 }", false),
			("pub async fn main() { 1 / 0 }", true),
		] {
			let (program, mode) =
				frontend::compile_file(&mut sources(source), &context, &mut Diagnostics::new(), 1)
					.unwrap();
			assert!(matches!(mode, frontend::FileEntry::Async));
			let entry = program.function(Hash::type_hash(["main"])).unwrap();
			let mut store = Store::new(Limits::default()).unwrap();
			let mut executor = Executor::new().unwrap();
			let prepared = executor.prepare(&program, &mut store, entry, &[]).unwrap();
			let mut checks = 0;
			let outcome = drive(prepared, 1024, || {
				checks += 1;
				checks > 1
			});
			match (outcome, fails) {
				(Some(Ok(root)), false) => store.root_release(root).unwrap(),
				(Some(Err(error)), true) => assert_eq!(error.error.kind, ErrorKind::DivisionByZero),
				_ => panic!("the settled outcome was lost for {source}"),
			}
			assert_eq!(checks, 1, "{source}");
			store.close().unwrap();
		}
	}
}
