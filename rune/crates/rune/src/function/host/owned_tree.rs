//! Owned inspection packet. Only the core exporter can construct a capsule.
use crate as rune;
use crate::alloc::String;
use crate::runtime::{FromValue, Inline, RuntimeError, TypeHash, Value};

#[doc(hidden)]
pub enum TreeKind {
	/// Traversal stopped before reading this node; not a serializable value.
	DepthCut,
	Scalar(Value),
	String(String),
	Vec,
	Tuple,
	Object,
	Option(bool),
	/// Already classified diagnostic leaf; never contains executable data.
	Unsupported(String),
}
#[doc(hidden)]
pub struct TreeNode {
	pub(crate) kind: TreeKind,
	pub(crate) type_info: crate::runtime::TypeInfo,
	pub(crate) edges: std::vec::Vec<(Option<String>, usize)>,
	pub(crate) cycle: bool,
	pub(crate) frontier: bool,
}
impl TreeNode {
	/// Use the stock diagnostic formatter for a non-number packet node.
	pub fn expected_number_error(&self) -> crate::alloc::Result<RuntimeError> {
		use crate::alloc::clone::TryClone;
		Ok(RuntimeError::new(
			crate::runtime::VmErrorKind::ExpectedNumber {
				actual: self.type_info.try_clone()?,
			},
		))
	}
	/// Structured diagnostic type identity, independent of Unsupported prose.
	pub fn type_info(&self) -> &crate::runtime::TypeInfo {
		&self.type_info
	}
	pub fn kind(&self) -> &TreeKind {
		&self.kind
	}
	pub fn len(&self) -> usize {
		self.edges.len()
	}
	pub fn is_empty(&self) -> bool {
		self.edges.is_empty() && !self.frontier
	}
	pub fn cycle(&self) -> bool {
		self.cycle
	}
	pub fn frontier(&self) -> bool {
		self.frontier
	}
	pub fn child(&self, n: usize) -> Option<(Option<&str>, usize)> {
		self.edges.get(n).map(|(k, v)| (k.as_deref(), *v))
	}
}
#[derive(crate::Any)]
pub(crate) struct Capsule {
	pub(crate) nodes: std::vec::Vec<TreeNode>,
	pub(crate) remaining_bytes: usize,
}

/// A sealed owned argument. Not a general Value input or a return type.
#[doc(hidden)]
pub struct OwnedTreeInput {
	repr: Repr,
}
enum Repr {
	Leaf(Value),
	Tree(Capsule),
}
impl OwnedTreeInput {
	/// Store byte allowance remaining after the owned argument's scratch charge.
	/// Scalar arguments carry no allowance.
	pub fn remaining_bytes(&self) -> Option<usize> {
		match &self.repr {
			Repr::Leaf(_) => None,
			Repr::Tree(capsule) => Some(capsule.remaining_bytes),
		}
	}

	pub fn leaf(&self) -> Option<&Value> {
		match &self.repr {
			Repr::Leaf(v) => Some(v),
			_ => None,
		}
	}
	pub fn node(&self, index: usize) -> Option<&TreeNode> {
		match &self.repr {
			Repr::Tree(c) => c.nodes.get(index),
			_ => None,
		}
	}
}
impl FromValue for OwnedTreeInput {
	fn from_value(value: Value) -> Result<Self, RuntimeError> {
		let repr =
			if matches!(
				value.as_inline(),
				Some(
					Inline::Unit
						| Inline::Bool(_) | Inline::Signed(_)
						| Inline::Unsigned(_)
						| Inline::Float(_) | Inline::Char(_)
						| Inline::Ordering(_)
				)
			) || value.type_hash() == String::HASH
			{
				// Keep the existing bridge snapshot, without making a second copy.
				Repr::Leaf(value)
			} else if value.type_hash() == Capsule::HASH {
				Repr::Tree(value.into_any_obj()?.downcast::<Capsule>()?)
			} else {
				return Err(RuntimeError::panic("broken owned-tree input route"));
			};
		Ok(Self { repr })
	}
}
impl crate::runtime::MaybeTypeOf for OwnedTreeInput {
	fn maybe_type_of() -> crate::alloc::Result<crate::compile::meta::DocType> {
		Value::maybe_type_of()
	}
}
