//! Consuming, budgeted document construction. The owner ledger is not a second
//! document: it contains only the local ownership of freshly admitted heap nodes.
use super::object_index::{Index, INDEX_AT};
use super::*;
use crate::function::host::owned_output::{contract, Materialized};
use crate::function::host::{OutputNode, OwnedOutputSource};

fn malformed() -> Error {
	contract(super::super::error::HostDynamicReason::OutputTree)
}
fn overflow() -> Error {
	err(ErrorKind::Overflow)
}
fn bytes<T>(n: usize) -> Result<usize> {
	n.checked_mul(size_of::<T>()).ok_or_else(overflow)
}

// Only consuming parser iterators and non-owning child handles live here.
// All frame/buffer storage is checked against the remaining Store byte budget.
enum Frame<S: OwnedOutputSource> {
	Array {
		values: S::Array,
		children: Vec<Value>,
		len: usize,
	},
	Object {
		values: S::Object,
		fields: Vec<(String, Value)>,
		len: usize,
		allocated: usize,
		pending: Option<std::string::String>,
	},
}
enum Progress<S: OwnedOutputSource> {
	Descend(S),
	Complete(Value),
}

struct Builder<'a> {
	store: &'a mut Store,
	owners: Vec<Value>,
	scratch: usize,
	cleanup: Option<Error>,
}
impl Builder<'_> {
	fn check(&self, extra: usize) -> Result<()> {
		self.store
			.host_room(self.scratch.checked_add(extra).ok_or_else(overflow)?, 0)
	}
	fn owner_room(&mut self) -> Result<()> {
		if self.owners.len() < self.owners.capacity() {
			return Ok(());
		}
		let old = bytes::<Value>(self.owners.capacity())?;
		let capacity = self
			.owners
			.capacity()
			.checked_mul(2)
			.ok_or_else(overflow)?
			.max(1);
		// Both old and new allocations exist during a fallible growth.
		self.check(bytes::<Value>(capacity)?)?;
		#[cfg(test)]
		tests::allocation("owner");
		self.owners
			.try_reserve_exact(capacity - self.owners.len())?;
		self.scratch = self
			.scratch
			.checked_sub(old)
			.and_then(|n| n.checked_add(bytes::<Value>(self.owners.capacity()).ok()?))
			.ok_or_else(overflow)?;
		self.check(0)
	}
	fn own(&mut self, value: Value) -> Result<Value> {
		if let Err(primary) = self.owners.try_push(value) {
			self.cleanup = self.store.release(value).err();
			return Err(primary.into());
		}
		self.check(0)?;
		Ok(value)
	}
	fn buffer<T>(&mut self, len: usize) -> Result<Vec<T>> {
		self.check(bytes::<T>(len)?)?;
		#[cfg(test)]
		tests::allocation("buffer");
		let buffer = Vec::try_with_capacity(len)?;
		self.scratch = self
			.scratch
			.checked_add(bytes::<T>(buffer.capacity())?)
			.ok_or_else(overflow)?;
		self.check(0)?;
		Ok(buffer)
	}
	fn text(&mut self, text: &str) -> Result<Value> {
		self.owner_room()?;
		self.check(text.len())?;
		#[cfg(test)]
		tests::allocation("string");
		let value = self.store.string(text)?;
		self.own(value)
	}
	fn node<S: OwnedOutputSource>(&mut self, source: S) -> Result<Value> {
		let OutputNode::Result { success, value } = source.into_node() else {
			return Err(malformed());
		};
		let child = if success {
			self.walk(value)?
		} else {
			let OutputNode::String(text) = value.into_node() else {
				return Err(malformed());
			};
			self.text(&text)?
		};
		self.owner_room()?;
		self.check(size_of::<Value>())?;
		#[cfg(test)]
		tests::allocation("result");
		let value = self.store.variant(Type::Result, success, Some(child))?;
		self.own(value)
	}
	fn push_frame<S: OwnedOutputSource>(
		&mut self,
		frames: &mut Vec<Frame<S>>,
		frame: Frame<S>,
	) -> Result<()> {
		if frames.len() >= 127 {
			return Err(malformed());
		}
		if frames.len() == frames.capacity() {
			let old = bytes::<Frame<S>>(frames.capacity())?;
			let cap = frames
				.capacity()
				.checked_mul(2)
				.ok_or_else(overflow)?
				.max(1);
			self.check(bytes::<Frame<S>>(cap)?)?;
			#[cfg(test)]
			tests::allocation("traversal");
			frames.try_reserve_exact(cap - frames.len())?;
			self.scratch = self
				.scratch
				.checked_sub(old)
				.and_then(|n| n.checked_add(bytes::<Frame<S>>(frames.capacity()).ok()?))
				.ok_or_else(overflow)?;
			self.check(0)?;
		}
		frames.try_push(frame)?;
		Ok(())
	}
	fn walk<S: OwnedOutputSource>(&mut self, source: S) -> Result<Value> {
		let mut frames = Vec::<Frame<S>>::new();
		let mut source = Some(source);
		let mut completed = None;
		loop {
			if let Some(next) = source.take() {
				// Scalars complete here. A large Frame/Progress enum never travels
				// through a helper's return ABI for every array element.
				completed = match next.into_node() {
					OutputNode::Unit => Some(Value::Unit),
					OutputNode::Bool(v) => Some(Value::Bool(v)),
					OutputNode::I64(v) => Some(Value::I64(v)),
					OutputNode::U64(v) => Some(Value::U64(v)),
					OutputNode::F64(v) if v.is_finite() => Some(Value::F64(v)),
					OutputNode::F64(_) | OutputNode::Result { .. } => return Err(malformed()),
					OutputNode::String(text) => Some(self.text(&text)?),
					OutputNode::Array(values) => {
						if frames.len() >= 127 {
							return Err(malformed());
						}
						let len = values.len();
						let children = self.buffer::<Value>(len)?;
						self.push_frame(
							&mut frames,
							Frame::Array {
								values,
								children,
								len,
							},
						)?;
						None
					}
					OutputNode::Object(values) => {
						if frames.len() >= 127 {
							return Err(malformed());
						}
						let len = values.len();
						let fields = self.buffer::<(String, Value)>(len)?;
						let allocated = bytes::<(String, Value)>(fields.capacity())?;
						self.push_frame(
							&mut frames,
							Frame::Object {
								values,
								fields,
								len,
								allocated,
								pending: None,
							},
						)?;
						None
					}
				};
			}
			let Some(frame) = frames.last_mut() else {
				self.scratch = self
					.scratch
					.checked_sub(bytes::<Frame<S>>(frames.capacity())?)
					.ok_or_else(overflow)?;
				return completed.ok_or_else(malformed);
			};
			match self.advance(frame, completed.take())? {
				Progress::Descend(next) => source = Some(next),
				Progress::Complete(value) => {
					// Completion has already adopted/emptied the frame buffers.
					frames.pop();
					completed = Some(value);
				}
			}
		}
	}
	fn advance<S: OwnedOutputSource>(
		&mut self,
		frame: &mut Frame<S>,
		child: Option<Value>,
	) -> Result<Progress<S>> {
		match frame {
			Frame::Array {
				values,
				children,
				len,
			} => {
				if let Some(child) = child {
					children.try_push(child)?;
				}
				if let Some(source) = values.next() {
					if children.len() == *len {
						return Err(malformed());
					}
					return Ok(Progress::Descend(source));
				}
				if children.len() != *len {
					return Err(malformed());
				}
				self.owner_room()?;
				let allocated = bytes::<Value>(children.capacity())?;
				// Transfer the already-budgeted buffer, charging its actual capacity.
				// admit transactionally retains child edges; the ledger keeps its
				// local owners until the outer Result commits.
				self.scratch = self.scratch.checked_sub(allocated).ok_or_else(overflow)?;
				self.check(allocated)?;
				#[cfg(test)]
				tests::allocation("list");
				let value = self.store.admit(Payload::List {
					tag: Type::Vec,
					values: core::mem::take(children),
				})?;
				Ok(Progress::Complete(self.own(value)?))
			}
			Frame::Object {
				values,
				fields,
				len,
				allocated,
				pending,
			} => {
				if let Some(child) = child {
					let key = pending.take().ok_or_else(malformed)?;
					self.check(key.len())?;
					#[cfg(test)]
					tests::allocation("key");
					let key = String::try_from(key.as_str())?;
					self.scratch = self
						.scratch
						.checked_add(key.capacity())
						.ok_or_else(overflow)?;
					*allocated = allocated.checked_add(key.capacity()).ok_or_else(overflow)?;
					self.check(0)?;
					fields.try_push((key, child))?;
				}
				if let Some((key, source)) = values.next() {
					if fields.len() == *len
						|| (*len < INDEX_AT
							&& fields.iter().any(|(prior, _)| prior.as_str() == key))
					{
						return Err(malformed());
					}
					*pending = Some(key);
					return Ok(Progress::Descend(source));
				}
				if fields.len() != *len {
					return Err(malformed());
				}
				self.owner_room()?;
				self.check(if *len >= INDEX_AT {
					Index::planned(fields.capacity())?
				} else {
					0
				})?;
				#[cfg(test)]
				tests::allocation("index");
				let payload =
					self.store
						.object_payload(core::mem::take(fields))
						.map_err(|failure| {
							if failure.kind == ErrorKind::InvalidCode {
								malformed()
							} else {
								failure
							}
						})?;
				self.scratch = self.scratch.checked_sub(*allocated).ok_or_else(overflow)?;
				*allocated = 0;
				self.check(payload.bytes()?)?;
				let value = self.store.admit(payload)?;
				Ok(Progress::Complete(self.own(value)?))
			}
		}
	}
	fn release(&mut self) -> Option<Error> {
		let mut first = self.cleanup.take();
		while let Some(value) = self.owners.pop() {
			if let Err(failure) = self.store.release(value) {
				first.get_or_insert(failure);
			}
		}
		first
	}
}

#[cfg(test)]
mod tests;

impl Store {
	pub(crate) fn materialize_owned_output<S: OwnedOutputSource>(
		&mut self,
		source: S,
	) -> Materialized {
		let mut builder = Builder {
			store: self,
			owners: Vec::new(),
			scratch: 0,
			cleanup: None,
		};
		let mut result =
			std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| builder.node(source)))
				.unwrap_or_else(|_| {
					Err(err(ErrorKind::Host).with_detail(super::super::error::Detail::HostPanicked))
				});
		if let Ok(value) = result {
			if builder.owners.last() == Some(&value) {
				builder.owners.pop();
			} else {
				result = Err(malformed());
			}
		}
		let mut cleanup = builder.release();
		if let Some(primary) = cleanup.take() {
			if let Ok(value) = result {
				cleanup = builder.store.release(value).err();
				result = Err(primary);
			} else {
				cleanup = Some(primary);
			}
		}
		Materialized { result, cleanup }
	}
}
