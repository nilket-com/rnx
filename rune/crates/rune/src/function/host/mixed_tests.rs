//! ADR0275: sealed positions, owned outputs, and simultaneous packet lifetimes.
use super::*;
use crate::experimental_core::{
	execute::Executor,
	frontend,
	heap::{Limits, Store},
	value::Value as CoreValue,
};
use crate::runtime::FromValue;
use crate::{Context, Diagnostics, Module, Source, Sources};
use std::string::ToString;
use std::sync::{
	atomic::{AtomicUsize, Ordering},
	Arc,
};
fn compile(c: &Context, s: &str) -> crate::experimental_core::error::Result<frontend::Program> {
	let mut sources = Sources::new();
	sources.insert(Source::memory(s).unwrap()).unwrap();
	frontend::compile(&mut sources, c, &mut Diagnostics::new(), 1)
}
fn context() -> (Context, Arc<AtomicUsize>) {
	let mut c = Context::new();
	let mut m = Module::new();
	let calls = Arc::new(AtomicUsize::new(0));
	let n = calls.clone();
	m.host_function_tree_inputs(
		"pair",
		|_: &str, _: crate::Value, _: crate::Value| 0i64,
		move |label: &str, a: OwnedTreeInput, b: OwnedTreeInput| {
			n.fetch_add(1, Ordering::SeqCst);
			assert_eq!(label, "fixture");
			let count = |tree: &OwnedTreeInput| {
				let node = tree.node(0).unwrap();
				assert!(matches!(node.kind(), TreeKind::Vec));
				let mut size = 0;
				for i in 0..node.len() {
					let child = node.child(i).unwrap().1;
					let TreeKind::String(s) = tree.node(child).unwrap().kind() else {
						panic!("string")
					};
					size += s.len();
				}
				size
			};
			(count(&a) + count(&b)) as i64
		},
		None,
	)
	.unwrap();
	c.install(m).unwrap();
	(c, calls)
}
#[test]
fn both_packets_are_live_and_release_after_output() {
	let (c, calls) = context();
	let p = compile(
		&c,
		"pub fn main(){pair(\"fixture\",[\"ab\",\"c\"],[\"d\"])}",
	)
	.unwrap();
	let mut e = Executor::new().unwrap();
	e.set_host_runtime(Some(c.execution_runtime().unwrap()))
		.unwrap();
	let mut store = Store::new(Limits::default()).unwrap();
	let root = e
		.invoke(
			&p,
			&mut store,
			p.function(Hash::type_hash(["main"])).unwrap(),
			&[],
			100,
		)
		.unwrap();
	assert_eq!(root.value, CoreValue::I64(4));
	assert_eq!(calls.load(Ordering::SeqCst), 1);
	store.root_release(root).unwrap();
	assert_eq!((store.live_entries(), store.payload_bytes()), (0, 0));
}
#[test]
fn mixed_dynamic_output_requires_a_certificate_before_effects() {
	let mut c = Context::new();
	let mut m = Module::new();
	m.host_function_tree_inputs(
		"bad",
		|_: crate::Value, _: &str| crate::Value::unit(),
		|_: OwnedTreeInput, _: &str| crate::Value::unit(),
		None,
	)
	.unwrap();
	c.install(m).unwrap();
	let error = compile(&c, "pub fn main(){bad(#{},\"x\")}").err().unwrap();
	assert_eq!(
		error.refusal,
		Some(crate::experimental_core::error::Refusal::Native)
	);
}

#[test]
fn second_packet_overflow_runs_no_handler_and_unwinds_first_packet() {
	let (c, calls) = context();
	let text = "x".repeat(24000);
	let source = std::format!("pub fn main(){{let s={text:?};pair(\"fixture\",[s],[s,s])}}");
	let p = compile(&c, &source).unwrap();
	let mut e = Executor::new().unwrap();
	e.set_host_runtime(Some(c.execution_runtime().unwrap()))
		.unwrap();
	let mut store = Store::new(Limits {
		entries: 128,
		bytes: 48000,
	})
	.unwrap();
	let error = e
		.invoke(
			&p,
			&mut store,
			p.function(Hash::type_hash(["main"])).unwrap(),
			&[],
			100,
		)
		.unwrap_err();
	assert_eq!(
		error.error.kind,
		crate::experimental_core::error::ErrorKind::StoreCapacity
	);
	assert_eq!(error.error.operation, "owned host input bytes");
	assert_eq!(calls.load(Ordering::SeqCst), 0);
	assert_eq!((store.live_entries(), store.payload_bytes()), (0, 0));
}
#[test]
fn diagnostic_metadata_outlives_the_input_store() {
	let mut store = Store::new(Limits::default()).unwrap();
	let root = store
		.root_list(crate::experimental_core::value::Type::Vec, &[])
		.unwrap();
	let input = OwnedTreeInput::from_value(store.owned_host_input(root.value).unwrap()).unwrap();
	store.root_release(root).unwrap();
	drop(store);
	assert_eq!(
		input
			.node(0)
			.unwrap()
			.expected_number_error()
			.unwrap()
			.to_string(),
		"Expected number type, but found `std::vec::Vec`"
	);
}

#[test]
fn unsupported_leaf_contains_no_payload_or_child() {
	let mut store = Store::new(Limits::default()).unwrap();
	let value = store
		.root_list(crate::experimental_core::value::Type::Vec, &[])
		.unwrap();
	let result = store
		.root_variant(
			crate::experimental_core::value::Type::Result,
			true,
			Some(&value),
		)
		.unwrap();
	store.root_invalidate(&value).unwrap();
	let packet = OwnedTreeInput::from_value(store.owned_host_input(result.value).unwrap()).unwrap();
	let leaf = packet.node(0).unwrap();
	assert!(matches!(leaf.kind(), TreeKind::Unsupported(_)));
	assert_eq!(leaf.len(), 0);
	assert!(leaf.child(0).is_none());
	assert!(packet.node(1).is_none());
	store.root_release(result).unwrap();
	store.root_release(value).unwrap();
	assert_eq!(store.live_entries(), 0);
}
