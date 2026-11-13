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
	host::ExecutionRuntime,
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
struct SessionSchema {
	entry: u32,
	bindings: Vec<(String, Option<super::heap::host_value::Identity>)>,
}
pub(crate) struct Member {
	session: Option<SessionSchema>,
	pub(crate) program: Program,
	leased: bool,
	may_escape: bool,
}

impl Member {
	pub(super) fn consume_entry_lease(&mut self) {
		self.leased = false;
	}
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
		runtime: ExecutionRuntime,
	) -> Result<Self> {
		let epoch = runtime.inner.execution_epoch.ok_or_else(invalid)?;
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
			session: None,
			may_escape: program.code.may_escape(),
			program,
			leased: true,
		})?;
		Ok(ProgramKey {
			domain: self.id,
			code,
		})
	}
	/// Compile a structurally checked synchronous session wrapper. Binding facts
	/// come from this domain's actual roots, never caller-supplied type hashes.
	/// The wrapper must restore exactly these fields and return (delta, result).
	pub fn compile_session(
		&mut self,
		context: &Context,
		sources: &mut Sources,
		diagnostics: &mut Diagnostics,
		revision: u64,
		bindings: &[(&str, &DomainRoot)],
		delta: &[&str],
	) -> Result<ProgramKey> {
		self.drain()?;
		if context.execution_epoch.snapshot()? != self.epoch {
			return Err(invalid());
		}
		let mut schema = Vec::try_with_capacity(bindings.len())?;
		let mut facts = Vec::try_with_capacity(bindings.len())?;
		let mut seen = crate::alloc::HashSet::new();
		{
			let registry = self.registry.borrow();
			for (name, root) in bindings {
				if !seen.try_insert(*name)? {
					return Err(invalid());
				}
				let identity = self.store.session_identity(self.check(&registry, root)?)?;
				if let Some(id) = identity {
					if !context.opaque.types.get(&id.hash).is_some_and(|r| {
						id.native
							.is_none_or(|native| r.descriptor.type_id == native)
					}) {
						return Err(invalid());
					}
				}
				facts.try_push((String::try_from(*name)?, identity.map(|id| id.hash)))?;
				schema.try_push((String::try_from(*name)?, identity))?;
			}
		}
		let mut names = Vec::try_with_capacity(delta.len())?;
		for name in delta {
			names.try_push(String::try_from(*name)?)?;
		}
		let envelope = frontend::opaque::Envelope {
			bindings: facts,
			delta: names,
		};
		let program =
			frontend::compile_inner(sources, context, diagnostics, revision, Some(envelope))?;
		if program.epoch != Some(self.epoch) {
			return Err(invalid());
		}
		let entry = program
			.function(crate::Hash::type_hash(["main"]))
			.ok_or_else(invalid)?;
		let code = program.code.id();
		self.programs.try_push(Member {
			may_escape: program.code.may_escape(),
			program,
			leased: true,
			session: Some(SessionSchema {
				entry,
				bindings: schema,
			}),
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
		let mut missing = crate::alloc::HashSet::new();
		for member in &self.programs {
			if !member.leased && member.may_escape {
				missing.try_insert(member.program.code.id())?;
			}
		}
		if !missing.is_empty() {
			let r = self.registry.borrow();
			self.store
				.find_programs(r.slots.iter().filter_map(|s| s.root.as_ref()), &mut missing)?;
		}
		let leases = self.programs.iter().filter(|p| p.leased).count();
		let before = self.programs.len();
		self.programs
			.retain(|p| p.leased || (p.may_escape && !missing.contains(&p.program.code.id())));
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
	/// Primary diagnostic only, without source frames. The domain borrow keeps
	/// the defining program alive while callers map structured frames themselves.
	#[inline]
	pub fn primary_message(&self) -> impl core::fmt::Display + '_ {
		struct Primary<'a, 'b>(&'a CrossFailure<'b>);
		impl core::fmt::Display for Primary<'_, '_> {
			#[inline]
			fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
				if self.0.failure.error.detail == super::detail::Detail::ForeignFunction {
					f.write_str("function unavailable")
				} else if let Some(program) = self.0.program {
					write!(f, "{}", self.0.failure.message(program))
				} else {
					f.write_str("function unavailable")
				}
			}
		}
		Primary(self)
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
		if let Some(schema) = &member.session {
			if function != schema.entry || arguments.len() != 1 {
				return Err(invalid().into());
			}
			self.store
				.validate_session_record(arguments[0], &schema.bindings)?;
		} else {
			for root in &arguments {
				if self.store.session_identity(root)?.is_some() {
					return Err(Error::refused(super::error::Refusal::OpaqueEntry).into());
				}
			}
		}
		let (inner, programs) = self.executor.start_cross(
			&mut self.programs,
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
			programs,
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
	use std::string::ToString;
	fn context() -> Context {
		Context::with_default_modules().unwrap()
	}
	fn domain(c: &Context) -> ProgramDomain {
		ProgramDomain::new(
			Limits::default(),
			ExecutorLimits::default(),
			c.execution_runtime().unwrap(),
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
	fn nonescaping_members_and_leases_do_not_walk() {
		let c = context();
		let mut d = domain(&c);
		let key = compile(&mut d, &c, "pub fn main(){7}");
		Store::collection_probe();
		assert_eq!(d.collect_programs().unwrap().leases, 1);
		assert_eq!(Store::collection_probe(), (0, 0));
		let root = call(&mut d, key, &[], 1);
		assert_eq!(d.collect_programs().unwrap().retained, 0);
		assert_eq!(Store::collection_probe(), (0, 0));
		assert_eq!(d.scalar(&root).unwrap(), Some(Scalar::I64(7)));
	}
	#[test]
	fn common_library_calls_with_held_data_do_not_walk() {
		let mut c = context();
		let mut module = crate::Module::with_crate("collection_host").unwrap();
		module.host_function("scalar", |x: i64| x).unwrap();
		c.install(module).unwrap();
		let mut d = domain(&c);
		d.executor.test_output = Some(std::vec::Vec::new());
		let key = compile(
			&mut d,
			&c,
			"pub fn main(){let v=[];for i in 0..256{v.push(i);}v}",
		);
		let v = call(&mut d, key, &[], 256);
		d.collect_programs().unwrap();
		for source in [
			"pub fn main(v){v.len()}",
			"pub fn main(v){v[0]+1}",
			"pub fn main(v){let n=0;for i in 0..8{n+=i;}n}",
			"pub fn main(v){println!(\"collection control\");7}",
			"pub fn main(v){collection_host::scalar(7)}",
		] {
			let key = compile(&mut d, &c, source);
			let output = call(&mut d, key, &[&v], 256);
			Store::collection_probe();
			assert_eq!(d.collect_programs().unwrap().retained, 0, "{source}");
			assert_eq!(Store::collection_probe(), (0, 0), "{source}");
			drop(output);
		}
		assert_eq!(
			d.executor.test_output.as_deref(),
			Some(&b"collection control\n"[..])
		);
	}

	#[test]
	fn iterator_callback_forwarding_preserves_the_original_program() {
		let c = context();
		let mut d = domain(&c);
		let key = compile(
			&mut d,
			&c,
			"fn identity(x){x} pub fn main(){(||7,identity)}",
		);
		let pair = call(&mut d, key, &[], 256);
		let f = d.get(&pair, 0).unwrap();
		let callback = d.get(&pair, 1).unwrap();
		let key = compile(
			&mut d,
			&c,
			"pub fn main(f,callback){[f].iter().map(|x|callback(x)).collect::<Vec>()[0]}",
		);
		let forwarded = call(&mut d, key, &[&f, &callback], 256);
		drop(pair);
		drop(f);
		drop(callback);
		assert_eq!(d.collect_programs().unwrap().retained, 1);
		let key = compile(&mut d, &c, "pub fn main(f){f()}");
		let output = call(&mut d, key, &[&forwarded], 256);
		assert_eq!(d.scalar(&output).unwrap(), Some(Scalar::I64(7)));
		drop(output);
		drop(forwarded);
		assert_eq!(d.collect_programs().unwrap().retained, 0);
	}
	#[test]
	fn typed_host_rejects_function_input_without_releasing_its_program() {
		use std::sync::{
			atomic::{AtomicUsize, Ordering},
			Arc,
		};
		let calls = Arc::new(AtomicUsize::new(0));
		let seen = calls.clone();
		let mut c = context();
		let mut m = crate::Module::with_crate("collection_host").unwrap();
		m.host_function("scalar", move |x: i64| {
			seen.fetch_add(1, Ordering::SeqCst);
			x
		})
		.unwrap();
		c.install(m).unwrap();
		let mut d = domain(&c);
		let key = compile(&mut d, &c, "pub fn main(){||7}");
		let f = call(&mut d, key, &[], 256);
		let key = compile(&mut d, &c, "pub fn main(f){collection_host::scalar(f)}");
		let id = d.function(&key, crate::Hash::type_hash(["main"])).unwrap();
		{
			let mut invocation = d.start(&key, id, &[&f], 10000).unwrap();
			match invocation.resume(core::num::NonZeroUsize::new(10000).unwrap()) {
				CrossOutcome::Failed(error) => {
					assert_eq!(error.failure.error.kind, ErrorKind::Host);
					assert!(
						matches!(
							error.failure.error.detail,
							super::super::detail::Detail::HostExpectedNumber { arg: Some(0), .. }
						),
						"{}",
						error.message()
					);
				}
				_ => panic!("expected function conversion error"),
			}
		}
		assert_eq!(calls.load(Ordering::SeqCst), 0);
		assert_eq!(d.collect_programs().unwrap().retained, 1);
		drop(f);
		assert_eq!(d.collect_programs().unwrap().retained, 0);
	}

	fn top_level_closure_root_order(closure_first: bool) {
		let c = context();
		let mut d = domain(&c);
		let (first, second) = if closure_first {
			(
				"pub fn main(){let x=7;||x}",
				"pub fn main(){let v=[];for i in 0..256{v.push(i);}v}",
			)
		} else {
			(
				"pub fn main(){let v=[];for i in 0..256{v.push(i);}v}",
				"pub fn main(){let x=7;||x}",
			)
		};
		let key = compile(&mut d, &c, first);
		let a = call(&mut d, key, &[], 1);
		let key = compile(&mut d, &c, second);
		let b = call(&mut d, key, &[], 1);
		d.collect_programs().unwrap(); // Remove any dead conservative producer.
		Store::collection_probe();
		assert_eq!(d.collect_programs().unwrap().retained, 1);
		assert_eq!(Store::collection_probe(), (1, 2));
		drop((a, b));
		assert_eq!(d.collect_programs().unwrap().retained, 0);
	}
	#[test]
	fn top_level_closure_is_found_before_data_in_both_root_orders() {
		top_level_closure_root_order(true);
		top_level_closure_root_order(false);
	}
	#[test]
	fn top_level_closure_before_data_is_found_directly() {
		top_level_closure_root_order(true);
	}
	#[test]
	fn top_level_closure_after_data_is_found_directly() {
		top_level_closure_root_order(false);
	}
	#[test]
	fn repeated_function_bits_stop_at_the_first_child() {
		let c = context();
		let mut d = domain(&c);
		let key = compile(
			&mut d,
			&c,
			"pub fn main(){let f=||7;let v=[];for i in 0..256{v.push(f);}v}",
		);
		let root = call(&mut d, key, &[], 1);
		Store::collection_probe();
		assert_eq!(d.collect_programs().unwrap().retained, 1);
		assert_eq!(Store::collection_probe(), (1, 2));
		drop(root);
		assert_eq!(d.collect_programs().unwrap().retained, 0);
	}
	#[test]
	fn dead_candidate_needs_one_full_walk_and_no_more() {
		let c = context();
		let mut d = domain(&c);
		let key = compile(
			&mut d,
			&c,
			"pub fn main(){let v=[];for i in 0..256{v.push(i);}v}",
		);
		let held = call(&mut d, key, &[], 1);
		d.collect_programs().unwrap();
		let key = compile(&mut d, &c, "pub fn main(){||7}");
		drop(call(&mut d, key, &[], 1));
		Store::collection_probe();
		assert_eq!(d.collect_programs().unwrap().retained, 0);
		assert_eq!(Store::collection_probe(), (1, 257));
		assert_eq!(d.collect_programs().unwrap().retained, 0);
		assert_eq!(Store::collection_probe(), (0, 0));
		let last = d.get(&held, 255).unwrap();
		assert_eq!(d.scalar(&last).unwrap(), Some(Scalar::I64(255)));
	}
	#[test]
	fn borrowed_and_forwarded_function_keeps_only_its_defining_program() {
		let c = context();
		let mut d = domain(&c);
		let key = compile(&mut d, &c, "pub fn main(){||7}");
		let f = call(&mut d, key, &[], 1);
		let key = compile(&mut d, &c, "pub fn main(f){f}");
		let forwarded = call(&mut d, key, &[&f], 1);
		assert_eq!(d.collect_programs().unwrap().retained, 1);
		drop(f);
		assert_eq!(d.collect_programs().unwrap().retained, 1);
		drop(forwarded);
		assert_eq!(d.collect_programs().unwrap().retained, 0);
	}
	#[test]
	fn a_candidate_found_at_the_root_does_not_hide_another_in_its_captures() {
		let c = context();
		let mut d = domain(&c);
		let key = compile(&mut d, &c, "pub fn main(){||7}");
		let f = call(&mut d, key, &[], 1);
		let key = compile(&mut d, &c, "pub fn main(f){move ||f()}");
		let g = call(&mut d, key, &[&f], 1);
		drop(f);
		Store::collection_probe();
		assert_eq!(d.collect_programs().unwrap().retained, 2);
		let (walks, values) = Store::collection_probe();
		assert_eq!(walks, 1);
		assert!(
			values > 1,
			"the second candidate is only in the capture graph"
		);
		drop(g);
		assert_eq!(d.collect_programs().unwrap().retained, 0);
	}
	#[test]
	fn allocation_failure_inside_the_walk_releases_no_program() {
		let c = context();
		let mut d = domain(&c);
		let key = compile(
			&mut d,
			&c,
			"pub fn main(){let v=[];for i in 0..32{v.push(i);}v}",
		);
		let _held = call(&mut d, key, &[], 1);
		d.collect_programs().unwrap();
		let key = compile(&mut d, &c, "pub fn main(){||7}");
		drop(call(&mut d, key, &[], 1));
		let before = d.programs.len();
		let mut walk_failed = false;
		let mut completed = false;
		for bytes in (0..2048).step_by(16) {
			Store::collection_probe();
			let result = rune_alloc::limit::with(bytes, || d.collect_programs()).call();
			let (walks, _) = Store::collection_probe();
			match result {
				Err(_) => {
					assert_eq!(d.programs.len(), before);
					walk_failed |= walks > 0;
				}
				Ok(report) => {
					assert_eq!(report.retained, 0);
					completed = true;
					break;
				}
			}
		}
		assert!(walk_failed && completed);
	}
	fn returned_function_keeps_defining_program(source: &str) {
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
	#[test]
	fn returned_scalar_function_and_closure_keep_defining_program() {
		returned_function_keeps_defining_program("fn f(){7} pub fn main(){f}");
		returned_function_keeps_defining_program("pub fn main(){let n=7; ||n}");
	}
	#[test]
	fn returned_function_literal_keeps_defining_program() {
		returned_function_keeps_defining_program("fn f(){7} pub fn main(){f}");
	}
	#[test]
	fn returned_capturing_closure_keeps_defining_program() {
		returned_function_keeps_defining_program("pub fn main(){let n=7; ||n}");
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
			ExecutionRuntime {
				inner: Arc::try_new(crate::runtime::RuntimeContext::default()).unwrap()
			}
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
					assert_eq!(
						failure.primary_message().to_string(),
						"Division by zero in `i64` division (`/`)"
					);
					assert!(failure
						.message()
						.to_string()
						.starts_with(&failure.primary_message().to_string()));
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
		let c = context();
		for slice in [1, 2, 3, 10000] {
			for (callback, body, want) in [
				("|a,b|a.cmp(b)", "let v=[3,1,2];v.sort_by(f);v[0]", 1),
				("|n|n+1", "[1,2,3].iter().map(|x|f(x)).sum::<i64>()", 9),
			] {
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
	fn cancel_and_budget_leave_domain_case(cancel: bool) {
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
	#[test]
	fn cancel_and_budget_leave_domain_reusable() {
		cancel_and_budget_leave_domain_case(false);
		cancel_and_budget_leave_domain_case(true);
	}
	#[test]
	fn budget_exhaustion_leaves_domain_reusable() {
		cancel_and_budget_leave_domain_case(false);
	}
	#[test]
	fn cancellation_leaves_domain_reusable() {
		cancel_and_budget_leave_domain_case(true);
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
	fn other_context_is_refused(c: &Context, other: &Context) {
		let mut d = domain(c);
		let mut sources = Sources::new();
		sources
			.insert(crate::Source::memory("pub fn main(){7}").unwrap())
			.unwrap();
		// Both contexts must independently admit the identical source. Empty
		// host tables match too; origin, not a module mismatch, must reject it.
		let key = d
			.compile_insert(c, &mut sources, &mut Diagnostics::new(), 0)
			.unwrap();
		d.release_program(key).unwrap();
		let mut other_domain = domain(other);
		let key = other_domain
			.compile_insert(other, &mut sources, &mut Diagnostics::new(), 0)
			.unwrap();
		other_domain.release_program(key).unwrap();
		assert!(d
			.compile_insert(other, &mut sources, &mut Diagnostics::new(), 0)
			.is_err());
	}
	fn failed_install_is_refused(mut c: Context) {
		let mut sources = Sources::new();
		sources
			.insert(crate::Source::memory("pub fn main(){7}").unwrap())
			.unwrap();
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
	fn other_context_and_failed_install_invalidate_admission() {
		let c = context();
		let other = context();
		other_context_is_refused(&c, &other);
		failed_install_is_refused(c);
	}
	#[test]
	fn other_context_with_matching_empty_hosts_is_refused() {
		// This origin check needs no standard-library function. The complete
		// default-module variants remain in the aggregate native control above.
		other_context_is_refused(&Context::new(), &Context::new());
	}
	#[test]
	fn failed_module_install_invalidates_admission() {
		failed_install_is_refused(Context::new());
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
	fn drain_failure_case(stage: u8) {
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
	#[test]
	fn drain_failure_before_and_after_consumption_is_transactional() {
		drain_failure_case(1);
		drain_failure_case(2);
	}
	#[test]
	fn drain_failure_before_consumption_is_transactional() {
		drain_failure_case(1);
	}
	#[test]
	fn drain_failure_after_consumption_is_transactional() {
		drain_failure_case(2);
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
	fn owner_publication_and_leave_failure(stage: u8) {
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
	#[test]
	fn owner_publication_and_leave_failures_keep_stacks_coherent() {
		for stage in 1..=4 {
			owner_publication_and_leave_failure(stage);
		}
	}
	#[test]
	fn owner_publication_and_leave_failure_stage_1() {
		owner_publication_and_leave_failure(1);
	}
	#[test]
	fn owner_publication_and_leave_failure_stage_2() {
		owner_publication_and_leave_failure(2);
	}
	#[test]
	fn owner_publication_and_leave_failure_stage_3() {
		owner_publication_and_leave_failure(3);
	}
	#[test]
	fn owner_publication_and_leave_failure_stage_4() {
		owner_publication_and_leave_failure(4);
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

#[cfg(test)]
mod host_session_tests {
	use super::super::error::Refusal;
	use super::*;
	use crate as rune;
	use crate::runtime::{Formatter, VmError};
	#[derive(crate::Any)]
	#[rune(item=::domain_host)]
	struct Native;
	fn display(_: &Native, _: &mut Formatter) -> core::result::Result<(), VmError> {
		Ok(())
	}
	fn context() -> Context {
		let mut c = Context::with_default_modules().unwrap();
		let mut m = crate::Module::with_crate("domain_host").unwrap();
		m.opaque_type::<Native>(|_| 1, display).unwrap();
		m.opaque_factory::<Native, _, _>("make", || Native).unwrap();
		c.install(m).unwrap();
		c
	}
	fn domain(c: &Context) -> ProgramDomain {
		ProgramDomain::new(
			Default::default(),
			Default::default(),
			c.execution_runtime().unwrap(),
		)
		.unwrap()
	}
	fn sources(text: &str) -> Sources {
		let mut s = Sources::new();
		s.insert(crate::Source::memory(text).unwrap()).unwrap();
		s
	}
	fn compile(
		d: &mut ProgramDomain,
		c: &Context,
		text: &str,
		bindings: &[(&str, &DomainRoot)],
	) -> Result<ProgramKey> {
		d.compile_session(
			c,
			&mut sources(text),
			&mut Diagnostics::new(),
			1,
			bindings,
			&[],
		)
	}
	fn call(d: &mut ProgramDomain, key: &ProgramKey, state: &DomainRoot) -> DomainRoot {
		let entry = d.entry(key, crate::Hash::type_hash(["main"])).unwrap();
		let mut invocation = d.start(key, entry, &[state], 10000).unwrap();
		loop {
			match invocation.resume(core::num::NonZeroUsize::new(256).unwrap()) {
				CrossOutcome::Limited => {}
				CrossOutcome::Complete(root) => return root,
				CrossOutcome::Failed(f) => panic!("{}", f.message()),
			}
		}
	}
	fn make(d: &mut ProgramDomain, c: &Context) -> DomainRoot {
		let key = compile(
			d,
			c,
			"pub fn main(s){let r=domain_host::make();(#{},r)}",
			&[],
		)
		.unwrap();
		let empty = d.record().unwrap();
		let output = call(d, &key, &empty);
		d.get(&output, 1).unwrap()
	}
	#[test]
	fn restored_host_schema_is_checked_before_prepare_and_cannot_claim_plain() {
		let c = context();
		let mut d = domain(&c);
		let host = make(&mut d, &c);
		let plain = d.i64(7).unwrap();
		let key = compile(
			&mut d,
			&c,
			"pub fn main(s){let x=s[\"x\"];let r=7;(#{},r)}",
			&[("x", &host)],
		)
		.unwrap();
		let forged = d.record_from_fields(&[("x", &plain)]).unwrap();
		let entry = d.entry(&key, crate::Hash::type_hash(["main"])).unwrap();
		assert!(
			matches!(d.start(&key,entry,&[&forged],1000),Err(StartError::Prepare(super::super::execute::PrepareError::Invalid(e))) if e.refusal==Some(Refusal::OpaqueEnvelope))
		);
		let valid = d.record_from_fields(&[("x", &host)]).unwrap();
		let output = call(&mut d, &key, &valid);
		let value = d.get(&output, 1).unwrap();
		assert_eq!(d.scalar(&value).unwrap(), Some(Scalar::I64(7)));
		let nested = d.record_from_fields(&[("nested", &host)]).unwrap();
		let error = compile(
			&mut d,
			&c,
			"pub fn main(s){let x=s[\"x\"];let r=x;(#{},r)}",
			&[("x", &nested)],
		)
		.unwrap_err();
		assert_eq!(error.refusal, Some(Refusal::OpaqueContainer));
		d.close().unwrap();
	}
	#[test]
	fn restored_record_cannot_escape_through_user_code_or_wrapper_function_values() {
		let c = context();
		let mut d = domain(&c);
		for body in [
			"let r=s;(#{},r)",
			"let r=||s;(#{},r)",
			"let r=main;(#{},r)",
			"let r=main(s);(#{},r)",
		] {
			let source = std::format!("pub fn main(s){{{body}}}");
			let error = compile(&mut d, &c, &source, &[]).unwrap_err();
			assert!(
				matches!(
					error.refusal,
					Some(Refusal::OpaqueEnvelope | Refusal::OpaqueIngress)
				),
				"{source}: {error:?}"
			);
		}
		d.close().unwrap();
	}
	#[test]
	fn a_host_value_cannot_enter_a_non_session_program() {
		let c = context();
		let mut d = domain(&c);
		let host = make(&mut d, &c);
		let key = d
			.compile_insert(
				&c,
				&mut sources("pub fn main(x){x}"),
				&mut Diagnostics::new(),
				2,
			)
			.unwrap();
		let entry = d.entry(&key, crate::Hash::type_hash(["main"])).unwrap();
		assert!(
			matches!(d.start(&key,entry,&[&host],1000),Err(StartError::Prepare(super::super::execute::PrepareError::Invalid(e))) if e.refusal==Some(Refusal::OpaqueEntry))
		);
		d.close().unwrap();
	}
}

#[cfg(test)]
mod admission_tests;
