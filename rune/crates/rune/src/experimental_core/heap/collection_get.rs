//! Optional collection reads. Unlike indexing, invalid slice bounds and UTF-8
//! boundaries return None. Containers are copied shallowly; strings independently.
use super::*;
use crate::experimental_core::error::{Detail, MovedAccess};
use crate::function::object::Method;
use core::mem::size_of;

impl Store {
	#[cold]
	#[inline(never)]
	pub(crate) fn collection_get(
		&mut self,
		method: Method,
		receiver: Value,
		index: Value,
	) -> Result<Value> {
		let tag = self.tag(receiver)?;
		let provided = match method {
			Method::VecGet => tag == Type::Vec,
			Method::TupleGet => matches!(tag, Type::Tuple | Type::Unit),
			Method::StringGet => tag == Type::String,
			_ => false,
		};
		if !provided {
			return Err(self.text_missing(receiver, method.hash())?);
		}
		// Native argument conversion borrows the receiver before reading the
		// index. Returning a child handle does not borrow that child's payload.
		self.argument(receiver, 0, MovedAccess::Read)?;
		let range = matches!(
			self.tag(index).ok(),
			Some(
				Type::Range
					| Type::RangeFrom
					| Type::RangeFull
					| Type::RangeInclusive
					| Type::RangeTo | Type::RangeToInclusive
			)
		);
		if !range {
			if tag == Type::String {
				return Err(err(ErrorKind::Type).with_detail(Detail::IndexTarget {
					target: super::super::error::TypeName::String,
					index: self
						.diagnostic_type(index)
						.ok_or_else(|| err(ErrorKind::InvalidCode))?,
				}));
			}
			let index = self.get_index(index)?;
			let value = self.get_list(receiver)?.get(index).copied();
			return self.variant(Type::Option, value.is_some(), value);
		}
		let len = self.len(receiver)?;
		let Some((start, end)) = self.get_bounds(index, len)? else {
			return self.variant(Type::Option, false, None);
		};
		let payload = if tag == Type::String {
			let Some(slice) = self.text(receiver)?.get(start..end) else {
				return self.variant(Type::Option, false, None);
			};
			self.get_copy_budget(slice.len())?;
			Payload::String(String::try_from(slice)?)
		} else {
			let Some(slice) = self.get_list(receiver)?.get(start..end) else {
				return self.variant(Type::Option, false, None);
			};
			let bytes = slice
				.len()
				.checked_mul(size_of::<Value>())
				.ok_or_else(|| err(ErrorKind::Overflow))?;
			self.get_copy_budget(bytes)?;
			let mut values = Vec::try_with_capacity(slice.len())?;
			for value in slice {
				values.try_push(*value)?;
			}
			// admit acquires child edges transactionally, including duplicates.
			Payload::List {
				tag: Type::Vec,
				values,
			}
		};
		let owned = self.admit(payload)?;
		let result = self.variant(Type::Option, true, Some(owned));
		// Drop the unpublished container's owner even if Option admission fails.
		self.release(owned)?;
		result
	}

	fn get_copy_budget(&self, bytes: usize) -> Result<()> {
		if bytes > self.limits.bytes.saturating_sub(self.bytes) {
			return Err(err(ErrorKind::StoreCapacity));
		}
		Ok(())
	}

	fn get_list(&self, receiver: Value) -> Result<&[Value]> {
		if receiver == Value::Unit {
			return Ok(&[]);
		}
		let Value::Heap(handle) = receiver else {
			return Err(err(ErrorKind::InvalidCode));
		};
		match self.payload(handle)? {
			Payload::List {
				tag: Type::Vec | Type::Tuple,
				values,
			} => Ok(values),
			_ => Err(err(ErrorKind::InvalidCode)),
		}
	}

	fn get_index(&self, value: Value) -> Result<usize> {
		super::super::execute::index_value(value).map_err(|error| {
			if error.detail == Detail::None {
				match self.diagnostic_type(value) {
					Some(found) => error.with_detail(Detail::ExpectedNumber { found }),
					None => err(ErrorKind::InvalidCode),
				}
			} else {
				error
			}
		})
	}

	fn get_bounds(&self, range: Value, len: usize) -> Result<Option<(usize, usize)>> {
		let Value::Heap(handle) = range else {
			return Err(err(ErrorKind::InvalidCode));
		};
		let (start, end, inclusive) = match self.payload(handle)? {
			Payload::OtherRange(shape) => (
				shape.start,
				shape.end,
				matches!(shape.tag, Type::RangeInclusive | Type::RangeToInclusive),
			),
			payload => {
				let (start, end) = payload.range_values()?;
				(Some(start), Some(end), false)
			}
		};
		let start = start.map(|v| self.get_index(v)).transpose()?.unwrap_or(0);
		let end = match end {
			Some(value) => {
				let end = self.get_index(value)?;
				if inclusive {
					let Some(end) = end.checked_add(1) else {
						return Ok(None);
					};
					end
				} else {
					end
				}
			}
			None => len,
		};
		Ok(Some((start, end)))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::alloc::limit;

	fn payload(s: &Store, value: Value) -> Option<Value> {
		s.variant_parts(value).unwrap().2
	}

	#[test]
	fn collection_get_retains_children_and_copies_slice_container() {
		let mut s = Store::new(Limits::default()).unwrap();
		let child = s.list(Type::Vec, &[Value::I64(1)]).unwrap();
		let source = s.list(Type::Vec, &[child, child]).unwrap();
		let range = s.range(0, 2).unwrap();
		let one = s
			.collection_get(Method::VecGet, source, Value::I64(0))
			.unwrap();
		assert_eq!(payload(&s, one), Some(child));
		let sliced = s.collection_get(Method::VecGet, source, range).unwrap();
		let copy = payload(&s, sliced).unwrap();
		assert_ne!(source, copy);
		assert_eq!(s.get_list(copy).unwrap(), &[child, child]);
		s.release(source).unwrap();
		s.release(child).unwrap();
		assert_eq!(s.len(payload(&s, one).unwrap()).unwrap(), 1);
		for value in [range, one, sliced] {
			s.release(value).unwrap();
		}
		assert_eq!((s.live_entries(), s.payload_bytes(), s.edges), (0, 0, 0));
	}

	#[test]
	fn collection_get_bounds_utf8_unit_and_inclusive_overflow() {
		let mut s = Store::new(Limits::default()).unwrap();
		let text = s.string("éx").unwrap();
		let invalid = s.range(1, 2).unwrap();
		let valid = s.range(0, 2).unwrap();
		let all = s.other_range(Type::RangeFull, &[]).unwrap();
		let overflow = s
			.other_range(Type::RangeToInclusive, &[Value::U64(usize::MAX as u64)])
			.unwrap();
		for range in [invalid, overflow] {
			let result = s.collection_get(Method::StringGet, text, range).unwrap();
			assert_eq!(payload(&s, result), None);
			s.release(result).unwrap();
		}
		let result = s.collection_get(Method::StringGet, text, valid).unwrap();
		let copy = payload(&s, result).unwrap();
		assert_eq!(s.text(copy).unwrap(), "é");
		assert_ne!(copy, text);
		let unit = s
			.collection_get(Method::TupleGet, Value::Unit, all)
			.unwrap();
		assert_eq!(s.get_list(payload(&s, unit).unwrap()).unwrap(), &[]);
		let missing = s
			.collection_get(Method::TupleGet, Value::Unit, Value::I64(0))
			.unwrap();
		assert_eq!(payload(&s, missing), None);
		for value in [text, invalid, valid, all, overflow, result, unit, missing] {
			s.release(value).unwrap();
		}
		assert_eq!((s.live_entries(), s.payload_bytes(), s.edges), (0, 0, 0));
	}

	#[test]
	fn collection_get_moved_receiver_precedes_index_conversion_but_child_is_not_read() {
		let mut s = Store::new(Limits::default()).unwrap();
		let child = s.string("moved").unwrap();
		let source = s.list(Type::Tuple, &[child]).unwrap();
		let moved = s.take_capture(child).unwrap();
		s.release(moved).unwrap();
		let result = s
			.collection_get(Method::TupleGet, source, Value::I64(0))
			.unwrap();
		assert_eq!(payload(&s, result), Some(child));
		assert!(s.text(child).is_err());
		let moved = s.take_capture(source).unwrap();
		s.release(moved).unwrap();
		let error = s
			.collection_get(Method::TupleGet, source, Value::Bool(false))
			.unwrap_err();
		assert_eq!(
			error.detail,
			Detail::MovedArgument {
				arg: 0,
				access: MovedAccess::Read
			}
		);
		for value in [child, source, result] {
			s.release(value).unwrap();
		}
		assert_eq!((s.live_entries(), s.payload_bytes(), s.edges), (0, 0, 0));
	}

	#[test]
	fn collection_get_slice_admission_rolls_back_children_and_option_failure() {
		let mut s = Store::new(Limits::default()).unwrap();
		let child = s.string("child").unwrap();
		let source = s.list(Type::Vec, &[child, child]).unwrap();
		let range = s.range(0, 2).unwrap();
		let before = (
			s.live_entries(),
			s.payload_bytes(),
			s.edges,
			s.test_strong(child),
		);
		let error = limit::with(0, || {
			s.collection_get(Method::VecGet, source, range).unwrap_err()
		})
		.call();
		assert_eq!(error.kind, ErrorKind::Allocation);
		assert_eq!(
			(
				s.live_entries(),
				s.payload_bytes(),
				s.edges,
				s.test_strong(child)
			),
			before
		);
		let count = s.test_strong(child);
		s.test_set_strong(child, u32::MAX - 1);
		assert_eq!(
			s.collection_get(Method::VecGet, source, range)
				.unwrap_err()
				.kind,
			ErrorKind::Overflow
		);
		assert_eq!(s.test_strong(child), u32::MAX - 1);
		s.test_set_strong(child, count);
		// Exactly one free entry: the slice fits, its Some wrapper does not.
		s.limits.entries = s.live_entries() + 1;
		assert_eq!(
			s.collection_get(Method::VecGet, source, range)
				.unwrap_err()
				.kind,
			ErrorKind::StoreCapacity
		);
		assert_eq!(
			(
				s.live_entries(),
				s.payload_bytes(),
				s.edges,
				s.test_strong(child)
			),
			before
		);
		for value in [child, source, range] {
			s.release(value).unwrap();
		}
		assert_eq!((s.live_entries(), s.payload_bytes(), s.edges), (0, 0, 0));
	}
}
