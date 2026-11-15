//! Minimal document and carrier controls; no parser, compiler or std modules.
use super::*;
use crate::experimental_core::error::{Detail, HostDynamicReason};
use crate::function::host::OwnedTreeOutput;
use std::cell::RefCell;
use std::sync::{
	atomic::{AtomicUsize, Ordering},
	Arc,
};

std::thread_local! {
	static TRACE: RefCell<std::vec::Vec<&'static str>> = const { RefCell::new(std::vec::Vec::new()) };
}
pub(super) fn allocation(name: &'static str) {
	TRACE.with(|t| t.borrow_mut().push(name));
}

#[derive(Clone)]
enum Tree {
	Root(bool, std::boxed::Box<Tree>),
	Text(std::string::String),
	Int(i64),
	Array(std::vec::Vec<Tree>),
	Object(std::vec::Vec<(std::string::String, Tree)>),
	Panic,
}
impl OwnedOutputSource for Tree {
	type Array = std::vec::IntoIter<Self>;
	type Object = std::vec::IntoIter<(std::string::String, Self)>;
	fn into_node(self) -> OutputNode<Self> {
		match self {
			Self::Root(success, value) => OutputNode::Result {
				success,
				value: *value,
			},
			Self::Text(text) => OutputNode::String(text),
			Self::Int(n) => OutputNode::I64(n),
			Self::Array(v) => OutputNode::Array(v.into_iter()),
			Self::Object(v) => OutputNode::Object(v.into_iter()),
			Self::Panic => panic!("planted document producer panic"),
		}
	}
}
fn root(tree: Tree) -> Tree {
	Tree::Root(true, std::boxed::Box::new(tree))
}
fn document() -> Tree {
	root(Tree::Object(
		(0..40)
			.map(|i| {
				(
					std::format!("key-{i:03}"),
					Tree::Array(std::vec![Tree::Text("owned child".into()), Tree::Int(i)]),
				)
			})
			.collect(),
	))
}
fn balanced(store: &mut Store, live: usize, bytes: usize) {
	assert_eq!((store.live_entries(), store.payload_bytes()), (live, bytes));
	let fresh = store.string("reusable").unwrap();
	store.release(fresh).unwrap();
	assert_eq!((store.live_entries(), store.payload_bytes()), (live, bytes));
}
fn release_output(store: &mut Store, output: Materialized) {
	assert!(output.cleanup.is_none());
	store.release(output.result.unwrap()).unwrap();
}
fn reason(error: Error, expected: HostDynamicReason) {
	assert_eq!(error.kind, ErrorKind::Host);
	assert!(matches!(error.detail, Detail::HostDynamic { reason, .. } if reason == expected));
}

#[test]
fn consuming_document_owns_children_order_and_index() {
	let mut store = Store::new(Limits::default()).unwrap();
	let output = store.materialize_owned_output(document());
	assert!(output.cleanup.is_none());
	let value = output.result.unwrap();
	let (_, success, object) = store.variant_parts(value).unwrap();
	assert!(success);
	let object = object.unwrap();
	assert_eq!(store.len(object).unwrap(), 40);
	let child = store.field(object, "key-039").unwrap();
	let text = store.get(child, 0).unwrap();
	assert_eq!(store.text(text).unwrap(), "owned child");
	assert_eq!(store.get(child, 1).unwrap(), Value::I64(39));
	store.release(text).unwrap();
	store.release(child).unwrap();
	store.release(value).unwrap();
	balanced(&mut store, 0, 0);
}

#[test]
fn list_adopts_its_checked_buffer_and_charges_actual_capacity() {
	let mut children = Vec::try_with_capacity(3).unwrap();
	children.try_push(Value::I64(7)).unwrap();
	let pointer = children.as_ptr();
	let allocated = bytes::<Value>(children.capacity()).unwrap();
	let mut store = Store::new(Limits {
		entries: 4,
		bytes: allocated + size_of::<Value>(),
	})
	.unwrap();
	let mut builder = Builder {
		store: &mut store,
		owners: Vec::new(),
		scratch: allocated,
		cleanup: None,
	};
	let mut frame = Frame::<Tree>::Array {
		values: std::vec::Vec::new().into_iter(),
		children,
		len: 1,
	};
	let Progress::Complete(value) = builder.advance(&mut frame, None).unwrap() else {
		panic!("not completed");
	};
	let Value::Heap(handle) = value else {
		panic!("not a heap value");
	};
	let Payload::List { values, .. } = builder.store.payload(handle).unwrap() else {
		panic!("not a list");
	};
	assert_eq!(
		values.as_ptr(),
		pointer,
		"adoption must not allocate/copy another list"
	);
	assert_eq!(values.capacity(), 3);
	assert_eq!(builder.store.payload_bytes(), allocated);
	assert!(builder.release().is_none());
	assert_eq!(
		(builder.store.live_entries(), builder.store.payload_bytes()),
		(0, 0)
	);
}

struct Probe {
	root: bool,
	drops: Arc<AtomicUsize>,
}
impl Drop for Probe {
	fn drop(&mut self) {
		self.drops.fetch_add(1, Ordering::SeqCst);
	}
}
impl OwnedOutputSource for Probe {
	type Array = std::iter::Empty<Self>;
	type Object = std::iter::Empty<(std::string::String, Self)>;
	fn into_node(self) -> OutputNode<Self> {
		if self.root {
			OutputNode::Result {
				success: true,
				value: Self {
					root: false,
					drops: self.drops.clone(),
				},
			}
		} else {
			OutputNode::I64(7)
		}
	}
}
#[test]
fn carrier_consumes_once_and_drops_unconsumed_source() {
	let drops = Arc::new(AtomicUsize::new(0));
	let packet = OwnedTreeOutput::new(Probe {
		root: true,
		drops: drops.clone(),
	})
	.unwrap();
	drop(packet);
	assert_eq!(drops.load(Ordering::SeqCst), 1);
	let packet = OwnedTreeOutput::new(Probe {
		root: true,
		drops: drops.clone(),
	})
	.unwrap();
	let mut store = Store::new(Limits::default()).unwrap();
	let output = packet.materialize(&mut store);
	release_output(&mut store, output);
	assert_eq!(drops.load(Ordering::SeqCst), 3);
	let twice = packet.materialize(&mut store);
	reason(twice.result.unwrap_err(), HostDynamicReason::OutputConsumed);
	assert!(twice.cleanup.is_none());
	drop(packet);
	assert_eq!(drops.load(Ordering::SeqCst), 3);
	balanced(&mut store, 0, 0);
	let packet = OwnedTreeOutput::new(Probe {
		root: true,
		drops: drops.clone(),
	})
	.unwrap();
	let mut bounded = Store::new(Limits {
		entries: 2,
		bytes: 0,
	})
	.unwrap();
	assert_eq!(
		packet.materialize(&mut bounded).result.unwrap_err().kind,
		ErrorKind::StoreCapacity
	);
	reason(
		packet.materialize(&mut bounded).result.unwrap_err(),
		HostDynamicReason::OutputConsumed,
	);
	drop(packet);
	assert_eq!(drops.load(Ordering::SeqCst), 5);
	assert_eq!((bounded.live_entries(), bounded.payload_bytes()), (0, 0));
}

#[test]
fn malformed_and_panicking_producers_leave_no_partial_values() {
	let mut store = Store::new(Limits::default()).unwrap();
	let held = store.string("held alias").unwrap();
	store.retain(held).unwrap();
	let baseline = (store.live_entries(), store.payload_bytes());
	let mut deep = Tree::Int(0);
	for _ in 0..131 {
		deep = Tree::Array(std::vec![deep]);
	}
	for tree in [
		Tree::Int(0),
		root(root(Tree::Int(0))),
		root(deep),
		Tree::Root(false, std::boxed::Box::new(Tree::Int(0))),
		root(Tree::Object(std::vec![
			("a".into(), Tree::Text("one".into())),
			("a".into(), Tree::Int(2))
		])),
		root(Tree::Object(
			(0..32)
				.map(|_| ("same".into(), Tree::Text("child".into())))
				.collect(),
		)),
	] {
		let output = store.materialize_owned_output(tree);
		assert!(output.cleanup.is_none());
		reason(output.result.unwrap_err(), HostDynamicReason::OutputTree);
		balanced(&mut store, baseline.0, baseline.1);
	}
	let output = store.materialize_owned_output(root(Tree::Array(std::vec![
		Tree::Text("before panic".into()),
		Tree::Panic
	])));
	assert!(matches!(
		output.result.unwrap_err().detail,
		Detail::HostPanicked
	));
	assert!(output.cleanup.is_none());
	assert_eq!(store.text(held).unwrap(), "held alias");
	balanced(&mut store, baseline.0, baseline.1);
	store.release(held).unwrap();
	store.release(held).unwrap();
}

#[test]
#[cfg_attr(
	miri,
	ignore = "native exhaustive allocator sweep; focused Miri covers owner transitions"
)]
fn every_conversion_allocation_and_budget_failure_rolls_back() {
	// Prebuild the parsed source outside the Rune allocation limiter. Sweep every
	// byte of real conversion allowance until its first successful allocation path.
	let mut failures = std::collections::BTreeMap::new();
	let mut passed = false;
	for fixture in [
		document(),
		root(Tree::Object(
			(0..40)
				.map(|i| {
					(
						std::format!("long-key-{i:03}-{}", "k".repeat(200)),
						Tree::Int(i),
					)
				})
				.collect(),
		)),
	] {
		for budget in 0..100_000 {
			let mut store = Store::new(Limits::default()).unwrap();
			let held = store.string("held").unwrap();
			store.retain(held).unwrap();
			let baseline = (store.live_entries(), store.payload_bytes());
			let source = fixture.clone();
			TRACE.with(|t| t.borrow_mut().clear());
			let output =
				crate::alloc::limit::with(budget, || store.materialize_owned_output(source)).call();
			assert!(output.cleanup.is_none());
			match output.result {
				Ok(value) => {
					store.release(value).unwrap();
					passed = true;
				}
				Err(error) => {
					assert_eq!(error.kind, ErrorKind::Allocation);
					let trace = TRACE.with(|t| t.borrow().clone());
					failures
						.entry(trace.last().copied().unwrap())
						.or_insert(budget);
				}
			}
			balanced(&mut store, baseline.0, baseline.1);
			assert_eq!(store.text(held).unwrap(), "held");
			store.release(held).unwrap();
			store.release(held).unwrap();
			if passed {
				break;
			}
		}
		assert!(passed);
		passed = false;
	}
	for boundary in ["owner", "buffer", "string", "key", "index", "traversal"] {
		assert!(
			failures.contains_key(boundary),
			"unreached failure: {boundary}; {failures:?}"
		);
	}
	for budget in [0, 1, 256, 2048] {
		let mut store = Store::new(Limits {
			entries: 256,
			bytes: budget,
		})
		.unwrap();
		let output = store.materialize_owned_output(document());
		assert_eq!(output.result.unwrap_err().kind, ErrorKind::StoreCapacity);
		assert!(output.cleanup.is_none());
		assert_eq!((store.live_entries(), store.payload_bytes()), (0, 0));
	}
	// 80 child entries + object + final Result: fail immediately, midway and
	// at the final Result admission, even with ample byte/allocation allowance.
	for entries in [1, 40, 81] {
		let mut store = Store::new(Limits {
			entries,
			bytes: 1_000_000,
		})
		.unwrap();
		let output = store.materialize_owned_output(document());
		assert_eq!(output.result.unwrap_err().kind, ErrorKind::StoreCapacity);
		assert!(output.cleanup.is_none());
		balanced(&mut store, 0, 0);
	}
}
