//! Sealed ownership for persistent cross-program execution. No raw roots escape.
use super::{
	error::{Error, ErrorKind, Result},
	execute::{Executor, ExecutorLimits},
	frontend::{self, Program},
	heap::{Limits, Store},
	provenance::Epoch,
	value::{ReleaseError, Root, Scalar},
};
use crate::{
	alloc::{String, Vec},
	runtime::RuntimeContext,
	sync::{Arc, Weak},
	Context, Diagnostics, Sources,
};
use core::cell::RefCell;

fn invalid() -> Error {
	Error::new(ErrorKind::InvalidHandle, "program domain")
}

struct Slot {
	generation: u64,
	root: Option<Root>,
	queued: bool,
	next: Option<usize>,
}
#[derive(Default)]
struct Registry {
	slots: Vec<Slot>,
	pending: Option<usize>,
	closed: bool,
}
impl Registry {
	fn enqueue(&mut self, index: usize, generation: u64) {
		if self.closed {
			return;
		}
		if let Some(slot) = self.slots.get_mut(index) {
			if slot.generation == generation && slot.root.is_some() && !slot.queued {
				slot.queued = true;
				slot.next = self.pending;
				self.pending = Some(index);
			}
		}
	}
}

/// An affine root owned by one domain. Drop queues release without allocation.
pub struct DomainRoot {
	registry: Weak<RefCell<Registry>>,
	index: usize,
	generation: u64,
}
impl core::fmt::Debug for DomainRoot {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		f.debug_struct("DomainRoot")
			.field("slot", &self.index)
			.finish_non_exhaustive()
	}
}
impl Drop for DomainRoot {
	fn drop(&mut self) {
		if let Some(registry) = self.registry.upgrade() {
			registry.borrow_mut().enqueue(self.index, self.generation);
		}
	}
}

/// Admission refusal or a terminal setup failure with cleanup evidence.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum StartError {
	/// Nothing has been transferred; only a typed Refused permits fallback.
	Prepare(super::execute::PrepareError),
	/// Setup began; cleanup is complete or its failure is preserved here.
	Failed(super::execute::Failure),
}
impl From<Error> for StartError {
	fn from(error: Error) -> Self {
		Self::Prepare(error.into())
	}
}
impl From<super::execute::PrepareError> for StartError {
	fn from(error: super::execute::PrepareError) -> Self {
		Self::Prepare(error)
	}
}

/// A program lease; explicitly release it when no longer needed as an entry.
#[derive(Debug)]
pub struct ProgramKey {
	domain: u64,
	code: u32,
}
pub(crate) struct Member {
	pub(crate) program: Program,
	leased: bool,
}

/// Result of a complete, transactional program collection.
#[derive(Debug)]
pub struct CollectionReport {
	/// Number of released programs.
	pub released: usize,
	/// Number of programs still owned by the domain.
	pub retained: usize,
	/// Explicit program leases that keep programs alive.
	pub leases: usize,
	/// Pending root releases (zero after a successful drain).
	pub pending_releases: usize,
}

/// Owns a new Store, its exported roots, and compatible compiled programs.
pub struct ProgramDomain {
	store: Store,
	executor: Executor,
	epoch: Epoch,
	registry: Arc<RefCell<Registry>>,
	programs: Vec<Member>,
	id: u64,
}
impl ProgramDomain {
	/// A default/unstamped runtime is not eligible for cross-program execution.
	pub fn new(
		limits: Limits,
		execution: ExecutorLimits,
		runtime: Arc<RuntimeContext>,
	) -> Result<Self> {
		let epoch = runtime.execution_epoch.ok_or_else(invalid)?;
		let store = Store::new(limits)?;
		let id = store.domain_identity();
		let mut executor = Executor::with_limits(execution)?;
		executor.set_host_runtime(Some(runtime.clone()))?;
		Ok(Self {
			store,
			executor,
			epoch,
			registry: Arc::try_new(RefCell::new(Registry::default()))?,
			programs: Vec::new(),
			id,
		})
	}
	fn drain(&mut self) -> Result<()> {
		loop {
			let next = {
				let mut r = self.registry.borrow_mut();
				if r.closed {
					return Err(Error::new(ErrorKind::ClosedStore, "program domain"));
				}
				let Some(index) = r.pending else {
					return Ok(());
				};
				let slot = &mut r.slots[index];
				let next = slot.next.take();
				let root = slot.root.take().ok_or_else(invalid)?;
				slot.queued = false;
				r.pending = next;
				(index, root)
			};
			#[cfg(test)]
			let released = drain_injection::release(&mut self.store, next.1);
			#[cfg(not(test))]
			let released = self.store.root_release(next.1);
			match released {
				Ok(()) => {}
				Err(ReleaseError::Refused { root, error }) => {
					let mut r = self.registry.borrow_mut();
					let generation = r.slots[next.0].generation;
					r.slots[next.0].root = Some(root);
					r.enqueue(next.0, generation);
					return Err(error);
				}
				Err(ReleaseError::Failed(error)) => return Err(error),
			}
		}
	}
	fn check<'a>(&self, registry: &'a Registry, root: &DomainRoot) -> Result<&'a Root> {
		let other = root.registry.upgrade().ok_or_else(invalid)?;
		if !Arc::ptr_eq(&other, &self.registry) || registry.closed {
			return Err(invalid());
		}
		let slot = registry.slots.get(root.index).ok_or_else(invalid)?;
		if slot.generation != root.generation || slot.queued {
			return Err(invalid());
		}
		slot.root.as_ref().ok_or_else(invalid)
	}
	fn reserve_root(&self) -> Result<(usize, u64)> {
		let mut r = self.registry.borrow_mut();
		if r.closed {
			return Err(invalid());
		}
		if let Some((i, slot)) = r
			.slots
			.iter_mut()
			.enumerate()
			.find(|(_, s)| s.root.is_none() && !s.queued && s.generation != u64::MAX)
		{
			slot.generation += 1;
			return Ok((i, slot.generation));
		}
		let i = r.slots.len();
		r.slots.try_push(Slot {
			generation: 1,
			root: None,
			queued: false,
			next: None,
		})?;
		Ok((i, 1))
	}
	fn publish(&self, slot: (usize, u64), root: Root) -> DomainRoot {
		self.registry.borrow_mut().slots[slot.0].root = Some(root);
		DomainRoot {
			registry: Arc::downgrade(&self.registry),
			index: slot.0,
			generation: slot.1,
		}
	}
	/// Construct a scalar argument.
	pub fn i64(&mut self, value: i64) -> Result<DomainRoot> {
		self.drain()?;
		let slot = self.reserve_root()?;
		Ok(self.publish(slot, self.store.root_i64(value)))
	}
	/// Construct an empty session record.
	pub fn record(&mut self) -> Result<DomainRoot> {
		self.drain()?;
		let slot = self.reserve_root()?;
		let root = self.store.root_record(None, &[])?;
		Ok(self.publish(slot, root))
	}
	/// Build a record of retained aliases without exposing raw roots.
	pub fn record_from_fields(&mut self, fields: &[(&str, &DomainRoot)]) -> Result<DomainRoot> {
		self.drain()?;
		let slot = self.reserve_root()?;
		let root = {
			let registry = self.registry.borrow();
			let mut checked = Vec::new();
			checked.try_reserve(fields.len())?;
			for (name, root) in fields {
				checked.try_push((*name, self.check(&registry, root)?))?;
			}
			self.store.root_record(None, &checked)?
		};
		Ok(self.publish(slot, root))
	}
	/// Read a named field, registering its retained owner before returning.
	pub fn field(&mut self, root: &DomainRoot, name: &str) -> Result<DomainRoot> {
		self.drain()?;
		let slot = self.reserve_root()?;
		let result = {
			let registry = self.registry.borrow();
			self.store.root_field(self.check(&registry, root)?, name)?
		};
		Ok(self.publish(slot, result))
	}
	/// Bounded graph inspection; mutation and collection require an exclusive borrow.
	pub fn view(
		&self,
		root: &DomainRoot,
		limits: super::heap::ViewLimits,
	) -> Result<super::heap::RootView<'_>> {
		let registry = self.registry.borrow();
		self.store.root_view(self.check(&registry, root)?, limits)
	}
	/// Observations only: live programs, live registered roots, live Store objects.
	/// Call after collection: dropped handles remain registered until the drain.
	pub fn live_counts(&self) -> (usize, usize, usize) {
		(
			self.programs.len(),
			self.registry
				.borrow()
				.slots
				.iter()
				.filter(|s| s.root.is_some())
				.count(),
			self.store.live_entries(),
		)
	}
	/// Reusable program/root slots and Store bookkeeping bytes, separate from live owners.
	pub fn capacity_counts(&self) -> Result<(usize, usize, usize)> {
		Ok((
			self.programs.capacity(),
			self.registry.borrow().slots.capacity(),
			self.store.bookkeeping_bytes()?,
		))
	}

	/// Revisions still retained after collection, for host diagnostic-map pruning.
	pub fn retained_revisions(&self) -> Result<Vec<u64>> {
		let mut revisions = Vec::new();
		revisions.try_reserve(self.programs.len())?;
		for member in &self.programs {
			for source in member.program.sources() {
				revisions.try_push(source.revision)?;
			}
		}
		Ok(revisions)
	}
	/// Read a scalar without exposing its raw root.
	pub fn scalar(&self, root: &DomainRoot) -> Result<Option<Scalar>> {
		let r = self.registry.borrow();
		self.store.root_scalar(self.check(&r, root)?)
	}
	/// Format with the Store's bounded formatter.
	pub fn format(&self, root: &DomainRoot, debug: bool) -> Result<String> {
		let r = self.registry.borrow();
		self.store.root_format(self.check(&r, root)?, debug)
	}
	/// Retain a separately releasable alias.
	pub fn retain(&mut self, root: &DomainRoot) -> Result<DomainRoot> {
		self.drain()?;
		let slot = self.reserve_root()?;
		let value = {
			let r = self.registry.borrow();
			self.store.import(self.check(&r, root)?)?
		};
		self.store.retain(value)?;
		Ok(self.publish(slot, self.store.export(value)))
	}
	/// Extract a child and register it before returning it to the caller.
	pub fn get(&mut self, root: &DomainRoot, index: usize) -> Result<DomainRoot> {
		self.drain()?;
		let slot = self.reserve_root()?;
		let value = {
			let r = self.registry.borrow();
			self.store.import(self.check(&r, root)?)?
		};
		let temporary = self.store.export(value);
		let result = self.store.root_get(&temporary, index)?;
		Ok(self.publish(slot, result))
	}
	/// Explicitly release; a refusal leaves the queued root registered for recovery.
	pub fn release(&mut self, root: DomainRoot) -> Result<()> {
		self.drain()?;
		{
			let r = self.registry.borrow();
			self.check(&r, &root)?;
		}
		drop(root);
		self.drain()
	}
	/// Compile into this runtime epoch and keep an explicit entry lease.
	pub fn compile_insert(
		&mut self,
		context: &Context,
		sources: &mut Sources,
		diagnostics: &mut Diagnostics,
		revision: u64,
	) -> Result<ProgramKey> {
		self.drain()?;
		if context.execution_epoch.snapshot()? != self.epoch {
			return Err(invalid());
		}
		let program = frontend::compile(sources, context, diagnostics, revision)?;
		if program.epoch != Some(self.epoch) {
			return Err(invalid());
		}
		let code = program.code.id();
		self.programs.try_push(Member {
			program,
			leased: true,
		})?;
		Ok(ProgramKey {
			domain: self.id,
			code,
		})
	}
	/// Resolve a public entry hash without exposing Program ownership.
	pub fn function(&self, key: &ProgramKey, hash: crate::Hash) -> Result<u32> {
		self.entry(key, hash)
	}
	/// Look up an entry in a leased program. This never registers a host function.
	pub fn entry(&self, key: &ProgramKey, hash: crate::Hash) -> Result<u32> {
		if key.domain != self.id {
			return Err(invalid());
		}
		self.programs
			.iter()
			.find(|p| p.program.code.id() == key.code)
			.and_then(|p| p.program.function(hash))
			.ok_or_else(invalid)
	}
	/// Give up the explicit entry lease; values may still keep its program alive.
	pub fn release_program(&mut self, key: ProgramKey) -> Result<()> {
		self.drain()?;
		if key.domain != self.id {
			return Err(invalid());
		}
		self.programs
			.iter_mut()
			.find(|p| p.program.code.id() == key.code)
			.ok_or_else(invalid)?
			.leased = false;
		Ok(())
	}
	/// Collect only after all dropped handles have been released successfully.
	pub fn collect_programs(&mut self) -> Result<CollectionReport> {
		self.drain()?;
		let mut keep = {
			let r = self.registry.borrow();
			self.store
				.reachable_programs(r.slots.iter().filter_map(|s| s.root.as_ref()))?
		};
		let leases = self.programs.iter().filter(|p| p.leased).count();
		for p in self.programs.iter().filter(|p| p.leased) {
			keep.try_insert(p.program.code.id())?;
		}
		let before = self.programs.len();
		self.programs
			.retain(|p| keep.contains(&p.program.code.id()));
		Ok(CollectionReport {
			released: before - self.programs.len(),
			retained: self.programs.len(),
			leases,
			pending_releases: 0,
		})
	}
	/// Close even with outstanding handles; handles retain no Store or programs.
	pub fn close(&mut self) -> Result<()> {
		let drained = self.drain();
		self.registry.borrow_mut().closed = true;
		for slot in &mut self.registry.borrow_mut().slots {
			slot.root = None;
			slot.queued = false;
			slot.next = None;
		}
		self.registry.borrow_mut().pending = None;
		let closed = self.store.close();
		self.programs.clear();
		drained.and(closed)
	}
}
impl Drop for ProgramDomain {
	fn drop(&mut self) {
		let _ = self.close();
	}
}

/// One origin or caller resolved against its own defining program.
pub struct CrossFrame<'a> {
	/// Defining program, retained by the domain borrow.
	pub program: &'a Program,
	/// Source location in that program.
	pub location: Option<super::error::Location>,
}
/// A failure borrows the immutable program set until rendered or materialized.
#[allow(clippy::large_enum_variant)]
pub struct CrossFailure<'a> {
	/// Typed engine failure, independent of Store lifetime.
	/// Its single-program call_chain is empty; use frames() for resolved callers.
	pub failure: super::execute::Failure,
	program: Option<&'a Program>,
	frames: Vec<CrossFrame<'a>>,
	/// Allocation failure prevented a complete expanded chain.
	pub truncated: bool,
}
impl CrossFailure<'_> {
	/// Inspect the program-resolved origin and callers.
	pub fn frames(&self) -> &[CrossFrame<'_>] {
		&self.frames
	}
	/// Render while this borrow keeps the programs alive.
	pub fn message(&self) -> impl core::fmt::Display + '_ {
		struct Message<'a, 'b>(&'a CrossFailure<'b>);
		impl core::fmt::Display for Message<'_, '_> {
			fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
				if self.0.failure.error.detail == super::detail::Detail::ForeignFunction {
					f.write_str("function unavailable")?;
				} else if let Some(program) = self.0.program {
					write!(f, "{}", self.0.failure.message(program))?;
				} else {
					write!(f, "function unavailable")?;
				}
				for frame in &self.0.frames {
					if let Some(at) = frame.location {
						if let Some(source) = frame
							.program
							.sources()
							.iter()
							.find(|s| s.id == at.source && s.revision == at.revision)
						{
							write!(f, "\n  at {}:{}..{}", source.name, at.start, at.end)?;
						} else {
							write!(f, "\n  at unavailable source")?;
						}
					}
				}
				if self.0.truncated {
					write!(f, " (call chain capture truncated)")?;
				}
				Ok(())
			}
		}
		Message(self)
	}
	/// Copy text before releasing the diagnostic's program borrow.
	pub fn materialize(&self) -> crate::alloc::Result<String> {
		crate::alloc::fmt::try_format(format_args!("{}", self.message()))
	}
}
/// A soft suspension or terminal cross-program outcome.
#[allow(clippy::large_enum_variant)]
pub enum CrossOutcome<'a> {
	/// Soft allowance exhausted before the next charged instruction.
	Limited,
	/// Terminal result, registered in the owning domain.
	Complete(DomainRoot),
	/// Terminal error retaining its program-resolution borrow.
	Failed(CrossFailure<'a>),
}
/// Exclusive sliced execution in an owning domain.
pub struct CrossInvocation<'a> {
	inner: super::execute::SlicedInvocation<'a>,
	registry: &'a Arc<RefCell<Registry>>,
	slot: (usize, u64),
	programs: &'a [Member],
}
impl<'a> CrossInvocation<'a> {
	/// Hard fuel remaining, never replenished by a resume.
	pub fn remaining_work(&self) -> usize {
		self.inner.remaining_work()
	}
	fn failure(&self, mut failure: super::execute::Failure) -> CrossFailure<'a> {
		let (program, locations, mut truncated) = self.inner.cross_diagnostics();
		// A single-program chain cannot represent these source namespaces.
		failure.call_chain.clear();
		let mut frames = Vec::new();
		let origin_present =
			program.is_some_and(|p| locations.first().is_some_and(|(id, _)| *id == p.code.id()));
		if !origin_present {
			if let Some(program) = program {
				if frames
					.try_push(CrossFrame {
						program,
						location: failure.error.location,
					})
					.is_err()
				{
					truncated = true;
				}
			}
		}
		for (i, (code, location)) in locations.iter().enumerate() {
			if let Some(member) = self.programs.iter().find(|m| m.program.code.id() == *code) {
				if frames
					.try_push(CrossFrame {
						program: &member.program,
						location: if i == 0 && origin_present {
							failure.error.location
						} else {
							*location
						},
					})
					.is_err()
				{
					truncated = true;
					break;
				}
			} else {
				truncated = true;
			}
		}
		CrossFailure {
			failure,
			program,
			frames,
			truncated,
		}
	}
	/// Resume the shared sliced interpreter; never retry on another engine.
	pub fn resume(&mut self, slice: core::num::NonZeroUsize) -> CrossOutcome<'a> {
		match self.inner.resume(slice) {
			super::execute::SliceOutcome::Limited => CrossOutcome::Limited,
			super::execute::SliceOutcome::Failed(failure) => {
				CrossOutcome::Failed(self.failure(failure))
			}
			super::execute::SliceOutcome::Complete(root) => {
				self.registry.borrow_mut().slots[self.slot.0].root = Some(root);
				CrossOutcome::Complete(DomainRoot {
					registry: Arc::downgrade(self.registry),
					index: self.slot.0,
					generation: self.slot.1,
				})
			}
		}
	}
	/// Cancel once, preserving cleanup errors and already committed effects.
	pub fn cancel(&mut self) -> CrossFailure<'a> {
		let failure = self.inner.cancel();
		self.failure(failure)
	}
}
impl ProgramDomain {
	/// Validate and start an entry, consuming its lease only after admission.
	/// The borrowed key remains usable for retry or explicit release after refusal.
	#[allow(clippy::result_large_err)]
	pub fn start<'a>(
		&'a mut self,
		key: &ProgramKey,
		function: u32,
		args: &[&DomainRoot],
		fuel: usize,
	) -> core::result::Result<CrossInvocation<'a>, StartError> {
		self.drain()?;
		if key.domain != self.id {
			return Err(invalid().into());
		}
		let slot = self.reserve_root()?;
		let registry = self.registry.borrow();
		let mut arguments = Vec::new();
		for root in args {
			arguments
				.try_push(self.check(&registry, root)?)
				.map_err(Error::from)?;
		}
		let member = self
			.programs
			.iter_mut()
			.find(|m| m.program.code.id() == key.code)
			.ok_or_else(invalid)?;
		// A refused preparation retains its lease and caller-owned key.
		let _prepared =
			self.executor
				.prepare(&member.program, &mut self.store, function, &arguments)?;
		drop(_prepared);
		member.leased = false;
		let inner = self.executor.start_cross(
			&self.programs,
			key.code,
			&mut self.store,
			function,
			&arguments,
			fuel,
		)?;
		Ok(CrossInvocation {
			inner,
			registry: &self.registry,
			slot,
			programs: &self.programs,
		})
	}
}

#[cfg(test)]
mod drain_injection {
	use super::*;
	std::thread_local! { pub(super) static STAGE: core::cell::Cell<u8> = const { core::cell::Cell::new(0) }; }
	pub(super) fn release(store: &mut Store, root: Root) -> core::result::Result<(), ReleaseError> {
		let stage = STAGE.with(|s| s.replace(0));
		let error = Error::new(ErrorKind::Allocation, "injected domain drain");
		if stage == 1 {
			return Err(ReleaseError::Refused { root, error });
		}
		store.root_release(root)?;
		if stage == 2 {
			return Err(ReleaseError::Failed(error));
		}
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	fn context() -> Context {
		Context::with_default_modules().unwrap()
	}
	fn domain(c: &Context) -> ProgramDomain {
		ProgramDomain::new(
			Limits::default(),
			ExecutorLimits::default(),
			Arc::try_new(c.runtime().unwrap()).unwrap(),
		)
		.unwrap()
	}
	fn compile(d: &mut ProgramDomain, c: &Context, s: &str) -> ProgramKey {
		let mut sources = Sources::new();
		sources
			.insert(crate::Source::new("input", s).unwrap())
			.unwrap();
		d.compile_insert(c, &mut sources, &mut Diagnostics::new(), 0)
			.unwrap()
	}
	fn call(
		d: &mut ProgramDomain,
		key: ProgramKey,
		args: &[&DomainRoot],
		slice: usize,
	) -> DomainRoot {
		let id = d.function(&key, crate::Hash::type_hash(["main"])).unwrap();
		let mut call = d.start(&key, id, args, 10000).unwrap();
		loop {
			match call.resume(core::num::NonZeroUsize::new(slice).unwrap()) {
				CrossOutcome::Limited => {}
				CrossOutcome::Complete(root) => return root,
				CrossOutcome::Failed(error) => panic!("{}", error.message()),
			}
		}
	}
	#[test]
	fn returned_scalar_function_and_closure_keep_defining_program() {
		for source in ["fn f(){7} pub fn main(){f}", "pub fn main(){let n=7; ||n}"] {
			let c = context();
			let mut d = domain(&c);
			let key = compile(&mut d, &c, source);
			let value = call(&mut d, key, &[], 1);
			assert_eq!(d.collect_programs().unwrap().retained, 1);
			let key = compile(&mut d, &c, "pub fn main(f){f()}");
			let result = call(&mut d, key, &[&value], 1);
			assert_eq!(d.scalar(&result).unwrap(), Some(Scalar::I64(7)));
			drop(value);
			drop(result);
			assert_eq!(d.collect_programs().unwrap().retained, 0);
		}
	}
	#[test]
	fn dropped_handles_and_retained_aliases_drain_before_collect() {
		let c = context();
		let mut d = domain(&c);
		let key = compile(&mut d, &c, "pub fn main(){||9}");
		let value = call(&mut d, key, &[], 2);
		let alias = d.retain(&value).unwrap();
		drop(value);
		assert_eq!(d.collect_programs().unwrap().retained, 1);
		drop(alias);
		assert_eq!(d.collect_programs().unwrap().retained, 0);
	}
	#[test]
	fn context_epochs_fail_closed_but_old_snapshot_stays_compatible() {
		let mut c = context();
		let mut d = domain(&c);
		let key = compile(&mut d, &c, "pub fn main(){7}");
		c.install(crate::Module::new()).unwrap();
		let mut sources = Sources::new();
		sources
			.insert(crate::Source::new("input", "pub fn main(){8}").unwrap())
			.unwrap();
		assert!(d
			.compile_insert(&c, &mut sources, &mut Diagnostics::new(), 1)
			.is_err());
		let value = call(&mut d, key, &[], 1);
		assert_eq!(d.scalar(&value).unwrap(), Some(Scalar::I64(7)));
		assert!(ProgramDomain::new(
			Limits::default(),
			ExecutorLimits::default(),
			Arc::try_new(RuntimeContext::default()).unwrap()
		)
		.is_err());
	}
	#[test]
	fn close_releases_programs_despite_outstanding_handles() {
		let c = context();
		let mut d = domain(&c);
		let key = compile(&mut d, &c, "pub fn main(){||7}");
		let value = call(&mut d, key, &[], 1);
		d.close().unwrap();
		assert!(d.programs.is_empty());
		assert!(d.scalar(&value).is_err());
		drop(value);
	}
	#[test]
	fn leases_are_reported_and_released_explicitly() {
		let c = context();
		let mut d = domain(&c);
		let key = compile(&mut d, &c, "pub fn main(){7}");
		assert_eq!(d.collect_programs().unwrap().leases, 1);
		d.release_program(key).unwrap();
		assert_eq!(d.collect_programs().unwrap().retained, 0);
	}
	#[test]
	fn foreign_failure_borrows_its_defining_program() {
		let c = context();
		let mut d = domain(&c);
		let key = compile(&mut d, &c, "pub fn main(){||1/0}");
		let closure = call(&mut d, key, &[], 1);
		let key = compile(&mut d, &c, "pub fn main(f){f()}");
		let id = d.function(&key, crate::Hash::type_hash(["main"])).unwrap();
		let mut invocation = d.start(&key, id, &[&closure], 100).unwrap();
		loop {
			match invocation.resume(core::num::NonZeroUsize::new(1).unwrap()) {
				CrossOutcome::Limited => {}
				CrossOutcome::Failed(failure) => {
					assert_eq!(failure.failure.error.kind, ErrorKind::DivisionByZero);
					assert_eq!(failure.frames().len(), 2);
					assert!(failure.failure.call_chain.is_empty());
					assert_ne!(
						failure.frames()[0].program.code.id(),
						failure.frames()[1].program.code.id()
					);
					assert!(failure.frames().iter().all(|f| f.location.is_some()));
					assert!(failure.materialize().unwrap().contains("at input:"));
					assert!(!failure.truncated);
					break;
				}
				CrossOutcome::Complete(_) => panic!("missing failure"),
			}
		}
	}
	#[test]
	fn foreign_sort_and_iterator_callbacks_obey_slices() {
		for slice in [1, 2, 3, 10000] {
			for (callback, body, want) in [
				("|a,b|a.cmp(b)", "let v=[3,1,2];v.sort_by(f);v[0]", 1),
				("|n|n+1", "[1,2,3].iter().map(|x|f(x)).sum::<i64>()", 9),
			] {
				let c = context();
				let mut d = domain(&c);
				let key = compile(&mut d, &c, &format!("pub fn main(){{{callback}}}"));
				let f = call(&mut d, key, &[], slice);
				let key = compile(&mut d, &c, &format!("pub fn main(f){{{body}}}"));
				let value = call(&mut d, key, &[&f], slice);
				assert_eq!(d.scalar(&value).unwrap(), Some(Scalar::I64(want)));
			}
		}
	}
	#[test]
	fn three_programs_and_recursive_foreign_calls() {
		let c = context();
		let mut d = domain(&c);
		let a = compile(
			&mut d,
			&c,
			"fn rec(n){if n==0 {7}else{rec(n-1)}} pub fn main(){rec}",
		);
		let f = call(&mut d, a, &[], 1);
		let b = compile(&mut d, &c, "pub fn main(f){move ||f(4)}");
		let g = call(&mut d, b, &[&f], 2);
		let ckey = compile(&mut d, &c, "pub fn main(g){g()}");
		let result = call(&mut d, ckey, &[&g], 3);
		assert_eq!(d.scalar(&result).unwrap(), Some(Scalar::I64(7)));
	}
	#[test]
	fn collection_allocation_failure_releases_no_programs() {
		let c = context();
		let mut d = domain(&c);
		let k = compile(&mut d, &c, "pub fn main(){||7}");
		let root = call(&mut d, k, &[], 1);
		let before = d.programs.len();
		assert!(rune_alloc::limit::with(0, || d.collect_programs())
			.call()
			.is_err());
		assert_eq!(d.programs.len(), before);
		drop(root);
		assert_eq!(d.collect_programs().unwrap().retained, 0);
	}
	#[test]
	fn cancel_and_budget_leave_domain_reusable() {
		for cancel in [false, true] {
			let c = context();
			let mut d = domain(&c);
			let k = compile(&mut d, &c, "pub fn main(){||loop {}}");
			let root = call(&mut d, k, &[], 1);
			let k = compile(&mut d, &c, "pub fn main(f){f()}");
			let id = d.function(&k, crate::Hash::type_hash(["main"])).unwrap();
			{
				let mut inv = d.start(&k, id, &[&root], 10).unwrap();
				if cancel {
					assert!(matches!(
						inv.resume(core::num::NonZeroUsize::new(2).unwrap()),
						CrossOutcome::Limited
					));
					assert_eq!(inv.cancel().failure.error.kind, ErrorKind::Cancelled);
				} else {
					loop {
						match inv.resume(core::num::NonZeroUsize::new(1).unwrap()) {
							CrossOutcome::Limited => {}
							CrossOutcome::Failed(f) => {
								assert_eq!(f.failure.error.kind, ErrorKind::FuelExhausted);
								break;
							}
							_ => panic!("unexpected completion"),
						}
					}
				}
			}
			drop(root);
			d.collect_programs().unwrap();
			let k = compile(&mut d, &c, "pub fn main(){42}");
			let result = call(&mut d, k, &[], 1);
			assert_eq!(d.scalar(&result).unwrap(), Some(Scalar::I64(42)));
		}
	}
	#[test]
	fn domain_brand_and_recycled_generation_reject_old_handles() {
		let c = context();
		let mut a = domain(&c);
		let b = domain(&c);
		let root = a.i64(7).unwrap();
		assert!(b.scalar(&root).is_err());
		let stale = DomainRoot {
			registry: root.registry.clone(),
			index: root.index,
			generation: root.generation,
		};
		drop(root);
		let fresh = a.i64(8).unwrap();
		assert!(a.scalar(&stale).is_err());
		drop(stale);
		assert_eq!(a.scalar(&fresh).unwrap(), Some(Scalar::I64(8)));
	}
	#[test]
	fn other_context_and_failed_install_invalidate_admission() {
		let mut c = context();
		let mut d = domain(&c);
		let other = context();
		let mut sources = Sources::new();
		sources
			.insert(crate::Source::memory("pub fn main(){7}").unwrap())
			.unwrap();
		assert!(d
			.compile_insert(&other, &mut sources, &mut Diagnostics::new(), 0)
			.is_err());
		let mut module = crate::Module::new();
		module.function("collision", || 7i64).build().unwrap();
		c.install(&module).unwrap();
		let mut d = domain(&c);
		assert!(c.install(&module).is_err());
		assert!(d
			.compile_insert(&c, &mut sources, &mut Diagnostics::new(), 0)
			.is_err());
	}
	#[test]
	fn closure_calls_back_into_entry_program() {
		let c = context();
		let mut d = domain(&c);
		let k = compile(&mut d, &c, "pub fn main(){|g|g(6)}");
		let f = call(&mut d, k, &[], 1);
		let k = compile(&mut d, &c, "fn g(x){x+1} pub fn main(f){f(g)}");
		let result = call(&mut d, k, &[&f], 1);
		assert_eq!(d.scalar(&result).unwrap(), Some(Scalar::I64(7)));
	}
	#[test]
	fn missing_program_fails_before_foreign_effects() {
		let c = context();
		let mut d = domain(&c);
		let k = compile(&mut d, &c, "pub fn main(){||panic(\"must not run\")}");
		let f = call(&mut d, k, &[], 1);
		// Test-only metadata removal; public collection cannot remove this owner.
		d.programs.clear();
		let k = compile(&mut d, &c, "pub fn main(f){f()}");
		let id = d.function(&k, crate::Hash::type_hash(["main"])).unwrap();
		let mut invocation = d.start(&k, id, &[&f], 100).unwrap();
		loop {
			match invocation.resume(core::num::NonZeroUsize::new(1).unwrap()) {
				CrossOutcome::Limited => {}
				CrossOutcome::Failed(failure) => {
					assert_eq!(
						failure.failure.error.detail,
						super::super::detail::Detail::ForeignFunction
					);
					assert!(failure
						.materialize()
						.unwrap()
						.starts_with("function unavailable"));
					break;
				}
				_ => panic!("missing program executed"),
			}
		}
	}
	#[test]
	fn drain_failure_before_and_after_consumption_is_transactional() {
		for stage in [1, 2] {
			let c = context();
			let mut d = domain(&c);
			let k = compile(&mut d, &c, "pub fn main(){||7}");
			let f = call(&mut d, k, &[], 1);
			drop(f);
			drain_injection::STAGE.with(|s| s.set(stage));
			assert_eq!(
				d.collect_programs().unwrap_err().kind,
				ErrorKind::Allocation
			);
			assert_eq!(d.programs.len(), 1);
			let r = d.registry.borrow();
			assert_eq!(r.pending.is_some(), stage == 1);
			assert_eq!(
				r.slots.iter().filter(|s| s.root.is_some()).count(),
				usize::from(stage == 1)
			);
			drop(r);
			assert_eq!(d.collect_programs().unwrap().retained, 0);
			assert_eq!(d.store.live_entries(), 0);
		}
	}
	#[test]
	fn collection_traces_captures_nested_collections_and_cycles() {
		let c = context();
		let mut d = domain(&c);
		let k = compile(&mut d, &c, "pub fn main(){let a=[];let f=||a;a.push(f);a}");
		let root = call(&mut d, k, &[], 1);
		assert_eq!(d.collect_programs().unwrap().retained, 1);
		let k = compile(&mut d, &c, "pub fn main(a){a[0]()[0]}");
		let nested = call(&mut d, k, &[&root], 1);
		drop(root);
		assert_eq!(d.collect_programs().unwrap().retained, 1);
		drop(nested);
		assert_eq!(d.collect_programs().unwrap().retained, 0);
		d.close().unwrap();
		assert_eq!(d.store.live_entries(), 0);
	}
	#[test]
	fn owner_publication_and_leave_failures_keep_stacks_coherent() {
		for stage in 1..=4 {
			let c = context();
			let mut d = domain(&c);
			let k = compile(&mut d, &c, "pub fn main(){|x|x+1}");
			let f = call(&mut d, k, &[], 1);
			let k = compile(&mut d, &c, "pub fn main(f){f(6)}");
			let id = d.function(&k, crate::Hash::type_hash(["main"])).unwrap();
			super::super::execute::inject_cross_stage(stage);
			{
				let mut inv = d.start(&k, id, &[&f], 100).unwrap();
				loop {
					match inv.resume(core::num::NonZeroUsize::new(1).unwrap()) {
						CrossOutcome::Limited => {}
						CrossOutcome::Failed(f) => {
							assert_eq!(f.failure.error.kind, ErrorKind::Allocation);
							break;
						}
						_ => panic!("injection missed"),
					}
				}
			}
			assert!(d.executor.is_idle());
			let k = compile(&mut d, &c, "pub fn main(){42}");
			let value = call(&mut d, k, &[], 1);
			assert_eq!(d.scalar(&value).unwrap(), Some(Scalar::I64(42)));
		}
	}
	#[test]
	fn refused_start_keeps_a_releasable_lease() {
		let c = context();
		let mut d = domain(&c);
		let k = compile(&mut d, &c, "pub fn main(x){x}");
		let id = d.function(&k, crate::Hash::type_hash(["main"])).unwrap();
		assert!(d.start(&k, id, &[], 100).is_err());
		assert_eq!(d.collect_programs().unwrap().leases, 1);
		d.release_program(k).unwrap();
		assert_eq!(d.collect_programs().unwrap().retained, 0);
	}
	#[test]
	fn record_fields_retain_aliases_and_views_survive_handle_drop() {
		let c = context();
		let mut d = domain(&c);
		let k = compile(&mut d, &c, "pub fn main(){[7]}");
		let root = call(&mut d, k, &[], 1);
		let record = d.record_from_fields(&[("x", &root)]).unwrap();
		let child = d.field(&record, "x").unwrap();
		{
			let a = d
				.view(&root, super::super::heap::ViewLimits { nodes: 8, depth: 8 })
				.unwrap();
			let b = d
				.view(
					&child,
					super::super::heap::ViewLimits { nodes: 8, depth: 8 },
				)
				.unwrap();
			assert_eq!(a.root().unwrap().identity(), b.root().unwrap().identity());
			drop(root);
			drop(child);
			drop(record);
			assert!(matches!(
				a.root().unwrap().child(0).unwrap().kind().unwrap(),
				super::super::heap::ViewKind::Scalar(Scalar::I64(7))
			));
		}
		d.collect_programs().unwrap();
		assert_eq!(d.live_counts(), (0, 0, 0));
	}
	#[test]
	fn record_fields_reject_foreign_and_stale_roots() {
		let c = context();
		let mut a = domain(&c);
		let mut b = domain(&c);
		let root = a.i64(7).unwrap();
		assert!(b.record_from_fields(&[("x", &root)]).is_err());
		assert!(b
			.view(&root, super::super::heap::ViewLimits { nodes: 1, depth: 1 })
			.is_err());
		let stale = DomainRoot {
			registry: root.registry.clone(),
			index: root.index,
			generation: root.generation,
		};
		drop(root);
		a.collect_programs().unwrap();
		let replacement = a.i64(9).unwrap();
		assert!(a.record_from_fields(&[("x", &stale)]).is_err());
		assert!(a.field(&stale, "x").is_err());
		assert_eq!(a.scalar(&replacement).unwrap(), Some(Scalar::I64(9)));
	}
	#[test]
	fn record_fields_failed_reservations_reuse_empty_slot() {
		let c = context();
		let mut d = domain(&c);
		let mut foreign = domain(&c);
		let root = d.i64(7).unwrap();
		let other = foreign.i64(8).unwrap();
		let record = d.record_from_fields(&[("x", &root)]).unwrap();
		assert!(d.field(&record, "absent").is_err());
		let capacity = d.capacity_counts().unwrap();
		let live = d.live_counts();
		let empty = d
			.registry
			.borrow()
			.slots
			.iter()
			.position(|s| s.root.is_none() && !s.queued)
			.unwrap();
		for _ in 0..32 {
			assert!(d.record_from_fields(&[("x", &other)]).is_err());
			assert!(d.field(&record, "absent").is_err());
			assert!(
				rune_alloc::limit::with(0, || d.record_from_fields(&[("x", &root)]))
					.call()
					.is_err()
			);
			assert_eq!(d.capacity_counts().unwrap(), capacity);
			assert_eq!(d.live_counts(), live);
		}
		let child = d.field(&record, "x").unwrap();
		assert_eq!(child.index, empty);
		assert_eq!(d.capacity_counts().unwrap(), capacity);
		assert_eq!(d.scalar(&child).unwrap(), Some(Scalar::I64(7)));
	}
	#[test]
	fn record_fields_allocation_refusal_preserves_live_owners() {
		let c = context();
		let mut d = domain(&c);
		let root = d.i64(7).unwrap();
		let before = d.live_counts();
		assert!(
			rune_alloc::limit::with(0, || d.record_from_fields(&[("x", &root)]))
				.call()
				.is_err()
		);
		assert_eq!(d.live_counts(), before);
		assert_eq!(d.scalar(&root).unwrap(), Some(Scalar::I64(7)));
		let record = d.record_from_fields(&[("x", &root)]).unwrap();
		assert!(d.field(&record, "absent").is_err());
		let child = d.field(&record, "x").unwrap();
		assert_eq!(d.scalar(&child).unwrap(), Some(Scalar::I64(7)));
	}
}
