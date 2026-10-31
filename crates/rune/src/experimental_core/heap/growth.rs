//! The checked payload growth policy shared by interpreted and native scalar Push.
use super::{err, ErrorKind, Result, Value};
use core::mem::size_of;

#[derive(Clone, Copy, Debug)]
pub(super) struct Growth {
	pub(super) capacity: usize,
	pub(super) bytes: usize,
}

impl Growth {
	#[inline(always)]
	pub(super) fn next(capacity: usize) -> Result<Self> {
		let capacity = capacity
			.checked_mul(2)
			.ok_or_else(|| err(ErrorKind::Overflow))?
			.max(1);
		let bytes = Self::bytes(capacity)?;
		Ok(Self { capacity, bytes })
	}

	#[inline(always)]
	pub(super) fn bytes(capacity: usize) -> Result<usize> {
		capacity
			.checked_mul(size_of::<Value>())
			.ok_or_else(|| err(ErrorKind::Overflow))
	}

	/// Additional live allocation at the replacement's allocation point. The old
	/// payload has not been dropped yet. The allocator applies this same peak by
	/// charging the new allocation before releasing the old one.
	#[cfg(feature = "native-code")]
	pub(super) fn peak(self, old_capacity: usize, original: usize) -> Option<usize> {
		Self::bytes(old_capacity)
			.ok()?
			.checked_add(self.bytes)?
			.checked_sub(Self::bytes(original).ok()?)
	}
}
