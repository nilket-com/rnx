//! Cold Object allocation and growth transactions. The index never owns Values.
use super::indexing::RecordKey;
use super::object_index::{Index, INDEX_AT};
use super::*;

impl Store {
	#[cold]
	#[inline(never)]
	pub(super) fn object_field(&mut self, handle: Handle, key: &str) -> Result<Value> {
		let value = self
			.payload(handle)?
			.key_value(key)?
			.ok_or_else(|| err(ErrorKind::IndexOutOfBounds))?;
		self.retain(value)?;
		Ok(value)
	}
	#[cold]
	#[inline(never)]
	pub(super) fn object_field_get_mut(&mut self, handle: Handle, key: &str) -> Result<Value> {
		self.object_field(handle, key)
	}
	#[cold]
	#[inline(never)]
	pub(super) fn object_field_set_mut(
		&mut self,
		handle: Handle,
		key: &str,
		new: Value,
	) -> Result<()> {
		let index = self
			.payload(handle)?
			.key_position(key)?
			.ok_or_else(|| err(ErrorKind::InvalidCode))?;
		let Payload::Object { fields, .. } = self.payload(handle)? else {
			return Err(err(ErrorKind::InvalidCode));
		};
		let old = fields[index].1;
		if old == new {
			return Ok(());
		}
		let edges = self.replacement_edges(old, new)?;
		self.retain(new)?;
		let Payload::Object { fields, .. } = self.mutable(handle)? else {
			return Err(err(ErrorKind::InvalidCode));
		};
		fields[index].1 = new;
		self.edges = edges;
		self.assert_work();
		self.release(old)
	}

	#[cold]
	#[inline(never)]
	pub(super) fn object_set_key(
		&mut self,
		handle: Handle,
		key: RecordKey<'_>,
		new: Value,
	) -> Result<()> {
		let text = key.borrow(self)?;
		if let Some(i) = self.payload(handle)?.key_position(text)? {
			let Payload::Object { fields, .. } = self.payload(handle)? else {
				return Err(err(ErrorKind::InvalidCode));
			};
			let old = fields[i].1;
			let edges = self.replacement_edges(old, new)?;
			self.retain(new)?;
			let Payload::Object { fields, .. } = self.mutable(handle)? else {
				return Err(err(ErrorKind::InvalidCode));
			};
			fields[i].1 = new;
			self.edges = edges;
			self.assert_work();
			return self.release(old);
		}
		self.object_insert(handle, key, new)
	}

	fn object_room(&self, bytes: usize) -> Result<()> {
		if bytes > self.limits.bytes.saturating_sub(self.bytes) {
			return Err(err(ErrorKind::StoreCapacity));
		}
		Ok(())
	}
	fn object_buffer_bytes(capacity: usize) -> Result<usize> {
		capacity
			.checked_mul(size_of::<(String, Value)>())
			.ok_or_else(|| err(ErrorKind::Overflow))
	}
	fn object_key_size(key: &str) -> Result<()> {
		key.len()
			.checked_add(1)
			.and_then(|n| n.checked_mul(9))
			.map(|_| ())
			.ok_or_else(|| err(ErrorKind::Overflow))
	}
	pub(super) fn object_payload(&self, fields: Vec<(String, Value)>) -> Result<Payload> {
		let bytes = fields.iter().try_fold(
			Self::object_buffer_bytes(fields.capacity())?,
			|n, (key, _)| {
				Self::object_key_size(key)?;
				n.checked_add(key.capacity())
					.ok_or_else(|| err(ErrorKind::Overflow))
			},
		)?;
		let index = if fields.len() >= INDEX_AT {
			// Duplicate names are invalid private input; capacity/allocation may
			// fail before that invariant is diagnosed. Never fall back to a scan.
			self.object_room(
				bytes
					.checked_add(Index::planned(fields.capacity())?)
					.ok_or_else(|| err(ErrorKind::Overflow))?,
			)?;
			Some(
				Box::try_new(Index::build(fields.len(), fields.capacity(), |i| {
					fields[i].0.as_str()
				})?)
				.map_err(crate::alloc::Error::from)?,
			)
		} else {
			None
		};
		let payload = Payload::Object { fields, index };
		self.object_room(payload.bytes()?)?;
		Ok(payload)
	}
	pub(super) fn object_record(&mut self, fields: &[(&str, Value)]) -> Result<Value> {
		let mut bytes = Self::object_buffer_bytes(fields.len())?;
		for (i, (key, _)) in fields.iter().enumerate() {
			Self::object_key_size(key)?;
			if fields.len() < INDEX_AT && fields[..i].iter().any(|(k, _)| k == key) {
				return Err(err(ErrorKind::InvalidCode));
			}
			bytes = bytes
				.checked_add(key.len())
				.ok_or_else(|| err(ErrorKind::Overflow))?;
		}
		if fields.len() >= INDEX_AT {
			bytes = bytes
				.checked_add(Index::planned(fields.len())?)
				.ok_or_else(|| err(ErrorKind::Overflow))?;
		}
		self.object_room(bytes)?;
		if self.live == self.limits.entries
			|| (self.free.is_empty() && self.slots.len() == self.limits.entries)
		{
			return Err(err(ErrorKind::StoreCapacity));
		}
		let mut copy = Vec::try_with_capacity(fields.len())?;
		for (key, value) in fields {
			copy.try_push((String::try_from(*key)?, *value))?;
		}
		let payload = self.object_payload(copy)?;
		self.admit(payload)
	}

	pub(super) fn object_insert(
		&mut self,
		h: Handle,
		key: RecordKey<'_>,
		new: Value,
	) -> Result<()> {
		let text = key.borrow(self)?;
		Self::object_key_size(text)?;
		let Payload::Object { fields, index } = self.payload(h)? else {
			return Err(err(ErrorKind::InvalidCode));
		};
		let len = fields.len();
		let cap = fields.capacity();
		let next = if len == cap {
			cap.checked_mul(2)
				.ok_or_else(|| err(ErrorKind::Overflow))?
				.max(1)
		} else {
			cap
		};
		let old_index_bytes = index.as_ref().map_or(0, |i| i.bytes());
		let key_bytes = self
			.entry(h)?
			.charged_bytes()
			.checked_sub(Self::object_buffer_bytes(cap)?)
			.and_then(|n| n.checked_sub(old_index_bytes))
			.ok_or_else(|| err(ErrorKind::InvalidCode))?;
		let promote = len + 1 >= INDEX_AT && index.is_none();
		let grow_index = index.as_ref().is_some_and(|i| !i.room());
		let staged_index_bytes = if promote || grow_index {
			Index::planned(next)?
		} else {
			0
		};
		let peak = text
			.len()
			.checked_add(if len == cap {
				Self::object_buffer_bytes(next)?
			} else {
				0
			})
			.and_then(|n| n.checked_add(staged_index_bytes))
			.ok_or_else(|| err(ErrorKind::Overflow))?;
		self.object_room(peak)?;
		let copy = String::try_from(text)?;
		let mut replacement = if len == cap {
			Some(Vec::try_with_capacity(next)?)
		} else {
			None
		};
		let mut next_index = if promote {
			Some(
				Box::try_new(Index::build(len, next, |i| fields[i].0.as_str())?)
					.map_err(crate::alloc::Error::from)?,
			)
		} else if grow_index {
			let old = index.as_ref().ok_or_else(|| err(ErrorKind::InvalidCode))?;
			Some(Box::try_new(old.copy(next)?).map_err(crate::alloc::Error::from)?)
		} else {
			None
		};
		let selected = next_index.as_ref().or(index.as_ref());
		let insert = selected
			.map(|i| {
				i.prepare(&copy, len, |i| fields[i].0.as_str())
					.ok_or_else(|| err(ErrorKind::InvalidCode))
			})
			.transpose()?;
		let actual_cap = replacement.as_ref().map_or(cap, |v| v.capacity());
		let charged = Self::object_buffer_bytes(actual_cap)?
			.checked_add(key_bytes)
			.and_then(|n| n.checked_add(copy.capacity()))
			.and_then(|n| n.checked_add(selected.map_or(0, |i| i.bytes())))
			.ok_or_else(|| err(ErrorKind::Overflow))?;
		let bytes = self.replacement_bytes(h, charged)?;
		let edges = self
			.edges
			.checked_add(Self::heap_edge(new))
			.ok_or_else(|| err(ErrorKind::Overflow))?;
		if actual_cap < len + 1 || selected.is_some_and(|i| !i.room()) {
			return Err(err(ErrorKind::InvalidCode));
		}
		self.reserve_work(Self::heap_edge(new), 0)?;
		self.retain(new)?;
		let payload = match self.mutable(h) {
			Ok(payload) => payload,
			Err(error) => {
				self.release(new)?;
				return Err(error);
			}
		};
		let Payload::Object { fields, index } = payload else {
			self.release(new)?;
			return Err(err(ErrorKind::InvalidCode));
		};
		let commit = if let Some(insert) = insert {
			match next_index.as_mut().or(index.as_mut()) {
				Some(i) => i.commit(insert),
				None => Err(err(ErrorKind::InvalidCode)),
			}
		} else {
			Ok(())
		};
		if let Err(error) = commit {
			self.release(new)?;
			return Err(error);
		}
		let applied = (|| -> Result<()> {
			if let Some(mut next) = replacement.take() {
				#[cfg(test)]
				object_index::probe_copies(fields.len());
				while let Some(pair) = fields.pop() {
					if let Err(pair) = next.push_within_capacity(pair) {
						fields
							.push_within_capacity(pair)
							.map_err(|_| err(ErrorKind::InvalidCode))?;
						while let Some(pair) = next.pop() {
							fields
								.push_within_capacity(pair)
								.map_err(|_| err(ErrorKind::InvalidCode))?;
						}
						return Err(err(ErrorKind::InvalidCode));
					}
				}
				next.reverse();
				if next.push_within_capacity((copy, new)).is_err() {
					next.reverse();
					while let Some(pair) = next.pop() {
						fields
							.push_within_capacity(pair)
							.map_err(|_| err(ErrorKind::InvalidCode))?;
					}
					return Err(err(ErrorKind::InvalidCode));
				}
				core::mem::swap(fields, &mut next);
			} else {
				fields
					.push_within_capacity((copy, new))
					.map_err(|_| err(ErrorKind::InvalidCode))?;
			}
			Ok(())
		})();
		if let Err(error) = applied {
			if let (Some(insert), Some(index)) = (insert, index.as_mut()) {
				if next_index.is_none() {
					index.undo(insert);
				}
			}
			self.release(new)?;
			return Err(error);
		}
		if let Some(next) = next_index {
			*index = Some(next);
		}

		self.entry_mut(h)?.set_charged_bytes(charged);
		self.bytes = bytes;
		self.edges = edges;
		self.assert_work();
		Ok(())
	}
}
