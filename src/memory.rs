//! Tracked live allocation request bytes: the sizes live allocations asked
//! for, as seen by Rust's global allocator.
//!
//! This is not resident memory and not a bound on it in either direction.
//! It does not see allocations made by native code that bypasses this
//! allocator, memory mappings, thread stacks, or anything belonging to child
//! processes. It is not an accounting of what the session owns; it is a
//! process-wide figure a ceiling is enforced against.
//!
//! With the feature compiled out none of this is installed; the pieces stay
//! so the build differs only in whether the allocator is in place.
#![cfg_attr(not(feature = "count-allocations"), allow(dead_code))]
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

/// One counter, adjusted by each event, so a sample is one load of one
/// value rather than an arithmetic combination of separately loaded totals
/// that allocations elsewhere could move between the loads.
pub struct Counter {
	live: AtomicUsize,
}
impl Counter {
	pub const fn new() -> Self {
		Self {
			live: AtomicUsize::new(0),
		}
	}
	pub fn live(&self) -> usize {
		self.live.load(Ordering::Relaxed)
	}
	/// An allocation that returned `pointer`. A null pointer is a failure and
	/// changes nothing.
	pub fn allocated(&self, pointer: *mut u8, size: usize) {
		if !pointer.is_null() {
			self.live.fetch_add(size, Ordering::Relaxed);
		}
	}
	/// A deallocation of a block of `size`.
	pub fn deallocated(&self, size: usize) {
		self.live.fetch_sub(size, Ordering::Relaxed);
	}
	/// A reallocation from `old` to `new` bytes that returned `pointer`. A
	/// null pointer means the reallocation failed, the original block still
	/// stands, and the counter does not move.
	pub fn reallocated(&self, pointer: *mut u8, old: usize, new: usize) {
		if pointer.is_null() {
			return;
		}
		if new >= old {
			self.live.fetch_add(new - old, Ordering::Relaxed);
		} else {
			self.live.fetch_sub(old - new, Ordering::Relaxed);
		}
	}
}

static LIVE: Counter = Counter::new();
/// The reference point recorded once, at startup. `usize::MAX` means it has
/// not been recorded yet.
static BASELINE: AtomicUsize = AtomicUsize::new(usize::MAX);

/// The global allocator. Its hooks allocate nothing and cannot unwind: each
/// adjusts one relaxed atomic and delegates to the system allocator.
pub struct Counting;
unsafe impl GlobalAlloc for Counting {
	unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
		let pointer = unsafe { System.alloc(layout) };
		LIVE.allocated(pointer, layout.size());
		pointer
	}
	unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
		let pointer = unsafe { System.alloc_zeroed(layout) };
		LIVE.allocated(pointer, layout.size());
		pointer
	}
	unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
		LIVE.deallocated(layout.size());
		unsafe { System.dealloc(pointer, layout) }
	}
	unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
		let new_pointer = unsafe { System.realloc(pointer, layout, new_size) };
		LIVE.reallocated(new_pointer, layout.size(), new_size);
		new_pointer
	}
}

/// Whether allocation accounting is compiled in. A build without it enforces
/// no ceiling and reports no figure.
pub const ACCOUNTED: bool = cfg!(feature = "count-allocations");

/// Tracked live allocation request bytes, or `None` in a build with
/// accounting compiled out.
pub fn live() -> Option<usize> {
	ACCOUNTED.then(|| LIVE.live())
}
/// Record the startup reference point. Called once, after initialization and
/// after history is loaded, before the first evaluation is admitted; later
/// calls are ignored, so a reset can never establish a new one.
pub fn record_baseline() {
	if let Some(live) = live() {
		let _ = BASELINE.compare_exchange(usize::MAX, live, Ordering::Relaxed, Ordering::Relaxed);
	}
}
/// The startup reference point, if one was recorded.
pub fn baseline() -> Option<usize> {
	match BASELINE.load(Ordering::Relaxed) {
		usize::MAX => None,
		value => Some(value),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// The counter's arithmetic, tested on its own instance so the result
	/// does not depend on what the rest of the process is allocating.
	#[test]
	fn every_event_is_accounted_and_a_failure_is_not() {
		let counter = Counter::new();
		let pointer = 1usize as *mut u8;
		let failed = std::ptr::null_mut();

		counter.allocated(pointer, 100);
		assert_eq!(counter.live(), 100);
		// A zeroed allocation is an allocation.
		counter.allocated(pointer, 20);
		assert_eq!(counter.live(), 120);
		// An allocation that returns null changes nothing.
		counter.allocated(failed, 4096);
		assert_eq!(counter.live(), 120);
		// Growing adds the difference, shrinking subtracts it.
		counter.reallocated(pointer, 100, 250);
		assert_eq!(counter.live(), 270);
		counter.reallocated(pointer, 250, 50);
		assert_eq!(counter.live(), 70);
		// A reallocation of the same size moves nothing.
		counter.reallocated(pointer, 50, 50);
		assert_eq!(counter.live(), 70);
		// A failed reallocation leaves the original block and the counter.
		counter.reallocated(failed, 50, 1_000_000);
		assert_eq!(counter.live(), 70);
		counter.deallocated(50);
		counter.deallocated(20);
		assert_eq!(counter.live(), 0);
	}

	// The installed counter is exercised in its own process, by the payload
	// gate in `tests/repl.rs`. A process-wide comparison inside this parallel
	// suite would be at the mercy of what other tests allocate.
}
