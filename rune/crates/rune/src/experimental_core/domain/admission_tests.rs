use super::*;
use crate as rune;
use crate::experimental_core::{
	error::Refusal,
	execute::PrepareError,
	heap::Store,
	value::{Scalar, Type},
};
use crate::{Hash, Module};
use std::sync::{
	atomic::{AtomicUsize, Ordering},
	Arc as StdArc,
};

fn context() -> (Context, StdArc<AtomicUsize>) {
	let mut c = Context::new();
	let effects = StdArc::new(AtomicUsize::new(0));
	let copy = effects.clone();
	let mut module = Module::with_crate("entry269").unwrap();
	module
		.host_function("touch", move || {
			copy.fetch_add(1, Ordering::SeqCst);
		})
		.unwrap();
	c.install(module).unwrap();
	(c, effects)
}
fn domain(c: &Context) -> ProgramDomain {
	ProgramDomain::new(
		Default::default(),
		Default::default(),
		c.execution_runtime().unwrap(),
	)
	.unwrap()
}
fn compile(d: &mut ProgramDomain, c: &Context, s: &str) -> ProgramKey {
	let mut sources = Sources::new();
	sources.insert(crate::Source::memory(s).unwrap()).unwrap();
	d.compile_insert(c, &mut sources, &mut Diagnostics::new(), 0)
		.unwrap()
}
fn finish(d: &mut ProgramDomain, key: &ProgramKey, args: &[&DomainRoot]) -> DomainRoot {
	let entry = d.entry(key, Hash::type_hash(["main"])).unwrap();
	let mut invocation = d.start(key, entry, args, 10000).unwrap();
	loop {
		match invocation.resume(core::num::NonZeroUsize::new(8).unwrap()) {
			CrossOutcome::Limited => {}
			CrossOutcome::Complete(r) => return r,
			CrossOutcome::Failed(e) => panic!("{}", e.message()),
		}
	}
}

#[test]
fn successful_start_prepares_once_before_any_effect() {
	let (c, effects) = context();
	let mut d = domain(&c);
	let key = compile(&mut d, &c, "pub fn main(x){entry269::touch();x}");
	let root = d.i64(7).unwrap();
	Store::format_admission_probe(false);
	let entry = d.entry(&key, Hash::type_hash(["main"])).unwrap();
	let output = {
		let mut call = d.start(&key, entry, &[&root], 1000).unwrap();
		assert_eq!(Store::format_admission_probe(false), 1);
		assert_eq!(effects.load(Ordering::SeqCst), 0);
		loop {
			match call.resume(core::num::NonZeroUsize::new(8).unwrap()) {
				CrossOutcome::Limited => {}
				CrossOutcome::Complete(r) => break r,
				CrossOutcome::Failed(e) => panic!("{}", e.message()),
			}
		}
	};
	assert_eq!(effects.load(Ordering::SeqCst), 1);
	assert_eq!(d.scalar(&output).unwrap(), Some(Scalar::I64(7)));
	assert!(
		!d.programs
			.iter()
			.find(|m| m.program.code.id() == key.code)
			.unwrap()
			.leased
	);
}

#[test]
fn failed_prepare_keeps_lease_roots_and_is_retryable_without_effects() {
	let (c, effects) = context();
	let mut d = domain(&c);
	let key = compile(&mut d, &c, "pub fn main(x){entry269::touch();x}");
	let root = d.i64(7).unwrap();
	let entry = d.entry(&key, Hash::type_hash(["main"])).unwrap();
	Store::format_admission_probe(true);
	assert!(
		matches!(d.start(&key,entry,&[&root],1000),Err(StartError::Prepare(PrepareError::Invalid(e))) if e.kind==ErrorKind::Allocation)
	);
	assert_eq!(Store::format_admission_probe(false), 1);
	assert_eq!(effects.load(Ordering::SeqCst), 0);
	assert_eq!(d.scalar(&root).unwrap(), Some(Scalar::I64(7)));
	assert_eq!(d.collect_programs().unwrap().leases, 1);
	let output = finish(&mut d, &key, &[&root]);
	assert_eq!(effects.load(Ordering::SeqCst), 1);
	assert_eq!(d.scalar(&output).unwrap(), Some(Scalar::I64(7)));
}

#[test]
fn deep_formatter_refusal_keeps_lease_and_untouched_arguments() {
	let (c, effects) = context();
	let mut d = domain(&c);
	// The unmarked effective native key can also be a nominal DISPLAY_FMT key.
	let effective = Hash::type_hash(crate::item!(::entry269::touch));
	assert!(c.lookup_function(effective).is_some());
	let hash = Hash::associated_function(effective, &crate::runtime::Protocol::DISPLAY_FMT);
	assert!(!c.format_snapshot().unwrap().permits(hash));
	let nominal = super::super::nominal::intern(hash.into_inner(), "Entry269Denied").unwrap();
	let bad = d.store.root_record(Some(nominal), &[]).unwrap();
	let inner = d.store.root_list(Type::Vec, &[&bad]).unwrap();
	d.store.root_release(bad).unwrap();
	let slot = d.reserve_root().unwrap();
	let root = d.publish(slot, inner);
	let key = compile(&mut d, &c, "pub fn main(x){entry269::touch();x}");
	let entry = d.entry(&key, Hash::type_hash(["main"])).unwrap();
	let before = d.live_counts();
	Store::format_admission_probe(false);
	assert!(matches!(
		d.start(&key, entry, &[&root], 1000),
		Err(StartError::Prepare(PrepareError::Refused(Refusal::Entry)))
	));
	assert_eq!(Store::format_admission_probe(false), 1);
	assert_eq!(effects.load(Ordering::SeqCst), 0);
	assert_eq!(d.live_counts(), before);
	assert_eq!(d.collect_programs().unwrap().leases, 1);
	let view = d.view(&root, Default::default()).unwrap();
	assert!(view
		.root()
		.unwrap()
		.child(0)
		.unwrap()
		.nominal()
		.unwrap()
		.is_some());
}

#[test]
fn callback_from_an_older_member_records_new_struct_before_next_entry() {
	let (c, _) = context();
	let mut d = domain(&c);
	let maker = compile(&mut d, &c, "struct S {n} pub fn main(){||S{n:7}}");
	let callback = finish(&mut d, &maker, &[]);
	let caller = compile(&mut d, &c, "pub fn main(f){f()}");
	let result = finish(&mut d, &caller, &[&callback]);
	let reader = compile(&mut d, &c, "pub fn main(x){x.n}");
	Store::format_walk_probe();
	Store::format_admission_probe(false);
	let n = finish(&mut d, &reader, &[&result]);
	assert_eq!(d.scalar(&n).unwrap(), Some(Scalar::I64(7)));
	assert_eq!(Store::format_admission_probe(false), 1);
	assert_eq!(Store::format_walk_probe(), (0, 0, 1));
}
