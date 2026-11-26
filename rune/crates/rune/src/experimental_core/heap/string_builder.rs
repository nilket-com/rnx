//! In-place string operations keep one charged allocation under all aliases.
use super::*;
use crate::experimental_core::error::{Detail, MovedAccess};
use crate::function::string_builder::Operation;

impl Store {
	fn writable_text(&mut self, target: Value) -> Result<(Handle, &mut String)> {
		let Value::Heap(handle) = target else {
			return Err(err(ErrorKind::Type));
		};
		let Payload::String(text) = self.mutable(handle)? else {
			return Err(err(ErrorKind::Type));
		};
		Ok((handle, text))
	}

	pub(super) fn push_char(&mut self, target: Value, value: Value) -> Result<()> {
		let (handle, text) = self.writable_text(target)?;
		let Value::Char(ch) = value else {
			let found = self
				.diagnostic_type(value)
				.ok_or_else(|| err(ErrorKind::InvalidCode))?;
			return Err(err(ErrorKind::Type).with_detail(Detail::ExpectedChar { found }));
		};
		let capacity = text.capacity_after_reserve(ch.len_utf8())?;
		if capacity == text.capacity() {
			text.try_push(ch)?;
			return Ok(());
		}
		let total = self.replacement_bytes(handle, capacity)?;
		let (_, text) = self.writable_text(target)?;
		text.try_push(ch)?;
		self.finish_string_growth(handle, capacity, total)
	}

	fn finish_string_growth(
		&mut self,
		handle: Handle,
		capacity: usize,
		total: usize,
	) -> Result<()> {
		self.entry_mut(handle)?.set_charged_bytes(capacity);
		self.bytes = total;
		Ok(())
	}

	pub(crate) fn append_string(&mut self, target: Value, rhs: Value) -> Result<()> {
		let (handle, text) = self.writable_text(target)?;
		let current = text.capacity();
		let additional = self.text(rhs)?.len();
		let (_, text) = self.writable_text(target)?;
		let capacity = text.capacity_after_reserve(additional)?;
		if capacity > current {
			let total = self.replacement_bytes(handle, capacity)?;
			let mut result = String::try_with_capacity(capacity)?;
			result.try_push_str(self.text(target)?)?;
			result.try_push_str(self.text(rhs)?)?;
			*self.writable_text(target)?.1 = result;
			self.finish_string_growth(handle, capacity, total)?;
		} else {
			// The two handles may name the same allocation. Read the bytes
			// before borrowing the destination for the write.
			let suffix = String::try_from(self.text(rhs)?)?;
			self.writable_text(target)?.1.try_push_str(&suffix)?;
		}
		Ok(())
	}
}

impl Store {
	#[cold]
	pub(crate) fn string_builder(
		&mut self,
		op: Operation,
		target: Value,
		argument: Value,
	) -> Result<Value> {
		use Operation::*;
		if matches!(op, New | WithCapacity) {
			let capacity = if op == New {
				0
			} else {
				self.usize_argument(target, 0)?
			};
			if capacity > isize::MAX as usize {
				return Err(err(ErrorKind::Allocation).with_detail(Detail::CapacityOverflow));
			}
			self.admission_room(capacity)?;
			return self.string_owned(String::try_with_capacity(capacity)?);
		}
		if self.tag(target)? != Type::String {
			return Err(err(ErrorKind::InvalidCode));
		}
		if op == Capacity {
			self.argument(target, 0, MovedAccess::Read)?;
			let Value::Heap(h) = target else {
				return Err(err(ErrorKind::InvalidCode));
			};
			let Payload::String(text) = self.payload(h)? else {
				return Err(err(ErrorKind::InvalidCode));
			};
			return Ok(Value::U64(text.capacity() as u64));
		}
		self.argument(target, 0, MovedAccess::Write)?;
		match op {
			PushStr => {
				self.string_argument(argument, MovedAccess::Read)?;
				self.append_string(target, argument)?;
			}
			Clear => self.writable_text(target)?.1.clear(),
			ShrinkToFit => {
				let (h, text) = self.writable_text(target)?;
				text.try_shrink_to_fit()?;
				let capacity = text.capacity();
				let total = self.replacement_bytes(h, capacity)?;
				self.finish_string_growth(h, capacity, total)?;
			}
			Reserve | ReserveExact => {
				let additional = self.usize_argument(argument, 1)?;
				let (handle, text) = self.writable_text(target)?;
				let capacity = if op == Reserve {
					text.capacity_after_reserve(additional).map_err(|_| {
						err(ErrorKind::Allocation).with_detail(Detail::CapacityOverflow)
					})?
				} else {
					text.len()
						.checked_add(additional)
						.filter(|n| *n <= isize::MAX as usize)
						.ok_or_else(|| {
							err(ErrorKind::Allocation).with_detail(Detail::CapacityOverflow)
						})?
				};
				if capacity > text.capacity() {
					let total = self.replacement_bytes(handle, capacity)?;
					let (_, text) = self.writable_text(target)?;
					text.try_reserve_exact(capacity - text.len())?;
					self.finish_string_growth(handle, capacity, total)?;
				}
			}
			Capacity | New | WithCapacity => return Err(err(ErrorKind::InvalidCode)),
		}
		Ok(Value::Unit)
	}
}
