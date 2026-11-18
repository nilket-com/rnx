use super::*;
use rune::experimental_core::{
	domain::{CrossOutcome, DomainRoot, ProgramDomain},
	error::ErrorKind,
	heap::Limits,
};
use std::num::NonZeroUsize;

#[rune::function(free, path = DataFrame::empty, opaque_factory)]
fn zero_charge_factory() -> DataFrame {
	DataFrame(p::DataFrame::empty())
}

fn context(zero_charge: bool) -> rune::Context {
	let mut context = rune::Context::with_default_modules().unwrap();
	let mut module = rune::Module::with_crate("polars").unwrap();
	if zero_charge {
		module.opaque_type::<DataFrame>(|_| 0, display_fmt).unwrap();
		module.function_meta(zero_charge_factory).unwrap();
	} else {
		build(&mut module).unwrap();
	}
	context.install(module).unwrap();
	context
}

fn domain(context: &rune::Context, bytes: usize) -> ProgramDomain {
	ProgramDomain::new(
		Limits {
			bytes,
			..Limits::default()
		},
		Default::default(),
		context.execution_runtime().unwrap(),
	)
	.unwrap()
}

fn frame(domain: &mut ProgramDomain, context: &rune::Context) -> Result<DomainRoot, ErrorKind> {
	let mut sources = rune::Sources::new();
	sources
		.insert(
			rune::Source::memory(
				"pub fn main(s) { let frame = polars::DataFrame::empty(); (#{}, frame) }",
			)
			.unwrap(),
		)
		.unwrap();
	let key = domain
		.compile_session(
			context,
			&mut sources,
			&mut rune::Diagnostics::new(),
			1,
			&[],
			&[],
		)
		.unwrap();
	let main = domain.entry(&key, rune::Hash::type_hash(["main"])).unwrap();
	let state = domain.record().unwrap();
	let outcome = {
		let mut invocation = domain.start(&key, main, &[&state], 10_000).unwrap();
		match invocation.resume(NonZeroUsize::new(10_000).unwrap()) {
			CrossOutcome::Complete(root) => Ok(root),
			CrossOutcome::Failed(failure) => Err(failure.failure.error.kind),
			CrossOutcome::Limited => panic!("empty frame creation exhausted the slice"),
		}
	};
	domain.release(state).unwrap();
	outcome
}

#[test]
fn real_factory_charges_its_wrapper_and_reuses_the_allowance_after_release() {
	let reference = context(true);
	// The reference uses the same wrapper, call and envelope, with only the
	// descriptor's logical charge zeroed. The minimum includes Store overhead.
	let (mut lower, mut upper) = (1, 4096);
	while lower < upper {
		let mid = lower + (upper - lower) / 2;
		let mut domain = domain(&reference, mid);
		match frame(&mut domain, &reference) {
			Ok(root) => {
				domain.release(root).unwrap();
				upper = mid;
			}
			Err(ErrorKind::StoreCapacity) => lower = mid + 1,
			Err(error) => panic!("zero-charge control failed: {error:?}"),
		}
		domain.close().unwrap();
	}
	assert!(lower < 4096);
	let empty = DataFrame(p::DataFrame::empty());
	assert_eq!(empty.0.estimated_size(), 0);
	let bytes = lower + std::mem::size_of::<DataFrame>();

	let actual = context(false);
	let mut too_small = domain(&actual, bytes - 1);
	assert!(matches!(
		frame(&mut too_small, &actual),
		Err(ErrorKind::StoreCapacity)
	));
	too_small.collect_programs().unwrap();
	assert_eq!(too_small.live_counts(), (0, 0, 0));
	too_small.close().unwrap();

	let mut exact = domain(&actual, bytes);
	for _ in 0..2 {
		let root = frame(&mut exact, &actual).unwrap();
		assert_eq!(exact.live_counts().1, 1);
		exact.release(root).unwrap();
		exact.collect_programs().unwrap();
		assert_eq!(exact.live_counts(), (0, 0, 0));
	}
	exact.close().unwrap();
}
