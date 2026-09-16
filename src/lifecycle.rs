//! Per-context, thread-local ownership for trusted extension operations.
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};
use std::thread::ThreadId;

const CANCELLED: &str = "operation cancelled";
const RETIRED: &str = "operation scope belongs to a retired context";
static NEXT: AtomicU64 = AtomicU64::new(1);
thread_local! {
	static CONTEXTS: RefCell<BTreeMap<u64, Weak<State>>> = const { RefCell::new(BTreeMap::new()) };
}

/// A capture-safe handle to operations owned by one serving context.
///
/// The handle may be captured by Rune's Send + Sync function closures. Its
/// futures remain on the creating thread and need not be Send or Sync.
#[derive(Clone)]
pub struct Scope {
	id: u64,
	thread: ThreadId,
	name: &'static str,
}
impl Scope {
	/// Track a lazy operation. Failed executions revoke operations they polled;
	/// reset and teardown revoke all. Repolling a revoked value returns
	/// `Err("operation cancelled")`, without polling the native future again.
	/// Wrong-thread or retired-context use has a distinct error.
	pub fn track<F, T>(&self, future: F) -> impl Future<Output = Result<T, String>> + use<F, T>
	where
		F: Future<Output = Result<T, String>> + 'static,
		T: 'static,
	{
		let state = if self.thread != std::thread::current().id() {
			Err("operation scope used on the wrong thread")
		} else {
			CONTEXTS.with(|contexts| {
				contexts
					.borrow()
					.get(&self.id)
					.and_then(Weak::upgrade)
					.filter(|s| !s.retired.get())
					.ok_or(RETIRED)
			})
		};
		match state {
			Ok(state) => {
				let owner = Rc::new(Owner {
					future: RefCell::new(Some(
						Box::pin(future) as Pin<Box<dyn Future<Output = Result<T, String>>>>
					)),
					state: Rc::downgrade(&state),
					name: self.name,
					stamp: Cell::new(0),
					key: state.next.get(),
				});
				// Never reuse an operation key, including after reset.
				if let Some(next) = state.next.get().checked_add(1) {
					state.next.set(next);
					let erased: Rc<dyn Revoke> = owner.clone();
					state
						.owners
						.borrow_mut()
						.insert(owner.key, Rc::downgrade(&erased));
					Tracked {
						owner: Some(owner),
						error: None,
					}
				} else {
					drop(owner);
					Tracked {
						owner: None,
						error: Some("operation identity exhausted".into()),
					}
				}
			}
			Err(error) => {
				let drop = crate::extensions::build_catching(|| {
					drop(future);
					Ok(())
				});
				Tracked {
					owner: None,
					error: Some(match drop {
						Ok(()) => error.into(),
						Err(reason) => format!(
							"{error}; extension `{}` lifecycle cleanup {reason}",
							self.name
						),
					}),
				}
			}
		}
	}
}
trait Revoke {
	fn stamp(&self) -> u64;
	fn revoke(&self);
}
struct State {
	id: u64,
	generation: Cell<u64>,
	active: Cell<bool>,
	next: Cell<u64>,
	retired: Cell<bool>,
	owners: RefCell<BTreeMap<u64, Weak<dyn Revoke>>>,
	failure: RefCell<Option<String>>,
}
impl State {
	fn revoke(&self, all: bool) {
		// Upgrade under the registry borrow, then release it before adapter Drop.
		let owners: Vec<_> = self
			.owners
			.borrow()
			.values()
			.filter_map(Weak::upgrade)
			.collect();
		for owner in owners {
			if all || owner.stamp() == self.generation.get() {
				owner.revoke();
			}
		}
	}
	fn retire(&self) {
		self.retired.set(true);
		CONTEXTS.with(|contexts| {
			contexts.borrow_mut().remove(&self.id);
		});
		self.revoke(true);
	}
	fn result(&self) -> Result<(), String> {
		match self.failure.borrow().clone() {
			Some(error) => Err(error),
			None => Ok(()),
		}
	}
}
type Inner<T> = Pin<Box<dyn Future<Output = Result<T, String>>>>;
struct Owner<T> {
	future: RefCell<Option<Inner<T>>>,
	state: Weak<State>,
	name: &'static str,
	stamp: Cell<u64>,
	key: u64,
}
impl<T> Owner<T> {
	fn dispose(&self, future: Option<Inner<T>>) {
		if let Some(state) = self.state.upgrade() {
			state.owners.borrow_mut().remove(&self.key);
		}
		if future.is_none() {
			return;
		}
		if let Err(error) = crate::extensions::build_catching(|| {
			drop(future);
			Ok(())
		}) && let Some(state) = self.state.upgrade()
		{
			state.failure.borrow_mut().get_or_insert_with(|| {
				format!("extension `{}` lifecycle cleanup {error}", self.name)
			});
		}
	}
}
impl<T> Revoke for Owner<T> {
	fn stamp(&self) -> u64 {
		self.stamp.get()
	}
	fn revoke(&self) {
		let future = self.future.borrow_mut().take();
		self.dispose(future);
	}
}
impl<T> Drop for Owner<T> {
	fn drop(&mut self) {
		let future = self.future.get_mut().take();
		self.dispose(future);
	}
}
struct Tracked<T> {
	owner: Option<Rc<Owner<T>>>,
	error: Option<String>,
}
impl<T> Future for Tracked<T> {
	type Output = Result<T, String>;
	fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
		let this = self.get_mut();
		if let Some(error) = &this.error {
			return Poll::Ready(Err(error.clone()));
		}
		let owner = this.owner.as_ref().expect("tracked owner or error");
		let Some(state) = owner.state.upgrade().filter(|s| !s.retired.get()) else {
			owner.revoke();
			return Poll::Ready(Err(CANCELLED.into()));
		};
		owner.stamp.set(state.generation.get());
		let Some(mut future) = owner.future.borrow_mut().take() else {
			return Poll::Ready(Err(CANCELLED.into()));
		};
		// No registry/owner borrow across a poll or destructor.
		match future.as_mut().poll(cx) {
			Poll::Pending => {
				*owner.future.borrow_mut() = Some(future);
				Poll::Pending
			}
			Poll::Ready(result) => {
				owner.dispose(Some(future));
				Poll::Ready(result)
			}
		}
	}
}

/// Disabled contexts allocate nothing. Only lifecycle-aware extensions opt in.
#[derive(Clone, Default)]
pub(crate) struct Lifecycle(Option<Rc<State>>);
impl Lifecycle {
	pub fn new(enabled: bool) -> Result<Self, String> {
		if !enabled {
			return Ok(Self::default());
		}
		let id = NEXT
			.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
			.map_err(|_| "operation context identity exhausted")?;
		let state = Rc::new(State {
			id,
			generation: Cell::new(0),
			active: Cell::new(false),
			next: Cell::new(1),
			retired: Cell::new(false),
			owners: RefCell::new(BTreeMap::new()),
			failure: RefCell::new(None),
		});
		CONTEXTS.with(|contexts| {
			contexts.borrow_mut().insert(id, Rc::downgrade(&state));
		});
		Ok(Self(Some(state)))
	}
	pub fn scope(&self, name: &'static str) -> Scope {
		Scope {
			id: self.0.as_ref().expect("lifecycle enabled").id,
			thread: std::thread::current().id(),
			name,
		}
	}
	pub fn begin(&self) -> Result<(), String> {
		if let Some(state) = &self.0 {
			state.result()?;
			if state.retired.get() {
				return Err(RETIRED.into());
			}
			let generation = state
				.generation
				.get()
				.checked_add(1)
				.ok_or("execution generation exhausted")?;
			state.generation.set(generation);
			state.active.set(true);
		}
		Ok(())
	}
	pub fn finish(&self, failed: bool) -> Result<(), String> {
		if let Some(state) = &self.0 {
			if state.active.replace(false) && failed {
				state.revoke(false);
			}
			if state.result().is_err() {
				state.retire();
			}
			state.result()?;
		}
		Ok(())
	}
	pub fn clear(&self) -> Result<(), String> {
		if let Some(state) = &self.0 {
			state.revoke(true);
			if state.result().is_err() {
				state.retire();
			}
			state.result()?;
		}
		Ok(())
	}
	pub fn close(&self) -> Result<(), String> {
		if let Some(state) = &self.0 {
			state.retire();
			state.result()?;
		}
		Ok(())
	}
	pub fn failed(&self) -> bool {
		self.0
			.as_ref()
			.is_some_and(|s| s.failure.borrow().is_some())
	}
}
impl Drop for Lifecycle {
	fn drop(&mut self) {
		if self
			.0
			.as_ref()
			.is_some_and(|s| Rc::strong_count(s) == 1 && !s.retired.get())
			&& let Err(error) = self.close()
		{
			eprintln!("error: {}", crate::format::terminal_safe(&error));
		}
	}
}
/// Explicit process exits bypass guards. There is no subsequent runtime turn.
pub(crate) fn close_thread() -> Result<(), String> {
	let states: Vec<_> = CONTEXTS.with(|contexts| {
		contexts
			.borrow()
			.values()
			.filter_map(Weak::upgrade)
			.collect()
	});
	let mut error = None;
	for state in states {
		state.retire();
		if let Err(e) = state.result() {
			error.get_or_insert(e);
		}
	}
	error.map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::task::Waker;
	struct Pending {
		live: Rc<Cell<usize>>,
		polled: Rc<Cell<usize>>,
		panic: bool,
	}
	impl Pending {
		fn new(live: &Rc<Cell<usize>>, polled: &Rc<Cell<usize>>, panic: bool) -> Self {
			live.set(live.get() + 1);
			Self {
				live: live.clone(),
				polled: polled.clone(),
				panic,
			}
		}
	}
	impl Future for Pending {
		type Output = Result<i64, String>;
		fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
			self.polled.set(self.polled.get() + 1);
			Poll::Pending
		}
	}
	impl Drop for Pending {
		fn drop(&mut self) {
			self.live.set(self.live.get() - 1);
			assert!(!self.panic, "injected destructor panic");
		}
	}
	fn poll<F: Future + Unpin>(future: &mut F) -> Poll<F::Output> {
		Pin::new(future).poll(&mut Context::from_waker(Waker::noop()))
	}
	#[test]
	fn retained_owner_is_revoked_without_another_poll() {
		let life = Lifecycle::new(true).unwrap();
		let scope = life.scope("fixture");
		let live = Rc::new(Cell::new(0));
		let polled = Rc::new(Cell::new(0));
		let mut first = scope.track(Pending::new(&live, &polled, false));
		let mut other = scope.track(Pending::new(&live, &polled, false));
		assert_eq!(polled.get(), 0);
		life.begin().unwrap();
		assert!(poll(&mut first).is_pending());
		assert!(poll(&mut other).is_pending());
		life.finish(false).unwrap();
		assert_eq!(live.get(), 2);
		life.begin().unwrap();
		assert!(poll(&mut first).is_pending());
		life.finish(true).unwrap();
		assert_eq!(live.get(), 1);
		assert_eq!(poll(&mut first), Poll::Ready(Err(CANCELLED.into())));
		assert_eq!(polled.get(), 3);
		life.begin().unwrap();
		assert!(poll(&mut other).is_pending());
		life.finish(false).unwrap();
		life.clear().unwrap();
		assert_eq!(live.get(), 0);
		assert_eq!(poll(&mut other), Poll::Ready(Err(CANCELLED.into())));
		assert!(life.0.as_ref().unwrap().owners.borrow().is_empty());
	}
	#[test]
	fn final_drop_completion_and_returned_error_remove_entries() {
		let life = Lifecycle::new(true).unwrap();
		let scope = life.scope("fixture");
		let live = Rc::new(Cell::new(0));
		let polled = Rc::new(Cell::new(0));
		drop(scope.track(Pending::new(&live, &polled, false)));
		assert_eq!(live.get(), 0);
		assert_eq!(polled.get(), 0);
		for result in [Ok(42), Err("ordinary error".into())] {
			let mut future = scope.track(std::future::ready(result.clone()));
			assert_eq!(poll(&mut future), Poll::Ready(result));
			assert!(life.0.as_ref().unwrap().owners.borrow().is_empty());
		}
	}
	#[test]
	fn contexts_threads_and_retired_handles_never_fall_back() {
		let first = Lifecycle::new(true).unwrap();
		let second = Lifecycle::new(true).unwrap();
		let scope = first.scope("fixture");
		assert_ne!(scope.id, second.scope("fixture").id);
		let wrong = scope.clone();
		let error = std::thread::spawn(move || poll(&mut wrong.track(std::future::ready(Ok(1)))))
			.join()
			.unwrap();
		assert_eq!(
			error,
			Poll::Ready(Err("operation scope used on the wrong thread".into()))
		);
		first.close().unwrap();
		assert_eq!(
			poll(&mut scope.track(std::future::ready(Ok(1)))),
			Poll::Ready(Err(RETIRED.into()))
		);
		assert_eq!(
			poll(&mut second.scope("fixture").track(std::future::ready(Ok(2)))),
			Poll::Ready(Ok(2))
		);
	}
	#[test]
	fn destructor_failure_retires_and_still_revokes_other_owners() {
		let life = Lifecycle::new(true).unwrap();
		let scope = life.scope("fixture");
		let live = Rc::new(Cell::new(0));
		let polled = Rc::new(Cell::new(0));
		let mut bad = scope.track(Pending::new(&live, &polled, true));
		let mut good = scope.track(Pending::new(&live, &polled, false));
		life.begin().unwrap();
		assert!(poll(&mut bad).is_pending());
		assert!(poll(&mut good).is_pending());
		let error = life.finish(true).unwrap_err();
		assert!(
			error.contains(
				"extension `fixture` lifecycle cleanup panicked: injected destructor panic"
			)
		);
		assert_eq!(live.get(), 0);
		assert_eq!(poll(&mut good), Poll::Ready(Err(CANCELLED.into())));
		assert!(life.begin().is_err());
	}
	#[test]
	fn rune_registration_accepts_scope_and_non_send_future() {
		let life = Lifecycle::new(true).unwrap();
		let scope = life.scope("fixture");
		let mut module = rune::Module::with_crate("fixture").unwrap();
		module
			.function("answer", move || {
				scope.track(async {
					let value = Rc::new(42i64);
					std::future::ready(()).await;
					Ok(*value)
				})
			})
			.build()
			.unwrap();
	}
	struct Resource {
		live: std::sync::Arc<std::sync::atomic::AtomicUsize>,
		started: bool,
		_not_send: Rc<()>,
	}
	impl Future for Resource {
		type Output = Result<i64, String>;
		fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
			if !self.started {
				self.live.fetch_add(1, Ordering::Relaxed);
				self.started = true;
			}
			Poll::Pending
		}
	}
	impl Drop for Resource {
		fn drop(&mut self) {
			if self.started {
				self.live.fetch_sub(1, Ordering::Relaxed);
			}
		}
	}
	fn session() -> (
		crate::session::Session,
		std::sync::Arc<std::sync::atomic::AtomicUsize>,
	) {
		let live = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
		let counter = live.clone();
		let extensions =
			crate::Extensions::none().with_lifecycle("fixture", move |module, scope| {
				module
					.function("pending", move || {
						scope.track(Resource {
							live: counter.clone(),
							started: false,
							_not_send: Rc::new(()),
						})
					})
					.build()
					.map_err(|e| e.to_string())?;
				Ok(vec![])
			});
		let life = extensions.lifecycle().unwrap();
		let mut context = rune::Context::with_default_modules().unwrap();
		crate::time::install(&mut context).unwrap();
		extensions.install_with(&mut context, &life).unwrap();
		(
			crate::session::Session::new(context)
				.unwrap()
				.with_lifecycle(life),
			live,
		)
	}
	const SELECT: &str = "let timer = time::sleep(1); select { _ = q => (), _ = timer => () };";
	#[test]
	fn rune_retained_runtime_failure_and_budget_revoke_before_return() {
		for ending in ["panic!(\"failed input\")", "loop {}"] {
			let (mut session, live) = session();
			session.set_budget(10000);
			session
				.eval("let q = fixture::pending(); let kept = 42;")
				.unwrap();
			assert_eq!(live.load(Ordering::Relaxed), 0);
			let error = session.eval(&format!("{SELECT} {ending}")).unwrap_err();
			assert!(matches!(
				error,
				crate::session::Failure::Runtime { .. } | crate::session::Failure::Budget(_)
			));
			assert_eq!(live.load(Ordering::Relaxed), 0);
			let value = session.eval("q.await").unwrap();
			assert_eq!(
				crate::format::render(&value, None, &Default::default()),
				"Err(\"operation cancelled\")"
			);
			assert_eq!(
				crate::format::render(&session.eval("kept").unwrap(), None, &Default::default()),
				"42"
			);
		}
	}
	#[test]
	fn rune_success_unrelated_failure_compile_refusal_and_reset() {
		let (mut session, live) = session();
		session.eval("let q = fixture::pending();").unwrap();
		session.eval(SELECT).unwrap();
		assert_eq!(live.load(Ordering::Relaxed), 1);
		session.renumber();
		assert!(session.eval("let =").is_err());
		assert!(session.eval("panic!(\"unrelated\")").is_err());
		assert_eq!(live.load(Ordering::Relaxed), 1);
		session
			.eval(&format!("{SELECT} Err(\"caught value\")"))
			.unwrap();
		assert_eq!(live.load(Ordering::Relaxed), 1);
		session.reset_fallible().unwrap();
		assert_eq!(live.load(Ordering::Relaxed), 0);
		session.eval("let q = fixture::pending();").unwrap();
		session.eval(SELECT).unwrap();
		assert_eq!(live.load(Ordering::Relaxed), 1);
		session.close().unwrap();
		assert_eq!(live.load(Ordering::Relaxed), 0);
	}
	#[test]
	fn repeated_operations_do_not_accumulate_weak_entries() {
		let lifecycle = Lifecycle::new(true).unwrap();
		let scope = lifecycle.scope("fixture");
		lifecycle.begin().unwrap();
		for n in 0..5000 {
			let mut future = scope.track(std::future::ready(Ok(n)));
			assert_eq!(poll(&mut future), Poll::Ready(Ok(n)));
			assert!(lifecycle.0.as_ref().unwrap().owners.borrow().is_empty());
		}
		lifecycle.finish(false).unwrap();
	}
}
