//! Private immutable compilation epochs, never inferred from host hashes.
use core::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

static NEXT_CONTEXT: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Epoch {
	identity: u64,
	revision: u64,
}
impl crate::alloc::clone::TryClone for Epoch {
	fn try_clone(&self) -> crate::alloc::Result<Self> {
		Ok(*self)
	}
}

#[derive(Default)]
pub(crate) struct State {
	identity: OnceLock<Option<u64>>,
	revision: u64,
}
impl State {
	pub(crate) fn snapshot(&self) -> crate::alloc::Result<Epoch> {
		let identity = self.identity.get_or_init(|| {
			NEXT_CONTEXT
				.try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
				.ok()
		});
		Ok(Epoch {
			identity: identity.ok_or(crate::alloc::Error::CapacityOverflow)?,
			revision: self.revision,
		})
	}
	pub(crate) fn invalidate(&mut self) -> crate::alloc::Result<()> {
		self.revision = self
			.revision
			.checked_add(1)
			.ok_or(crate::alloc::Error::CapacityOverflow)?;
		Ok(())
	}
}
