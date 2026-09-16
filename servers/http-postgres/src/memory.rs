//! Tracked live allocation request bytes: the sizes live allocations asked
//! for, as seen by Rust's global allocator.
//!
//! This is not resident memory and not a bound on it in either direction.
//! It does not see allocations made by native code that bypasses this
//! allocator, memory mappings, thread stacks, or anything belonging to child
//! processes. It is not an accounting of what the session owns; it is a
//! process-wide measurement, not a memory ceiling.
//!
//! This standalone binary disables rnx's optional allocator and owns this one.
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
static PEAK: AtomicUsize = AtomicUsize::new(0);
pub fn peak() -> usize {
	PEAK.load(Ordering::Relaxed)
}
pub fn reset_peak() {
	PEAK.store(LIVE.live(), Ordering::Relaxed);
}
/// The global allocator. Its hooks allocate nothing and cannot unwind: each
/// adjusts one relaxed atomic and delegates to the system allocator.
pub struct Counting;
unsafe impl GlobalAlloc for Counting {
	unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
		let pointer = unsafe { System.alloc(layout) };
		LIVE.allocated(pointer, layout.size());
		PEAK.fetch_max(LIVE.live(), Ordering::Relaxed);
		pointer
	}
	unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
		let pointer = unsafe { System.alloc_zeroed(layout) };
		LIVE.allocated(pointer, layout.size());
		PEAK.fetch_max(LIVE.live(), Ordering::Relaxed);
		pointer
	}
	unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
		LIVE.deallocated(layout.size());
		unsafe { System.dealloc(pointer, layout) }
	}
	unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
		let new_pointer = unsafe { System.realloc(pointer, layout, new_size) };
		LIVE.reallocated(new_pointer, layout.size(), new_size);
		PEAK.fetch_max(LIVE.live(), Ordering::Relaxed);
		new_pointer
	}
}

pub fn live() -> usize {
	LIVE.live()
}
