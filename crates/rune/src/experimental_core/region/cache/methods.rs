//! Method cache ownership. Continuations, record validation and native ABI are shared
//! with scalar recursion; one per-code descriptor owns all retained artifacts.
use super::super::recursion::{
	methods::{entry, Methods},
	Outcome,
};
use super::*;
use crate::experimental_native::boundary::NativeRecursion;

pub(crate) struct MethodReady {
	pub(crate) shape: Methods,
	native: NativeRecursion,
	poisoned: Cell<bool>,
	entries: Cell<u64>,
	completes: Cell<u64>,
	replays: Cell<u64>,
	calls: Cell<u64>,
	peak: Cell<u64>,
	replay_work: Cell<u64>,
	helpers: Cell<u64>,
	recursive_calls: Cell<u64>,
	scratch_bytes: usize,
	_reservation: Token<'static>,
}
impl MethodReady {
	#[inline(never)]
	pub(crate) fn enter(
		&self,
		code: &Code,
		window: &mut [Value],
		store: &mut Store,
		canceled: bool,
		fuel: usize,
		room: Room,
	) -> Outcome {
		if self.poisoned.get() {
			return Outcome::Refused;
		}
		let (outcome, counts) = entry::enter(
			&self.shape,
			code,
			window,
			store,
			canceled,
			fuel,
			room,
			self.scratch_bytes,
			|c, r, s| {
				self.entries.set(self.entries.get().saturating_add(1));
				self.native.call(c, r, s)
			},
		);
		match outcome {
			Outcome::Complete { calls, peak, .. } => {
				self.completes.set(self.completes.get().saturating_add(1));
				self.calls.set(self.calls.get().saturating_add(calls));
				self.peak.set(self.peak.get().max(peak));
				self.helpers
					.set(self.helpers.get().saturating_add(counts.helpers));
				self.recursive_calls
					.set(self.recursive_calls.get().saturating_add(counts.recursive));
			}
			Outcome::Replay { work } => {
				self.replays.set(self.replays.get().saturating_add(1));
				self.replay_work
					.set(self.replay_work.get().saturating_add(work));
			}
			Outcome::Invalid => {
				self.poisoned.set(true);
				INVALID_EXITS.with(|c| c.set(c.get() + 1));
				std::eprintln!(
					"invalid native method exit: f{}",
					self.shape.engine.function
				);
				debug_assert!(false, "invalid native method exit");
			}
			Outcome::Refused => (),
		}
		outcome
	}
}
impl Build<'_> {
	pub(super) fn load_methods(&mut self, held: Held<'_>, shape: Methods) {
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
		if self.capped() || shape.engine.function as usize >= self.function_limit {
			return;
		}

		if !self.reserve_descriptor() {
			return;
		}
		let metadata = shape
			.metadata()
			.saturating_add(core::mem::size_of::<MethodReady>())
			.saturating_add(MODULE_RESERVE_BYTES)
			.saturating_add(if self.method_ready.is_empty() {
				self.function_limit * 4
			} else {
				0
			});
		if self.program_metadata.saturating_add(metadata) > self.limits.program_metadata_bytes
			|| self.method_ready.try_reserve_exact(1).is_err()
		{
			self.summary.metadata += 1;
			return;
		}
		self.compiles += 1;
		let offset =
			MAX_PROGRAM_CODE_BYTES - self.limits.program_code_bytes.min(MAX_PROGRAM_CODE_BYTES);
		let started = Instant::now();
		let compiled = std::panic::catch_unwind(core::panic::AssertUnwindSafe(|| {
			lower::recursion::compile_methods(&shape, self.program_bytes.saturating_add(offset))
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
		if self.method_ready.is_empty() {
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
		if self.method_ready.is_empty() {
			self.method_lookup = lookup.into_std();
		}
		self.method_lookup[shape.engine.function as usize] = self.method_ready.len() as u32;
		self.method_ready.push(MethodReady {
			shape,
			native,
			poisoned: Cell::new(false),
			entries: Cell::new(0),
			completes: Cell::new(0),
			replays: Cell::new(0),
			calls: Cell::new(0),
			peak: Cell::new(0),
			replay_work: Cell::new(0),
			helpers: Cell::new(0),
			recursive_calls: Cell::new(0),
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

pub(crate) struct MethodProgram {
	pub(crate) loops: ProgramRegions,
	pub(super) scalar: StdVec<RecursiveReady>,
	pub(super) scalar_lookup: StdVec<u32>,
	pub(super) methods: StdVec<MethodReady>,
	pub(super) lookup: StdVec<u32>,
}
pub(crate) enum Entry<'a> {
	Scalar(&'a RecursiveReady),
	Method(&'a MethodReady),
}
/// One statically selected metadata pointer, with no extra closure capture on
/// scalar-only invocations. This is not a dynamically dispatched interface.
pub(crate) trait EntryProgram {
	const METHODS: bool;
	fn entry(&self, function: u32) -> Option<Entry<'_>>;
}
impl EntryProgram for RecursiveProgram {
	const METHODS: bool = false;
	#[inline(always)]
	fn entry(&self, function: u32) -> Option<Entry<'_>> {
		self.function(function).map(Entry::Scalar)
	}
}
impl EntryProgram for MethodProgram {
	const METHODS: bool = true;
	#[inline(always)]
	fn entry(&self, function: u32) -> Option<Entry<'_>> {
		self.function(function)
	}
}
impl Entry<'_> {
	#[inline(always)]
	pub(crate) fn enter(
		&self,
		code: &Code,
		window: &mut [Value],
		store: &mut Store,
		canceled: bool,
		fuel: usize,
		room: Room,
	) -> Outcome {
		match self {
			Self::Scalar(r) => r.enter(window, store, canceled, fuel, room),
			Self::Method(r) => r.enter(code, window, store, canceled, fuel, room),
		}
	}
}
impl MethodProgram {
	pub(crate) fn function(&self, id: u32) -> Option<Entry<'_>> {
		if let Some(r) = self
			.lookup
			.get(id as usize)
			.and_then(|&i| self.methods.get(i as usize))
		{
			return Some(Entry::Method(r));
		}
		self.scalar_lookup
			.get(id as usize)
			.and_then(|&i| self.scalar.get(i as usize))
			.map(Entry::Scalar)
	}
}
#[cfg(test)]
pub(crate) fn counters(code: u32) -> StdVec<(u32, u64, u64, u64, u64, u64, u64, u64, u64)> {
	CACHE.with(|cache| {
		let cache = cache.borrow();
		let Some(index) = cache.position(code) else {
			return StdVec::new();
		};
		let Resident::Ready(program) = &cache.entries[index].resident else {
			return StdVec::new();
		};
		let ProgramDescriptor::Methods(program) = &program[0] else {
			return StdVec::new();
		};
		program
			.methods
			.iter()
			.map(|r| {
				(
					r.shape.engine.function,
					r.entries.get(),
					r.completes.get(),
					r.replays.get(),
					r.calls.get(),
					r.helpers.get(),
					r.recursive_calls.get(),
					r.peak.get(),
					r.replay_work.get(),
				)
			})
			.collect()
	})
}
