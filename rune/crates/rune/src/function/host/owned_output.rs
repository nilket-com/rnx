//! A consuming host document, carried once across the existing host ABI.
#![allow(missing_docs)]
use crate as rune;
use crate::experimental_core::{error, heap::Store, value::Value};
use std::sync::Mutex;

/// A host-owned tree, never a graph of legacy runtime values. Iterators consume
/// the original containers; splitting a node must not copy the document.
#[doc(hidden)]
pub trait OwnedOutputSource: Sized + Send + Sync + 'static {
	type Array: ExactSizeIterator<Item = Self>;
	type Object: ExactSizeIterator<Item = (std::string::String, Self)>;
	fn into_node(self) -> OutputNode<Self>;
}

#[doc(hidden)]
pub enum OutputNode<S: OwnedOutputSource> {
	Unit,
	Bool(bool),
	I64(i64),
	U64(u64),
	F64(f64),
	String(std::string::String),
	Array(S::Array),
	Object(S::Object),
	/// The document's outer Result. Nested results are outside this closed tree.
	Result {
		success: bool,
		value: S,
	},
}

pub(crate) struct Materialized {
	pub(crate) result: error::Result<Value>,
	pub(crate) cleanup: Option<error::Error>,
}

crate::declare_dyn_fn! {
	struct OutputProducerVTable;
	/// Type erasure uses the same fallible, ownership-tested ABI as host handlers.
	pub(crate) struct OutputProducer {
		fn call(store: &mut Store) -> Materialized;
	}
}

/// Sealed transport only. It is consumed by the cold execution output boundary
/// and can never be admitted into a script Store as a native value.
#[doc(hidden)]
#[derive(crate::Any)]
pub struct OwnedTreeOutput {
	producer: OutputProducer,
}

impl OwnedTreeOutput {
	pub fn new<S: OwnedOutputSource>(source: S) -> crate::alloc::Result<Self> {
		let source = Mutex::new(Some(source));
		Ok(Self {
			producer: OutputProducer::new(move |store| {
				let source = match source.lock() {
					Ok(mut slot) => slot.take(),
					Err(_) => None,
				};
				match source {
					Some(source) => store.materialize_owned_output(source),
					None => Materialized {
						result: Err(contract(error::HostDynamicReason::OutputConsumed)),
						cleanup: None,
					},
				}
			})?,
		})
	}
	pub(crate) fn materialize(&self, store: &mut Store) -> Materialized {
		self.producer.call(store)
	}
}

pub(crate) fn contract(reason: error::HostDynamicReason) -> error::Error {
	error::Error::new(error::ErrorKind::Host, "owned host output").with_detail(
		error::Detail::HostDynamic {
			reason,
			type_hash: None,
		},
	)
}
