//! One receiver transaction. All real handles stay here, outside the native ABI.
//! Every guard and ownership check precedes publication; the writes cannot fail.
use super::{Entry, Handle, Payload, Store};
use crate::experimental_core::{
	code::Code,
	region::recursion::{
		self,
		methods::{Methods, MAX_FIELDS},
	},
	value::{Type, Value},
};

#[derive(Clone, Copy)]
struct Field {
	position: usize,
	value: Value,
}
const EMPTY: Field = Field {
	position: 0,
	value: Value::Unit,
};

pub(crate) struct Snapshot {
	store: u64,
	root: Handle,
	vector: Option<Handle>,
	fields: [Field; MAX_FIELDS],
	count: usize,
	record_len: usize,
	record_capacity: usize,
	vector_len: usize,
	vector_capacity: usize,
	vector_tag: u32,
	root_strong: u32,
	vector_strong: u32,
	live: usize,
	bytes: usize,
	edges: usize,
	work_capacity: usize,
}

// Entry identity/generation/open checks come from Store::entry. Keep native
// inspection separate from the interpreter's error-producing payload leaf.
fn bound_payload(entry: &Entry) -> Option<&Payload> {
	match entry.payload.as_ref()? {
		Payload::Moved(_) => None,
		payload => Some(payload),
	}
}

impl Snapshot {
	pub(crate) fn vector_len(&self) -> usize {
		self.vector_len
	}
	pub(crate) fn vector_tag(&self) -> u32 {
		self.vector_tag
	}

	/// The original root frame owns self throughout every virtual activation;
	/// the unchanged receiver edge owns the Vec. At most max_slots owned aliases
	/// per activation plus call/put's transient retained prefix can exist. No
	/// admitted operation replaces slot0 or the heap field, or returns a heap.
	fn headroom(strong: u32, slots: usize, depth: usize) -> Option<()> {
		let bound = slots.checked_add(5)?.checked_mul(depth.checked_add(1)?)?;
		strong.checked_add(u32::try_from(bound).ok()?)?;
		Some(())
	}

	pub(crate) fn prepare(
		store: &Store,
		code: &Code,
		shape: &Methods,
		root: Value,
		depth: usize,
	) -> Option<Self> {
		if !store.scalar_release_is_noop()
			|| shape.code != code.id()
			|| shape.fields.len() > MAX_FIELDS
		{
			return None;
		}
		let Value::Heap(root) = root else {
			return None;
		};
		let entry = store.entry(root).ok()?;
		if entry.readers != 0 {
			return None;
		}
		Self::headroom(entry.strong, shape.engine.slots, depth)?;
		let Payload::Record {
			tag: Type::Struct(nominal),
			fields,
		} = bound_payload(entry)?
		else {
			return None;
		};
		if *nominal != shape.nominal {
			return None;
		}
		let mut snapshot = Self {
			store: store.id,
			root,
			vector: None,
			fields: [EMPTY; MAX_FIELDS],
			count: shape.fields.len(),
			record_len: fields.len(),
			record_capacity: fields.capacity(),
			vector_len: 0,
			vector_capacity: 0,
			vector_tag: 0,
			root_strong: entry.strong,
			vector_strong: 0,
			live: store.live,
			bytes: store.bytes,
			edges: store.edges,
			work_capacity: store.work.capacity(),
		};
		for (i, descriptor) in shape.fields.iter().enumerate() {
			let name = code
				.function(descriptor.function)?
				.strings
				.get(descriptor.key as usize)?
				.as_str();
			let position = fields.iter().position(|(key, _)| key.as_str() == name)?;
			let value = fields[position].1;
			if snapshot.fields[..i].iter().any(|f| f.position == position) {
				return None;
			}
			match descriptor.kind {
				1 => {
					recursion::tag(value)?;
				}
				2 => {
					let Value::Heap(vector) = value else {
						return None;
					};
					if snapshot.vector.is_some_and(|old| old != vector) {
						return None;
					}
					snapshot.vector = Some(vector);
				}
				_ => return None,
			}
			snapshot.fields[i] = Field { position, value };
		}
		if let Some(vector) = snapshot.vector {
			let entry = store.entry(vector).ok()?;
			if entry.readers != 0 {
				return None;
			}
			Self::headroom(entry.strong, shape.engine.slots, depth)?;
			let Payload::List {
				tag: Type::Vec,
				values,
			} = bound_payload(entry)?
			else {
				return None;
			};
			let tag = match values.first() {
				Some(Value::I64(_)) => 1,
				Some(Value::Bool(_)) => 2,
				None => shape.vector_tag.unwrap_or(1),
				_ => return None,
			};
			if shape.vector_tag.is_some_and(|required| required != tag)
				|| !values
					.iter()
					.all(|&v| recursion::tag(v).is_some_and(|(t, _)| t == tag))
			{
				return None;
			}
			snapshot.vector_len = values.len();
			snapshot.vector_capacity = values.capacity();
			snapshot.vector_strong = entry.strong;
			snapshot.vector_tag = tag;
		}
		Some(snapshot)
	}

	pub(crate) fn fill(
		&self,
		store: &Store,
		shape: &Methods,
		values: &mut [i64],
		tags: &mut [i64],
		vector: &mut [i64],
	) -> Option<()> {
		if values.len() != self.count
			|| tags.len() != self.count
			|| vector.len() != self.vector_len.max(1)
		{
			return None;
		}
		for i in 0..self.count {
			let (tag, value) = if shape.fields[i].kind == 2 {
				(5, 1)
			} else {
				recursion::tag(self.fields[i].value)?
			};
			values[i] = value;
			tags[i] = tag as i64;
		}
		if let Some(handle) = self.vector {
			let Payload::List { values, .. } = bound_payload(store.entry(handle).ok()?)? else {
				return None;
			};
			for (out, &value) in vector.iter_mut().zip(values.iter()) {
				*out = recursion::tag(value)?.1;
			}
		}
		Some(())
	}

	/// Decode only closed private identity tokens. There is no way to synthesize
	/// a real engine handle from an integer returned by native code.
	pub(crate) fn decode(&self, shape: &Methods, tag: u32, value: i64) -> Option<Value> {
		match tag {
			4 if value == 0 => Some(Value::Heap(self.root)),
			5 if value == 1 => Some(Value::Heap(self.vector?)),
			6 => Some(Value::U64(value as u64)),
			7 => {
				let pc = usize::try_from(value).ok()?;
				Some(Value::Function {
					code: shape.code,
					id: shape.function(pc)?.id,
				})
			}
			_ => recursion::scalar(tag, value),
		}
	}

	#[allow(clippy::too_many_arguments)]
	pub(crate) fn publish(
		&self,
		store: &mut Store,
		code: &Code,
		shape: &Methods,
		window: &mut [Value],
		original: &[Value],
		output: &[Value],
		fields: &[i64],
		tags: &[i64],
		vector: &[i64],
	) -> bool {
		if store.id != self.store
			|| shape.code != code.id()
			|| !store.scalar_release_is_noop()
			|| store.live != self.live
			|| store.bytes != self.bytes
			|| store.edges != self.edges
			|| store.work.capacity() != self.work_capacity
			|| self.count != shape.fields.len()
			|| window != original
			|| window.len() != output.len()
			|| window.first() != Some(&Value::Heap(self.root))
			|| shape.functions.first().map(|f| f.slots) != Some(window.len())
			|| output.first() != window.first()
			|| fields.len() != self.count
			|| tags.len() != self.count
			|| vector.len() != self.vector_len.max(1)
			|| original[1..].iter().any(|&v| recursion::tag(v).is_none())
		{
			return false;
		}
		let Ok(entry) = store.entry(self.root) else {
			return false;
		};
		if entry.strong != self.root_strong || entry.readers != 0 {
			return false;
		}
		let Some(Payload::Record {
			tag: Type::Struct(nominal),
			fields: old_fields,
		}) = bound_payload(entry)
		else {
			return false;
		};
		if *nominal != shape.nominal
			|| old_fields.len() != self.record_len
			|| old_fields.capacity() != self.record_capacity
		{
			return false;
		}
		let mut new_fields = [Value::Unit; MAX_FIELDS];
		for (i, descriptor) in shape.fields.iter().enumerate() {
			let Some((name, value)) = old_fields.get(self.fields[i].position) else {
				return false;
			};
			let Some(expected) = code
				.function(descriptor.function)
				.and_then(|f| f.strings.get(descriptor.key as usize))
			else {
				return false;
			};
			if name != expected || *value != self.fields[i].value {
				return false;
			}
			if descriptor.kind == 2 {
				if tags[i] != 5 || fields[i] != 1 || self.vector.map(Value::Heap) != Some(*value) {
					return false;
				}
			} else if descriptor.kind == 1 && recursion::tag(*value).is_some() {
				let Ok(tag) = u32::try_from(tags[i]) else {
					return false;
				};
				let Some(value) = recursion::scalar(tag, fields[i]) else {
					return false;
				};
				new_fields[i] = value;
			} else {
				return false;
			}
		}
		if let Some(handle) = self.vector {
			let Ok(entry) = store.entry(handle) else {
				return false;
			};
			if entry.readers != 0 || entry.strong != self.vector_strong {
				return false;
			}
			let Some(Payload::List {
				tag: Type::Vec,
				values,
			}) = bound_payload(entry)
			else {
				return false;
			};
			if values.len() != self.vector_len
				|| values.capacity() != self.vector_capacity
				|| !values
					.iter()
					.all(|&v| recursion::tag(v).is_some_and(|(t, _)| t == self.vector_tag))
				|| (self.vector_tag == 2
					&& vector[..self.vector_len].iter().any(|&v| v != 0 && v != 1))
			{
				return false;
			}
		}
		let mut root_delta = 0u32;
		let mut vector_delta = 0u32;
		for &value in &output[1..] {
			if let Value::Heap(handle) = value {
				if handle == self.root {
					root_delta += 1;
				} else if Some(handle) == self.vector {
					vector_delta += 1;
				} else {
					return false;
				}
			}
		}
		let Some(root_strong) = self.root_strong.checked_add(root_delta) else {
			return false;
		};
		let Some(vector_strong) = self.vector_strong.checked_add(vector_delta) else {
			return false;
		};
		// Every payload, scalar, position, reference count and destination is now
		// validated. The exclusive Store borrow prevents invalidation. No fallible
		// setter, allocation, release, queue growth or callback follows this point.
		let entry = store.slots[self.root.slot as usize].entry.as_mut().unwrap();
		entry.strong = root_strong;
		let Payload::Record {
			fields: destination,
			..
		} = entry.payload.as_mut().unwrap()
		else {
			unreachable!()
		};
		for (i, descriptor) in shape.fields.iter().enumerate() {
			if descriptor.kind == 1 {
				destination[self.fields[i].position].1 = new_fields[i];
			}
		}
		if let Some(handle) = self.vector {
			let entry = store.slots[handle.slot as usize].entry.as_mut().unwrap();
			entry.strong = vector_strong;
			let Payload::List { values, .. } = entry.payload.as_mut().unwrap() else {
				unreachable!()
			};
			for (out, &value) in values.iter_mut().zip(vector.iter()) {
				*out = if self.vector_tag == 1 {
					Value::I64(value)
				} else {
					Value::Bool(value != 0)
				};
			}
		}
		window.copy_from_slice(output);
		true
	}
}
