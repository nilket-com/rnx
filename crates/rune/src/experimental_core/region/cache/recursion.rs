//! N5 cache loading and counters; old Ready and loop entry remain unchanged.
use super::super::recursion::{self, Outcome, Shape};
use super::*;
use crate::experimental_native::boundary::NativeRecursion;

pub(crate) struct RecursiveReady {
	pub(crate) shape: Shape,
	native: NativeRecursion,
	poisoned: Cell<bool>,
	pub(crate) entries: Cell<u64>,
	pub(crate) completes: Cell<u64>,
	pub(crate) replays: Cell<u64>,
	pub(crate) calls: Cell<u64>,
	pub(crate) peak: Cell<u64>,
	pub(crate) replay_work: Cell<u64>,
	scratch_bytes: usize,
	_reservation: Token<'static>,
}
impl RecursiveReady {
	#[inline(never)]
	pub(crate) fn enter(
		&self,
		window: &mut [Value],
		store: &Store,
		canceled: bool,
		fuel: usize,
		room: Room,
	) -> Outcome {
		if self.poisoned.get() {
			return Outcome::Refused;
		}
		let outcome = recursion::enter(
			&self.shape,
			window,
			store,
			canceled,
			fuel,
			room,
			self.scratch_bytes,
			|b, r, s| {
				self.entries.set(self.entries.get().saturating_add(1));
				self.native.call(b, r, s)
			},
		);
		match outcome {
			Outcome::Complete { calls, peak, .. } => {
				self.completes.set(self.completes.get().saturating_add(1));
				self.calls.set(self.calls.get().saturating_add(calls));
				self.peak.set(self.peak.get().max(peak));
			}
			Outcome::Replay { work } => {
				self.replays.set(self.replays.get().saturating_add(1));
				self.replay_work
					.set(self.replay_work.get().saturating_add(work));
			}
			Outcome::Invalid => {
				self.poisoned.set(true);
				INVALID_EXITS.with(|c| c.set(c.get() + 1));
				std::eprintln!("invalid native recursion exit: f{}", self.shape.function);
				debug_assert!(false, "invalid native recursion exit");
			}
			Outcome::Refused => (),
		}
		outcome
	}
}
impl Build<'_> {
	pub(super) fn load_recursion(&mut self, held: Held<'_>, shape: Shape) {
		match self.enabled {
			Enabled::Yes => (),
			Enabled::Environment => {
				self.summary.environment += 1;
				return;
			}
			Enabled::PageSize => {
				self.summary.page_size += 1;
				return;
			}
		}
		if self.capped() || shape.function as usize >= self.function_limit {
			return;
		}

		let metadata = shape
			.metadata()
			.saturating_add(core::mem::size_of::<RecursiveReady>())
			.saturating_add(MODULE_RESERVE_BYTES)
			.saturating_add(if self.recursive.is_empty() {
				core::mem::size_of::<RecursiveProgram>() + self.function_limit * 4
			} else {
				0
			});
		if self.program_metadata.saturating_add(metadata) > self.limits.program_metadata_bytes
			|| self.recursive.try_reserve_exact(1).is_err()
		{
			self.summary.metadata += 1;
			return;
		}
		self.compiles += 1;
		let offset =
			MAX_PROGRAM_CODE_BYTES - self.limits.program_code_bytes.min(MAX_PROGRAM_CODE_BYTES);
		let started = Instant::now();
		let compiled = std::panic::catch_unwind(core::panic::AssertUnwindSafe(|| {
			lower::recursion::compile(&shape, self.program_bytes.saturating_add(offset))
		}));
		self.summary.compile_ns += elapsed_ns(started);
		let compiled = match compiled {
			Ok(Ok(c)) => c,
			Ok(Err(_)) => {
				self.summary.other += 1;
				return;
			}
			Err(_) => {
				self.summary.panicked += 1;
				return;
			}
		};
		let bytes = compiled.code_bytes();
		let mapped = page_ceil(bytes);
		let id = self.id;
		let mut evicted = StdVec::new();
		let reserved = CACHE.with(|c| {
			c.borrow_mut()
				.reserve(self.limits, id, mapped, metadata, &mut evicted)
		});
		drop(evicted);
		if !reserved {
			self.summary.thread_bound += 1;
			return;
		}
		let Some(reservation) = PROCESS.reserve(
			mapped,
			self.limits.process_mappings,
			self.limits.process_mapped_bytes,
		) else {
			unreserve(id, mapped, metadata);
			self.summary.process_bound += 1;
			return;
		};

		// The complete dense lookup allocation is reserved as metadata BEFORE
		// allocation. Only admitted recursive programs allocate it. Lookup is O(1).
		let mut lookup = crate::alloc::Vec::new();
		if self.recursive.is_empty() {
			let allocated = crate::alloc::Vec::try_with_capacity(self.function_limit);
			let Ok(mut table) = allocated else {
				drop(reservation);
				unreserve(id, mapped, metadata);
				self.summary.metadata += 1;
				return;
			};
			if table.capacity() != self.function_limit
				|| table.try_resize(self.function_limit, u32::MAX).is_err()
			{
				drop(table);
				drop(reservation);
				unreserve(id, mapped, metadata);
				self.summary.metadata += 1;
				return;
			}
			lookup = table;
		}
		let started = Instant::now();
		let loaded = std::panic::catch_unwind(core::panic::AssertUnwindSafe(|| {
			NativeRecursion::load(&compiled)
		}));
		self.summary.load_ns += elapsed_ns(started);
		let native = match loaded {
			Ok(Ok(n)) => n,
			Ok(Err(_)) => {
				drop(lookup);
				drop(reservation);
				unreserve(id, mapped, metadata);
				self.summary.load_refused += 1;
				return;
			}
			Err(_) => {
				drop(lookup);
				core::mem::forget(reservation);
				unreserve(id, mapped, metadata);
				self.summary.load_panicked += 1;
				return;
			}
		};
		// Buffers now retained by the exact metadata reservation, never uncharged.
		if self.recursive.is_empty() {
			self.recursive_lookup = lookup.into_std();
		}
		self.recursive_lookup[shape.function as usize] = self.recursive.len() as u32;
		self.recursive.push(RecursiveReady {
			shape,
			native,
			poisoned: Cell::new(false),
			entries: Cell::new(0),
			completes: Cell::new(0),
			replays: Cell::new(0),
			calls: Cell::new(0),
			peak: Cell::new(0),
			replay_work: Cell::new(0),
			scratch_bytes: self.limits.transient_analysis_bytes,
			_reservation: reservation,
		});
		drop(held);
		self.program_bytes += bytes;
		self.program_metadata += metadata;
		self.summary.loaded += 1;
		self.summary.code_bytes += bytes as u64;
		self.summary.mapped_bytes += mapped as u64;
		self.summary.metadata_bytes += (metadata - MODULE_RESERVE_BYTES) as u64;
		self.summary.reserved_bytes += MODULE_RESERVE_BYTES as u64;
	}
}

#[cfg(test)]
pub(crate) fn counters(code: u32) -> StdVec<(u64, u64, u64, u64, u64, u64)> {
	RECURSIVE.with(|table| {
		let table = table.borrow();
		let Some(program) = table
			.iter()
			.flatten()
			.find(|s| s.code == code)
			.and_then(|s| s.program.as_ref())
		else {
			return StdVec::new();
		};
		program[0]
			.recursive
			.iter()
			.map(|r| {
				(
					r.entries.get(),
					r.completes.get(),
					r.replays.get(),
					r.calls.get(),
					r.peak.get(),
					r.replay_work.get(),
				)
			})
			.collect()
	})
}

/// Ordinary ProgramRegions, Guard and Ready retain their N4 layouts. Only an
/// admitted recursive program owns this extra allocation and checkout path.
pub(crate) struct RecursiveProgram {
	pub(super) loops: ProgramRegions,
	pub(super) recursive: StdVec<RecursiveReady>,
	pub(super) lookup: StdVec<u32>,
}
impl RecursiveProgram {
	pub(crate) fn function(&self, id: u32) -> Option<&RecursiveReady> {
		self.lookup
			.get(id as usize)
			.and_then(|&i| self.recursive.get(i as usize))
	}
}

// Fixed handles share the existing cache's maximum program bound. Program
// allocations remain fallible and charged by load_recursion. A checked-out slot
// keeps its code key, so nested calls cannot reuse another guard's slot.
struct Slot {
	code: u32,
	program: Option<StdVec<RecursiveProgram>>,
}
std::thread_local! {
	static RECURSIVE: RefCell<[Option<Slot>; MAX_CACHED_PROGRAMS]> = const {
		RefCell::new([const { None }; MAX_CACHED_PROGRAMS])
	};
}
pub(super) const fn table_bytes() -> usize {
	core::mem::size_of::<[Option<Slot>; MAX_CACHED_PROGRAMS]>()
}
pub(super) fn install(program: StdVec<RecursiveProgram>) -> bool {
	RECURSIVE.with(|table| {
		let mut table = table.borrow_mut();
		let Some(slot) = table.iter_mut().find(|slot| slot.is_none()) else {
			return false;
		};
		*slot = Some(Slot {
			code: program[0].loops.code,
			program: Some(program),
		});
		true
	})
}
pub(super) fn evict(code: u32) {
	let removed = RECURSIVE.try_with(|table| {
		let mut table = table.borrow_mut();
		table
			.iter_mut()
			.find(|s| s.as_ref().is_some_and(|s| s.code == code))
			.and_then(Option::take)
	});
	// Dropping private executable buffers never calls the engine or this cache.
	drop(removed);
}
#[cfg(test)]
pub(super) fn clear() {
	let removed = RECURSIVE.with(|table| {
		core::mem::replace(
			&mut *table.borrow_mut(),
			[const { None }; MAX_CACHED_PROGRAMS],
		)
	});
	drop(removed);
}
pub(crate) fn checkout(code: &Code) -> Option<RecursiveGuard> {
	CACHE.with(|cache| {
		let mut cache = cache.borrow_mut();
		let i = cache.position(code.id())?;
		if !matches!(cache.entries[i].resident, Resident::Empty(_)) {
			return None;
		}
		let guard = RECURSIVE.with(|table| {
			let mut table = table.borrow_mut();
			let (index, slot) = table
				.iter_mut()
				.enumerate()
				.find_map(|(i, s)| s.as_mut().filter(|s| s.code == code.id()).map(|s| (i, s)))?;
			Some(RecursiveGuard {
				program: Some(slot.program.take()?),
				index,
			})
		})?;
		cache.entries[i].resident = Resident::CheckedOut;
		Some(guard)
	})
}
pub(crate) struct RecursiveGuard {
	program: Option<StdVec<RecursiveProgram>>,
	index: usize,
}
impl RecursiveGuard {
	pub(crate) fn regions(&self) -> Option<&ProgramRegions> {
		self.recursive().map(|p| &p.loops)
	}
	pub(crate) fn recursive(&self) -> Option<&RecursiveProgram> {
		self.program.as_ref().and_then(|p| p.first())
	}
}
impl Drop for RecursiveGuard {
	fn drop(&mut self) {
		let Some(program) = self.program.take() else {
			return;
		};
		let _ = CACHE.try_with(|cache| {
			if let Ok(mut cache) = cache.try_borrow_mut() {
				let code = program[0].loops.code;
				if let Some(i) = cache.position(code) {
					if matches!(cache.entries[i].resident, Resident::CheckedOut) {
						RECURSIVE.with(|table| {
							let mut table = table.borrow_mut();
							if let Some(slot) = table[self.index]
								.as_mut()
								.filter(|s| s.code == code && s.program.is_none())
							{
								let summary = program[0].loops.summary;
								slot.program = Some(program);
								cache.tick += 1;
								cache.entries[i].last_use = cache.tick;
								cache.entries[i].resident = Resident::Empty(summary);
							}
						});
					}
				}
			}
		});
	}
}
