//! Iterative, occurrence-counted export for the sealed owned input route.
use super::{Payload, Store};
use crate::experimental_core::{
	error::{Error, ErrorKind, Result},
	value::{Type, Value},
};
use crate::function::host::owned_tree::{Capsule, TreeKind, TreeNode};
use crate::{alloc::String, runtime};
use core::fmt::Write;

struct Budget {
	used: usize,
	limit: usize,
}
impl Budget {
	fn charge(&mut self, n: usize) -> Result<()> {
		let used = self.used.checked_add(n).ok_or_else(limit)?;
		if used > self.limit {
			return Err(limit());
		}
		self.used = used;
		Ok(())
	}
	fn room<T>(&mut self, v: &mut std::vec::Vec<T>, additional: usize) -> Result<()> {
		let need = v.len().checked_add(additional).ok_or_else(limit)?;
		if need <= v.capacity() {
			return Ok(());
		}
		let target = need.max(v.capacity().saturating_mul(2));
		let old = v.capacity() * size_of::<T>();
		let bytes = target.checked_mul(size_of::<T>()).ok_or_else(limit)?;
		self.charge(bytes)?; // Count old + replacement during reallocation.
		v.try_reserve_exact(target - v.len())
			.map_err(|_| Error::new(ErrorKind::Allocation, "owned host input"))?;
		let actual = v.capacity() * size_of::<T>();
		if actual > bytes {
			self.charge(actual - bytes)?;
		}
		self.used -= old;
		Ok(())
	}
	fn text(&mut self, s: &str) -> Result<String> {
		self.charge(s.len())?;
		Ok(String::try_from(s)?)
	}
}
fn invalid() -> Error {
	Error::new(ErrorKind::InvalidCode, "owned host input invariant")
}
fn description_limit() -> Error {
	Error::new(
		ErrorKind::Unsupported,
		"owned host input diagnostic exceeds 1024 bytes",
	)
}
fn limit() -> Error {
	Error::new(ErrorKind::StoreCapacity, "owned host input bytes")
}
struct Frame {
	value: Value,
	node: usize,
	next: usize,
	len: usize,
	depth: usize,
}
fn track(value: Value, store: &Store) -> Result<bool> {
	Ok(matches!(
		store.host_tag(value)?,
		(Type::Vec | Type::Tuple | Type::Object, _)
	))
}
fn child(store: &Store, value: Value, index: usize) -> Result<(Option<&str>, Value)> {
	let Value::Heap(h) = value else {
		return Err(Error::new(ErrorKind::InvalidCode, "owned input child"));
	};
	Ok(match store.payload(h)? {
		Payload::List { values, .. } => (None, *values.get(index).ok_or_else(invalid)?),
		Payload::Record { fields, .. } => {
			let (k, v) = fields.get(index).ok_or_else(invalid)?;
			(Some(k.as_str()), *v)
		}
		Payload::Variant {
			payload: Some(v), ..
		} if index == 0 => (None, *v),
		_ => return Err(Error::new(ErrorKind::InvalidCode, "owned input child")),
	})
}
// Diagnostic rendering is bounded on the stack before allocating its owned copy.
struct Text {
	bytes: [u8; 1024],
	len: usize,
}
impl core::fmt::Write for Text {
	fn write_str(&mut self, s: &str) -> core::fmt::Result {
		let end = self.len.checked_add(s.len()).ok_or(core::fmt::Error)?;
		let out = self.bytes.get_mut(self.len..end).ok_or(core::fmt::Error)?;
		out.copy_from_slice(s.as_bytes());
		self.len = end;
		Ok(())
	}
}
fn description(store: &Store, value: Value, budget: &mut Budget) -> Result<String> {
	let mut text = Text {
		bytes: [0; 1024],
		len: 0,
	};
	let write = match value {
		Value::Function { .. } => write!(text, "a function"),
		Value::Heap(h) => match store.payload(h)? {
			Payload::Variant {
				tag: Type::Result, ..
			} => write!(text, "a result"),
			Payload::Closure { .. } => write!(text, "a function"),
			p if matches!(p.tag(), Type::Struct(_)) => {
				let Type::Struct(n) = p.tag() else {
					unreachable!()
				};
				let mut name = [0; 256];
				let len = crate::experimental_core::nominal::name_copy(n, &mut name).ok_or_else(
					|| {
						Error::new(
							ErrorKind::InvalidCode,
							"owned input nominal name missing or exceeds 256 bytes",
						)
					},
				)?;
				let name = core::str::from_utf8(&name[..len]).map_err(|_| invalid())?;
				let shape = crate::experimental_core::nominal::shape_copy(n).ok_or_else(invalid)?;
				let prefix = match shape {
					crate::experimental_core::nominal::Shape::Empty => "empty struct",
					crate::experimental_core::nominal::Shape::Tuple(_) => "tuple struct",
					crate::experimental_core::nominal::Shape::Named => "struct",
				};
				write!(text, "{prefix} {name}")
			}
			_ => write!(
				text,
				"::{} (an external reference)",
				store.diagnostic_type(value).ok_or_else(invalid)?
			),
		},
		_ => write!(
			text,
			"::{}",
			store.diagnostic_type(value).ok_or_else(invalid)?
		),
	};
	write.map_err(|_| description_limit())?;
	budget.text(core::str::from_utf8(&text.bytes[..text.len]).map_err(|_| invalid())?)
}
fn node(
	store: &Store,
	value: Value,
	depth: usize,
	cycle: bool,
	b: &mut Budget,
) -> Result<(TreeNode, usize)> {
	let mut len = 0;
	let kind = if depth > runtime::MAX_VALUE_DEPTH {
		TreeKind::DepthCut
	} else {
		match value {
			Value::Unit => TreeKind::Scalar(runtime::Value::unit()),
			Value::Bool(v) => TreeKind::Scalar(v.into()),
			Value::I64(v) => TreeKind::Scalar(v.into()),
			Value::U64(v) => TreeKind::Scalar(v.into()),
			Value::F64(v) => TreeKind::Scalar(v.into()),
			Value::Char(v) => TreeKind::Scalar(v.into()),
			Value::Ordering(v) => TreeKind::Scalar(v.into()),
			Value::Heap(h) => match store.payload(h)? {
				Payload::String(s) => TreeKind::String(b.text(s)?),
				Payload::List {
					tag: Type::Vec,
					values,
				} => {
					len = values.len();
					TreeKind::Vec
				}
				Payload::List {
					tag: Type::Tuple,
					values,
				} => {
					len = values.len();
					TreeKind::Tuple
				}
				Payload::Record {
					tag: Type::Object,
					fields,
				} => {
					len = fields.len();
					TreeKind::Object
				}
				Payload::Variant {
					tag: Type::Option,
					success,
					payload,
				} => {
					len = usize::from(payload.is_some());
					TreeKind::Option(*success)
				}
				_ => TreeKind::Unsupported(description(store, value, b)?),
			},
			_ => TreeKind::Unsupported(description(store, value, b)?),
		}
	};
	// Option needs one more entry to let the shared walk report at depth+1.
	let frontier =
		depth >= runtime::MAX_VALUE_DEPTH && len > 0 && !matches!(kind, TreeKind::Option(_));
	let traverse = if cycle || frontier { 0 } else { len };
	Ok((
		TreeNode {
			kind,
			edges: std::vec::Vec::new(),
			cycle,
			frontier,
		},
		traverse,
	))
}
impl Store {
	pub(crate) fn owned_host_input(&self, value: Value) -> Result<runtime::Value> {
		let mut budget = Budget {
			used: size_of::<Capsule>(),
			limit: self.limits.bytes,
		};
		budget.charge(0)?;
		let mut nodes = std::vec::Vec::new();
		let mut frames: std::vec::Vec<Frame> = std::vec::Vec::new();
		budget.room(&mut nodes, 1)?;
		let (root, len) = node(self, value, 1, false, &mut budget)?;
		nodes.push(root);
		budget.room(&mut frames, 1)?;
		frames.push(Frame {
			value,
			node: 0,
			next: 0,
			len,
			depth: 1,
		});
		// Bound validation path/identity capacities (power-of-two growth), descriptor
		// copies and serializer traversal. No additional tree materialization.
		budget.charge((runtime::MAX_VALUE_DEPTH + 1).next_power_of_two() * 64)?;
		while let Some(frame) = frames.last_mut() {
			if frame.next == frame.len {
				frames.pop();
				continue;
			}
			let (parent, index, depth, value) =
				(frame.node, frame.next, frame.depth + 1, frame.value);
			frame.next += 1;
			let (key, value) = child(self, value, index)?;
			// Reserve validation's path-key copy as well as the packet key.
			if let Some(key) = key {
				budget.charge(key.len())?;
			}
			let key = key.map(|s| budget.text(s)).transpose()?;
			let cycle = matches!(value, Value::Heap(_))
				&& track(value, self)?
				&& frames.iter().any(|f| f.value == value);
			budget.room(&mut nodes, 1)?;
			let (next, len) = node(self, value, depth, cycle, &mut budget)?;
			let at = nodes.len();
			nodes.push(next);
			budget.room(&mut nodes[parent].edges, 1)?;
			nodes[parent].edges.push((key, at));
			if len > 0 {
				budget.room(&mut frames, 1)?;
				frames.push(Frame {
					value,
					node: at,
					next: 0,
					len,
					depth,
				});
			}
		}
		// The remaining allocations are the owned capsule/value box, not a second tree.
		budget.charge(size_of::<Capsule>() + 256)?;
		Ok(runtime::Value::new(Capsule { nodes })?)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::host::OwnedTreeInput;
	use crate::runtime::FromValue;

	#[test]
	fn packet_survives_store_release_and_expands_aliases() {
		let mut store = Store::new(super::super::Limits::default()).unwrap();
		let text = store.root_string("owned").unwrap();
		let inner = store.root_list(Type::Vec, &[&text]).unwrap();
		let outer = store.root_list(Type::Vec, &[&inner, &inner]).unwrap();
		let packet =
			OwnedTreeInput::from_value(store.owned_host_input(outer.value).unwrap()).unwrap();
		store.root_release(outer).unwrap();
		store.root_release(inner).unwrap();
		store.root_release(text).unwrap();
		assert_eq!(store.live_entries(), 0);
		drop(store);
		let root = packet.node(0).unwrap();
		let a = root.child(0).unwrap().1;
		let b = root.child(1).unwrap().1;
		assert_ne!(a, b);
		for at in [a, b] {
			let child = packet.node(at).unwrap().child(0).unwrap().1;
			let TreeKind::String(text) = packet.node(child).unwrap().kind() else {
				panic!("owned text")
			};
			assert_eq!(text.as_str(), "owned");
		}
	}

	#[test]
	fn expanded_aliases_exceed_the_input_store_bytes() {
		let mut store = Store::new(super::super::Limits {
			entries: 100,
			bytes: 20_000,
		})
		.unwrap();
		let mut root = store.root_string("leaf").unwrap();
		for _ in 0..9 {
			let next = store.root_list(Type::Vec, &[&root, &root]).unwrap();
			store.root_release(root).unwrap();
			root = next;
		}
		assert!(store.payload_bytes() < 20_000);
		let failure = store.owned_host_input(root.value).unwrap_err();
		assert_eq!(failure.kind, ErrorKind::StoreCapacity);
		assert_eq!(failure.operation, "owned host input bytes");
		store.root_release(root).unwrap();
		assert_eq!(store.live_entries(), 0);
	}

	#[test]
	fn reservation_failure_and_peak_accounting() {
		let mut store = Store::new(super::super::Limits::default()).unwrap();
		let empty = store.root_list(Type::Vec, &[]).unwrap();
		assert_eq!(
			child(&store, empty.value, 0).unwrap_err().kind,
			ErrorKind::InvalidCode
		);
		let mut budget = Budget {
			used: 0,
			limit: usize::MAX,
		};
		let (cut, children) = node(
			&store,
			Value::Unit,
			runtime::MAX_VALUE_DEPTH + 1,
			false,
			&mut budget,
		)
		.unwrap();
		assert!(matches!(cut.kind(), TreeKind::DepthCut));
		assert_eq!(children, 0);
		assert_ne!(description_limit().kind, ErrorKind::StoreCapacity);
		store.root_release(empty).unwrap();
		let mut budget = Budget { used: 0, limit: 16 };
		let mut values = std::vec::Vec::<u64>::new();
		budget.room(&mut values, 1).unwrap();
		values.push(1);
		let capacity = values.capacity();
		let failure = budget.room(&mut values, capacity + 1).unwrap_err();
		assert_eq!(failure.kind, ErrorKind::StoreCapacity);
		assert_eq!(values, [1]);
		assert_eq!(values.capacity(), capacity);
		// A deterministic capacity-overflow failure exercises fallible reservation
		// without relying on the allocator or asking the OS for a huge mapping.
		let mut budget = Budget {
			used: 0,
			limit: usize::MAX,
		};
		let mut bytes = std::vec::Vec::<u8>::new();
		let failure = budget.room(&mut bytes, usize::MAX).unwrap_err();
		assert_eq!(failure.kind, ErrorKind::Allocation);
		assert_eq!(bytes.capacity(), 0);
	}
}
