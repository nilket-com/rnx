//! Caller-owned handler execution: fresh invocations or reusable, thread-local slots.
//!
//! Enable `server-runtime`. This API installs no signals, runs no event loop,
//! and never terminates the host. Trusted native extensions are not sandboxed.
//! Construct fresh extensions with identical registrations for compilation and
//! each invocation, or once per reusable slot. Values and invocations stay on their creating thread.
//! The context constructor and ownership fields are deliberately private:
//! ```compile_fail
//! use rnx::server::context;
//! ```
//! ```compile_fail
//! fn owner(invocation: rnx::server::Invocation) { let _ = invocation.owner; }
//! ```
//! ```compile_fail
//! fn unit(program: rnx::server::Program) { let _ = program.0; }
//! ```
//! ```compile_fail
//! fn category(failure: rnx::server::Failure) { let _ = failure.category; }
//! ```
use crate::{Extensions, http, lifecycle::Lifecycle, program::Loader};
use rune::{
	Context, Diagnostics, Sources, Vm,
	runtime::{RuntimeContext, Value, VmError},
};
use std::{
	path::{Path, PathBuf},
	sync::Arc,
};

/// An owned diagnostic. Categories are `preparation`, `vm`, `cancelled`, and
/// `cleanup`. A cleanup failure is state loss, not an ordinary handler error.
#[derive(Clone, Debug)]
pub struct Failure {
	category: &'static str,
	message: String,
	location: Option<(PathBuf, usize, usize, String)>,
}
impl Failure {
	fn new(category: &'static str, message: impl ToString) -> Self {
		Self {
			category,
			message: message.to_string(),
			location: None,
		}
	}
	/// Stable failure category; cleanup takes precedence over execution failure.
	pub fn category(&self) -> &'static str {
		self.category
	}
	/// Diagnostic text, without terminal styling or escaping.
	pub fn message(&self) -> &str {
		&self.message
	}
	/// Source file, if attribution was possible.
	pub fn path(&self) -> Option<&Path> {
		self.location.as_ref().map(|p| p.0.as_path())
	}
	/// One-based source line and character column.
	pub fn position(&self) -> Option<(usize, usize)> {
		self.location.as_ref().map(|p| (p.1, p.2))
	}
	/// The original source line. Escape it before displaying it in a terminal.
	pub fn excerpt(&self) -> Option<&str> {
		self.location.as_ref().map(|p| p.3.as_str())
	}
	fn locate(mut self, source: &crate::program::Text, offset: usize) -> Self {
		let (line, column, excerpt) = crate::session::position(&source.text, offset);
		self.location = Some((source.path.clone(), line, column, excerpt));
		self
	}
}
impl std::fmt::Display for Failure {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str(&self.message)
	}
}
impl std::error::Error for Failure {}

struct Owner {
	life: Lifecycle,
	http: http::State,
}
impl Owner {
	fn close(&self) -> Result<(), Failure> {
		let result = self.life.close().map_err(|e| Failure::new("cleanup", e));
		self.http.cancel();
		result
	}
}
impl Drop for Owner {
	fn drop(&mut self) {
		let _ = self.close();
	}
}
fn context(extensions: Extensions) -> Result<(Context, Owner), Failure> {
	let owner = Owner {
		life: Lifecycle::new(true).map_err(|e| Failure::new("preparation", e))?,
		http: http::State::default(),
	};
	let result = (|| -> crate::Result<Context> {
		let mut context = Context::with_default_modules()?;
		// Unlike install_core, this path has no CLI interrupt initialization.
		crate::json::install(&mut context)?;
		crate::io::install(&mut context)?;
		crate::interchange::install(&mut context)?;
		crate::process::install_with_exit(&mut context, |_| {
			Err("cannot exit: this is a server/embedding context".into())
		})?;
		crate::fs::install(&mut context)?;
		crate::path::install(&mut context)?;
		crate::time::install(&mut context)?;
		crate::text::install(&mut context)?;
		crate::web::install(&mut context)?;
		crate::env::install(&mut context, Arc::from([]))?;
		crate::http::install(&mut context, &owner.http, owner.life.scope("http"))?;
		#[cfg(feature = "test-support")]
		crate::rnx_test::install(&mut context)?;
		extensions.install_with(&mut context, &owner.life)?;
		Ok(context)
	})();
	match result {
		Ok(context) => Ok((context, owner)),
		Err(error) => {
			owner.close()?;
			Err(Failure::new("preparation", error))
		}
	}
}
struct Compiled {
	unit: Arc<rune::Unit>,
	sources: Sources,
	loader: Loader,
}
/// A shareable compiled program and its retained source information.
#[derive(Clone)]
pub struct Program(Arc<Compiled>);
impl Program {
	/// Compile with the entry-file loader and its 8 MiB aggregate allowance.
	/// Extensions are schema registrations; no handler is executed here.
	pub fn compile(entry: impl AsRef<Path>, extensions: Extensions) -> Result<Self, Failure> {
		Self::build(extensions, |loader| loader.entry(entry.as_ref()))
	}
	/// Compile source text held by the caller. `name` identifies it in
	/// diagnostics and is never opened. `mod` declarations are refused, since
	/// the text has no directory to resolve them against.
	pub fn compile_source(
		name: &str,
		source: &str,
		extensions: Extensions,
	) -> Result<Self, Failure> {
		Self::build(extensions, |loader| loader.memory(name, source))
	}
	fn build(
		extensions: Extensions,
		entry: impl FnOnce(&mut Loader) -> Result<rune::Source, String>,
	) -> Result<Self, Failure> {
		let (context, owner) = context(extensions)?;
		let built = (|| {
			let mut loader = Loader::new();
			let mut sources = Sources::new();
			sources
				.insert(entry(&mut loader).map_err(|e| Failure::new("preparation", e))?)
				.map_err(|e| Failure::new("preparation", e))?;
			let mut diagnostics = Diagnostics::new();
			let result = rune::prepare(&mut sources)
				.with_context(&context)
				.with_diagnostics(&mut diagnostics)
				.with_source_loader(&mut loader)
				.build();
			let unit = result.map_err(|e| {
				for diagnostic in diagnostics.diagnostics() {
					if let rune::diagnostics::Diagnostic::Fatal(fatal) = diagnostic
						&& let rune::diagnostics::FatalDiagnosticKind::CompileError(error) =
							fatal.kind()
					{
						use rune::ast::Spanned;
						let failure = Failure::new("preparation", error);
						return match loader.get(&sources, fatal.source_id()) {
							Some(text) => failure.locate(text, error.span().range().start),
							None => failure,
						};
					}
				}
				Failure::new("preparation", e)
			})?;
			Ok(Self(Arc::new(Compiled {
				unit: Arc::new(unit),
				sources,
				loader,
			})))
		})();
		drop(context);
		owner.close()?;
		built
	}
	/// Compiled Rune function metadata for the stock HTTP host; never executes the item.
	#[cfg(feature = "http-server")]
	pub(crate) fn http_function(&self, name: &str) -> Result<Option<usize>, Failure> {
		let hash = rune::Hash::type_hash(name.split("::").collect::<Vec<_>>().as_slice());
		let Some(debug) = self.0.unit.debug_info() else {
			return Err(self.http_route_error("compiled function metadata is unavailable"));
		};
		let Some(signature) = debug.functions.get(&hash) else {
			return Ok(None);
		};
		let rune::runtime::debug::DebugArgs::Named(args) = &signature.args else {
			return Err(self.http_route_error(format!("{name} is not a compiled Rune function")));
		};
		if !debug.functions_rev.values().any(|h| *h == hash) {
			return Err(self.http_route_error(format!("{name} has no compiled function entry")));
		}
		Vm::without_runtime(self.0.unit.clone())
			.lookup_function(hash)
			.map_err(|e| self.http_route_error(format!("cannot resolve {name}: {e}")))?;
		Ok(Some(args.len()))
	}
	/// Dynamic route rows are attributed to the table function, not guessed source literals.
	#[cfg(feature = "http-server")]
	pub(crate) fn http_route_error(&self, message: impl ToString) -> Failure {
		let failure = Failure::new("preparation", message);
		let hash = rune::Hash::type_hash(["routes"]);
		let Some(debug) = self.0.unit.debug_info() else {
			return failure;
		};
		let Some((&ip, _)) = debug.functions_rev.iter().find(|(_, h)| **h == hash) else {
			return failure;
		};
		// Rune emits an unannotated Allocate at the function entry. Use the first
		// annotated instruction within this function, never a later function's source.
		let end = debug
			.functions_rev
			.keys()
			.copied()
			.filter(|p| *p > ip)
			.min()
			.unwrap_or(usize::MAX);
		let Some((_, instruction)) = debug
			.instructions
			.iter()
			.filter(|(p, _)| **p >= ip && **p < end)
			.min_by_key(|(p, _)| **p)
		else {
			return failure;
		};
		let Some(text) = self.0.loader.get(&self.0.sources, instruction.source_id) else {
			return failure;
		};
		failure.locate(text, instruction.span.range().start)
	}

	/// Construct one local invocation. `handler` is a `::`-separated item path;
	/// the function takes one argument. Budgets exclude zero and usize::MAX.
	pub fn prepare(
		&self,
		extensions: Extensions,
		handler: &str,
		argument: Value,
		budget: usize,
	) -> Result<Invocation, Failure> {
		self.prepare_with(extensions, handler, vec![argument], budget)
	}
	/// `prepare` for a handler taking any number of positional arguments.
	pub fn prepare_with(
		&self,
		extensions: Extensions,
		handler: &str,
		arguments: Vec<Value>,
		budget: usize,
	) -> Result<Invocation, Failure> {
		if budget == 0 || budget > crate::runner::LARGEST_BUDGET {
			return Err(Failure::new(
				"preparation",
				format!("budget must be 1 to {}", crate::runner::LARGEST_BUDGET),
			));
		}
		let (context, owner) = context(extensions)?;
		let runtime = match context.runtime() {
			Ok(runtime) => Arc::new(runtime),
			Err(error) => {
				owner.close()?;
				return Err(Failure::new("preparation", error));
			}
		};
		Ok(Invocation {
			program: self.clone(),
			runtime,
			owner,
			arguments: Some(arguments),
			handler: handler.into(),
			budget,
			started: false,
			failure: None,
		})
	}
	fn fault(&self, error: VmError) -> Failure {
		let mut failure = Failure::new("vm", &error);
		if let Some(location) = error.first_location()
			&& Arc::ptr_eq(&location.unit, &self.0.unit)
			&& let Some(instruction) = location
				.unit
				.debug_info()
				.and_then(|d| d.instruction_at(location.ip))
			&& let Some(text) = self.0.loader.get(&self.0.sources, instruction.source_id)
		{
			if let Some(named) = crate::method::named(&failure.message, &text.text) {
				failure.message = named;
			}
			failure = failure.locate(text, instruction.span.range().start);
		}
		failure
	}
}
/// One execution of `handler` on a fresh VM over `runtime`: shared by [`Invocation::run`] and
/// [`SlotInvocation::run`] so the two can't diverge. A halt with no location while the budget
/// is spent is reported as the budget itself, as the CLI reads it (record 0107).
async fn execute(
	program: &Program,
	runtime: Arc<RuntimeContext>,
	handler: &str,
	arguments: Vec<Value>,
	budget: usize,
) -> Result<Value, Failure> {
	let mut vm = Vm::new(runtime, program.0.unit.clone());
	match vm.execute(
		rune::Hash::type_hash(handler.split("::").collect::<Vec<_>>().as_slice()),
		arguments,
	) {
		Ok(execution) => {
			let exhausted = std::rc::Rc::new(std::cell::Cell::new(false));
			let settled = crate::execute::Settled {
				inner: Box::pin(async move {
					let mut execution = execution;
					execution.async_resume().await
				}),
				exhausted: exhausted.clone(),
			};
			match rune::runtime::budget::with(budget, settled)
				.await
				.into_result()
			{
				Ok(rune::runtime::GeneratorState::Complete(value)) => Ok(value),
				Ok(_) => Err(Failure::new("vm", "handler yielded instead of completing")),
				Err(error) if error.first_location().is_none() && exhausted.get() => {
					Err(Failure::new(
						"vm",
						format!("the budget of {budget} instructions was exhausted"),
					))
				}
				Err(error) => {
					let mut failure = program.fault(error);
					if exhausted.get() {
						failure.message.push_str(&format!(
							"; the budget of {budget} instructions was exhausted at that point"
						));
					}
					Err(failure)
				}
			}
		}
		Err(error) => Err(program.fault(error)),
	}
}

/// A thread-local execution owner. Explicitly close it even after abandoning
/// a polled run. Close repeats an execution failure unless cleanup itself failed.
///
/// Invocations cannot cross worker threads:
/// ```compile_fail
/// fn send<T: Send>() {}
/// send::<rnx::server::Invocation>();
/// ```
#[must_use = "explicit close observes cleanup and cancelled execution failures"]
pub struct Invocation {
	program: Program,
	runtime: Arc<RuntimeContext>,
	owner: Owner,
	arguments: Option<Vec<Value>>,
	handler: String,
	budget: usize,
	started: bool,
	failure: Option<Failure>,
}
// Declared before the VM so VM-held values are disposed before finish runs.
struct Finish<'a> {
	invocation: &'a mut Invocation,
	complete: bool,
}
impl Finish<'_> {
	fn finish(&mut self, result: Result<Value, Failure>) -> Result<Value, Failure> {
		self.complete = true;
		let cleanup = self.invocation.owner.life.finish(result.is_err());
		let result = match cleanup {
			Ok(()) => result,
			Err(error) => Err(Failure::new("cleanup", error)),
		};
		self.invocation.failure = result.as_ref().err().cloned();
		result
	}
}
impl Drop for Finish<'_> {
	fn drop(&mut self) {
		if !self.complete {
			self.invocation.failure = Some(match self.invocation.owner.life.finish(true) {
				Ok(()) => Failure::new("cancelled", "execution cancelled"),
				Err(error) => Failure::new("cleanup", error),
			});
		}
	}
}
impl Invocation {
	/// Run once. Dropping a polled future synchronously finishes it as failed.
	/// This drives no runtime and polls no other owner's tasks.
	pub async fn run(&mut self) -> Result<Value, Failure> {
		if self.started {
			return Err(Failure::new("preparation", "invocation has already run"));
		}
		self.started = true;
		self.owner
			.life
			.begin()
			.map_err(|e| Failure::new("cleanup", e))?;
		let mut guard = Finish {
			invocation: self,
			complete: false,
		};
		let result = {
			let this = &mut guard.invocation;
			let arguments = this.arguments.take().expect("one-shot arguments");
			execute(
				&this.program,
				this.runtime.clone(),
				&this.handler,
				arguments,
				this.budget,
			)
			.await
		};
		guard.finish(result)
	}
	/// Retire all owned operations, without entering or draining a runtime.
	/// Cleanup failures take precedence over a remembered execution failure.
	pub fn close(mut self) -> Result<(), Failure> {
		drop(self.arguments.take());
		self.owner.close()?;
		match self.failure.take() {
			Some(error) => Err(error),
			None => Ok(()),
		}
	}
}

/// Record 0163: a reusable invocation slot. One context, runtime and owner (lifecycle and HTTP
/// state) built once by [`Program::slot`], serving one [`SlotInvocation`] at a time. Each
/// invocation runs on a fresh VM with its own budget and lifecycle generation; at every request
/// boundary [`SlotInvocation::close`] revokes every operation the slot owns (`Lifecycle::clear`),
/// keeping only the HTTP client's connection pool. A slot whose cleanup failed is retired and
/// never handed back.
///
/// **Trusted-extension contract.** Extensions installed on a slot live as long as the slot: an
/// extension may capture only state intended to persist across the slot's requests; per-request
/// data travels in the handler's arguments; request-owned operations must use the lifecycle
/// (`Scope::track`) so the boundary revokes them. Mutable state an extension captures otherwise is
/// outside this isolation guarantee. Per-request extensions (such as a database lease) belong on
/// [`Program::prepare`], which builds a fresh context per invocation.
///
/// Values returned by a run belong to the caller: convert them to owned data and drop them
/// before `close`. `close` cannot destroy a `Value` retained elsewhere; lifecycle-tracked futures inside one
/// are revoked and fail with a cancellation error if polled later. Other caller-retained values
/// and untracked futures remain the caller's responsibility.
///
/// Slots cannot cross worker threads:
/// ```compile_fail
/// fn send<T: Send>() {}
/// send::<rnx::server::Slot>();
/// ```
pub struct Slot {
	program: Program,
	runtime: Arc<RuntimeContext>,
	owner: Owner,
}
impl Program {
	/// Build a reusable slot: the whole context and its runtime, once.
	pub fn slot(&self, extensions: Extensions) -> Result<Slot, Failure> {
		let (context, owner) = context(extensions)?;
		let runtime = match context.runtime() {
			Ok(runtime) => Arc::new(runtime),
			Err(error) => {
				owner.close()?;
				return Err(Failure::new("preparation", error));
			}
		};
		Ok(Slot {
			program: self.clone(),
			runtime,
			owner,
		})
	}
}
impl Slot {
	/// The lifecycle is reusable: idle, unretired, generation not exhausted and no failure.
	fn healthy(&self) -> bool {
		self.owner.life.reusable()
	}
	/// Prepare one invocation on this slot. Nothing begins here. A failure hands the slot back
	/// only if it is still healthy after the unused arguments are dropped; otherwise it is retired.
	#[allow(
		clippy::result_large_err,
		reason = "the additive API returns the owned slot without boxing on the request path"
	)]
	pub fn prepare(
		self,
		handler: &str,
		arguments: Vec<Value>,
		budget: usize,
	) -> Result<SlotInvocation, (Failure, Option<Slot>)> {
		if budget == 0 || budget > crate::runner::LARGEST_BUDGET {
			drop(arguments);
			let failure = Failure::new(
				"preparation",
				format!("budget must be 1 to {}", crate::runner::LARGEST_BUDGET),
			);
			return Err(match self.retire_unless_healthy() {
				Ok(slot) => (failure, Some(slot)),
				Err(cleanup) => (cleanup, None),
			});
		}
		Ok(SlotInvocation {
			slot: Some(self),
			arguments: Some(arguments),
			handler: handler.into(),
			budget,
			stage: std::cell::Cell::new(Stage::Ready),
			failure: None,
			cleanup: None,
		})
	}
	/// Hand the slot back if healthy; otherwise retire it and report its cleanup failure.
	fn retire_unless_healthy(self) -> Result<Slot, Failure> {
		if self.healthy() {
			return Ok(self);
		}
		Err(match self.owner.close() {
			Ok(()) => Failure::new("cleanup", "slot lifecycle failed"),
			Err(failure) => failure,
		})
	}
}

/// Where a slot invocation's one lifecycle generation stands: shared by run, its drop guard and
/// close, so `finish` happens exactly once.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Stage {
	/// Not yet polled: no generation has begun (an unpolled run future never executes its body).
	Ready,
	/// `begin` succeeded and the generation has not finished.
	Begun,
	/// The generation finished, or `begin` failed: nothing is left to finish.
	Done,
}

/// Record 0163: one invocation on a [`Slot`]. Run it once, then [`close`](Self::close) it to get
/// the slot back. Dropping it without `close` retires the slot.
///
/// ```compile_fail
/// fn send<T: Send>() {}
/// send::<rnx::server::SlotInvocation>();
/// ```
#[must_use = "close returns the slot; dropping retires it"]
pub struct SlotInvocation {
	slot: Option<Slot>,
	arguments: Option<Vec<Value>>,
	handler: String,
	budget: usize,
	stage: std::cell::Cell<Stage>,
	failure: Option<Failure>,
	/// A lifecycle cleanup failure seen while running: it retires the slot at close.
	cleanup: Option<Failure>,
}
struct SlotFinish<'a> {
	invocation: &'a mut SlotInvocation,
	complete: bool,
}
impl SlotFinish<'_> {
	fn finish(&mut self, result: Result<Value, Failure>) -> Result<Value, Failure> {
		self.complete = true;
		let invocation = &mut *self.invocation;
		invocation.stage.set(Stage::Done);
		let life = &invocation
			.slot
			.as_ref()
			.expect("slot while running")
			.owner
			.life;
		let result = match life.finish(result.is_err()) {
			Ok(()) => result,
			Err(error) => {
				let failure = Failure::new("cleanup", error);
				invocation.cleanup = Some(failure.clone());
				Err(failure)
			}
		};
		invocation.failure = result.as_ref().err().cloned();
		result
	}
}
impl Drop for SlotFinish<'_> {
	fn drop(&mut self) {
		if !self.complete {
			let invocation = &mut *self.invocation;
			invocation.stage.set(Stage::Done);
			let life = &invocation
				.slot
				.as_ref()
				.expect("slot while running")
				.owner
				.life;
			invocation.failure = Some(match life.finish(true) {
				Ok(()) => Failure::new("cancelled", "execution cancelled"),
				Err(error) => {
					let failure = Failure::new("cleanup", error);
					invocation.cleanup = Some(failure.clone());
					failure
				}
			});
		}
	}
}
impl SlotInvocation {
	/// Run once, on a fresh VM. The generation begins when this future is first polled; dropping
	/// an unpolled future leaves the invocation runnable once, and dropping a polled one finishes
	/// it as cancelled.
	pub async fn run(&mut self) -> Result<Value, Failure> {
		if self.stage.get() != Stage::Ready {
			return Err(Failure::new("preparation", "invocation has already run"));
		}
		let slot = self.slot.as_ref().expect("slot before close");
		if let Err(error) = slot.owner.life.begin() {
			self.stage.set(Stage::Done);
			let failure = Failure::new("cleanup", error);
			self.cleanup = Some(failure.clone());
			self.failure = Some(failure.clone());
			return Err(failure);
		}
		self.stage.set(Stage::Begun);
		let (program, runtime) = (slot.program.clone(), slot.runtime.clone());
		let mut guard = SlotFinish {
			invocation: self,
			complete: false,
		};
		let result = {
			let this = &mut guard.invocation;
			let arguments = this.arguments.take().expect("one-shot arguments");
			execute(&program, runtime, &this.handler, arguments, this.budget).await
		};
		guard.finish(result)
	}
	/// End this request and hand the slot back. In order: drop unused arguments; revoke every
	/// operation the slot owns (`Lifecycle::clear`), keeping the HTTP connection pool. A cleanup
	/// failure (while running or now) retires the slot and takes precedence: `Err((cleanup, None))`.
	/// Otherwise an execution failure is `Err((failure, Some(slot)))`, and success `Ok(slot)`.
	#[allow(
		clippy::result_large_err,
		reason = "the additive API returns the owned slot without boxing on the request path"
	)]
	pub fn close(mut self) -> Result<Slot, (Failure, Option<Slot>)> {
		drop(self.arguments.take());
		let slot = self.slot.take().expect("slot before close");
		debug_assert_ne!(
			self.stage.get(),
			Stage::Begun,
			"a run future finishes its generation"
		);
		let cleared = slot
			.owner
			.life
			.clear()
			.map_err(|e| Failure::new("cleanup", e));
		if let Some(cleanup) = self.cleanup.take().or(cleared.err()) {
			let _ = slot.owner.close();
			return Err((cleanup, None));
		}
		let slot = match slot.retire_unless_healthy() {
			Ok(slot) => slot,
			Err(cleanup) => return Err((cleanup, None)),
		};
		match self.failure.take() {
			Some(failure) => Err((failure, Some(slot))),
			None => Ok(slot),
		}
	}
}
impl Drop for SlotInvocation {
	/// Abandoned without `close`: the slot is retired, never silently returned.
	fn drop(&mut self) {
		drop(self.arguments.take());
		if let Some(slot) = self.slot.take() {
			let _ = slot.owner.close();
		}
	}
}

#[cfg(all(test, feature = "test-support", target_os = "linux"))]
mod host_tests {
	use super::*;
	use std::sync::{
		Condvar, Mutex,
		atomic::{AtomicUsize, Ordering},
	};
	static SIGNALS: AtomicUsize = AtomicUsize::new(0);
	extern "C" fn host_signal(_: libc::c_int) {
		SIGNALS.fetch_add(1, Ordering::SeqCst);
	}
	fn runtime() -> tokio::runtime::Runtime {
		tokio::runtime::Builder::new_current_thread()
			.enable_all()
			.build()
			.unwrap()
	}
	fn invoke(program: &Program, handler: &str) -> Result<Value, Failure> {
		let mut invocation = program
			.prepare(Extensions::none(), handler, Value::from(0i64), 10000)
			.unwrap();
		let result = runtime().block_on(invocation.run());
		let close = invocation.close();
		if result.is_ok() {
			close.unwrap();
		} else {
			assert_eq!(close.unwrap_err().category(), "vm");
		}
		result
	}
	fn preserved_signals(expected: usize) {
		// Exercise delivery, not only the address stored in sigaction.
		unsafe {
			libc::raise(libc::SIGINT);
			libc::raise(libc::SIGTERM);
		}
		assert_eq!(SIGNALS.load(Ordering::SeqCst), expected);
	}
	fn policy_child(entry: &Path, report: &Path) {
		unsafe {
			libc::signal(libc::SIGINT, host_signal as *const () as libc::sighandler_t);
			libc::signal(
				libc::SIGTERM,
				host_signal as *const () as libc::sighandler_t,
			);
		}
		preserved_signals(2);
		// A real preexisting CLI state: omitting the setter would prove nothing.
		crate::host::running_a_script();
		let program = Program::compile(entry, Extensions::none()).unwrap();
		preserved_signals(4);
		let value = invoke(&program, "exit").unwrap();
		let refusal = value.borrow_ref::<Result<Value, Value>>().unwrap();
		let Err(error_value) = &*refusal else {
			panic!("exit did not refuse")
		};
		let error = error_value.borrow_string_ref().unwrap();
		assert_eq!(&*error, "cannot exit: this is a server/embedding context");
		drop(error);
		drop(refusal);
		drop(value);
		preserved_signals(6);
		let failure = invoke(&program, "bad").unwrap_err();
		assert_eq!(failure.category(), "vm");
		assert!(failure.message().contains("missing"));
		assert_eq!(failure.path(), Some(entry));
		preserved_signals(8);
		// Force an owned compile diagnostic as well, without printing it.
		let invalid = entry.with_file_name("invalid.rn");
		let failed = Program::compile(invalid, Extensions::none()).err().unwrap();
		assert_eq!(failed.category(), "preparation");
		crate::config::report_reads();
		assert_eq!(std::fs::read_to_string(report).unwrap(), "0");
		// Positive control: the same valid installed config really is counted.
		let _ = crate::config::load();
		crate::config::report_reads();
		assert_eq!(std::fs::read_to_string(report).unwrap(), "1");
		preserved_signals(10);
		println!(
			"HOST policy passed: signals=10, exit refused with script flag, config reads=0 then control=1, owned diagnostics"
		);
	}
	fn blocked_child(entry: &Path) {
		let gate = Arc::new((Mutex::new(false), Condvar::new()));
		let (started, observed) = std::sync::mpsc::sync_channel(1);
		let factory = |started: std::sync::mpsc::SyncSender<()>| {
			let gate = gate.clone();
			Extensions::none().with("native", move |module| {
				module
					.function("blocked", move || {
						started.send(()).unwrap();
						let (lock, wake) = &*gate;
						let mut released = lock.lock().unwrap();
						while !*released {
							released = wake.wait(released).unwrap();
						}
						73i64
					})
					.build()
					.map_err(|e| e.to_string())?;
				Ok(vec![])
			})
		};
		let program = Program::compile(entry, factory(started.clone())).unwrap();
		let extension = factory(started);
		// Extensions is deliberately not Send; recreate the worker's captures there.
		drop(extension);
		let worker_gate = gate.clone();
		let (started, observed_worker) = std::sync::mpsc::sync_channel(1);
		let worker = std::thread::spawn(move || {
			let extension = Extensions::none().with("native", move |module| {
				module
					.function("blocked", move || {
						started.send(()).unwrap();
						let (lock, wake) = &*worker_gate;
						let mut released = lock.lock().unwrap();
						while !*released {
							released = wake.wait(released).unwrap();
						}
						73i64
					})
					.build()
					.map_err(|e| e.to_string())?;
				Ok(vec![])
			});
			let mut invocation = program
				.prepare(extension, "blocked", Value::from(0i64), 10000)
				.unwrap();
			let value = runtime().block_on(invocation.run()).unwrap();
			assert_eq!(value.as_integer::<i64>().unwrap(), 73);
			drop(value);
			invocation.close().unwrap();
		});
		observed_worker
			.recv_timeout(std::time::Duration::from_secs(5))
			.unwrap();
		assert!(observed.try_recv().is_err(), "schema executed native code");
		// Longer than the separate standalone server's five-second policy.
		std::thread::sleep(std::time::Duration::from_millis(5250));
		assert!(!worker.is_finished(), "native poll unexpectedly completed");
		println!("HOST deadline passed: worker still owned, host alive");
		*gate.0.lock().unwrap() = true;
		gate.1.notify_all();
		worker.join().unwrap();
		println!("HOST released native poll and joined worker");
	}
	#[test]
	fn subprocess_host_boundary() {
		if let Ok(mode) = std::env::var("RNX_HOST_FIXTURE_CHILD") {
			let entry = PathBuf::from(std::env::var_os("RNX_HOST_FIXTURE_ENTRY").unwrap());
			if mode == "policy" {
				policy_child(
					&entry,
					Path::new(&std::env::var_os("RNX_TEST_CONFIG_READS").unwrap()),
				);
			} else {
				blocked_child(&entry);
			}
			return;
		}
		let dir = std::env::temp_dir().join(format!("rnx-host-boundary-{}", std::process::id()));
		std::fs::create_dir(&dir).unwrap();
		struct Remove(PathBuf);
		impl Drop for Remove {
			fn drop(&mut self) {
				let _ = std::fs::remove_dir_all(&self.0);
			}
		}
		let _remove = Remove(dir.clone());
		std::fs::write(
			dir.join("policy.rn"),
			"pub fn exit(_) { process::exit(7) }\npub fn bad(_) { 1.missing() }\n",
		)
		.unwrap();
		std::fs::write(
			dir.join("blocked.rn"),
			"pub fn blocked(_) { native::blocked() }\n",
		)
		.unwrap();
		std::fs::write(dir.join("invalid.rn"), "pub fn bad(_) { unknown }\n").unwrap();
		std::fs::write(dir.join("config.rn"), "#{}\n").unwrap();
		for mode in ["policy", "blocked"] {
			let output = std::process::Command::new(std::env::current_exe().unwrap())
				.args([
					"--exact",
					"server::host_tests::subprocess_host_boundary",
					"--nocapture",
				])
				.env("RNX_HOST_FIXTURE_CHILD", mode)
				.env("RNX_HOST_FIXTURE_ENTRY", dir.join(format!("{mode}.rn")))
				.env("RNX_CONFIG", dir.join("config.rn"))
				.env("RNX_TEST_CONFIG_READS", dir.join("reads"))
				.output()
				.unwrap();
			assert!(
				output.status.success(),
				"{mode}: {:?} {} {}",
				output.status,
				String::from_utf8_lossy(&output.stdout),
				String::from_utf8_lossy(&output.stderr)
			);
			assert!(
				output.stderr.is_empty(),
				"unexpected stderr: {}",
				String::from_utf8_lossy(&output.stderr)
			);
			let stdout = String::from_utf8(output.stdout).unwrap();
			for line in stdout.lines().filter(|line| !line.is_empty()) {
				assert!(
					line.starts_with("HOST ")
						|| line == "running 1 test"
						|| line == "test server::host_tests::subprocess_host_boundary ... ok"
						|| line.starts_with("test result: ok."),
					"unexpected stdout: {line}"
				);
			}

			for line in stdout.lines().filter(|line| line.starts_with("HOST ")) {
				println!("{line}");
			}
			assert!(stdout.contains(if mode == "policy" {
				"HOST policy passed"
			} else {
				"HOST released native poll and joined worker"
			}));
		}
	}
}

#[cfg(test)]
mod source_tests {
	use super::*;
	fn run(program: &Program, handler: &str, arguments: Vec<Value>) -> Result<Value, Failure> {
		let mut invocation = program
			.prepare_with(Extensions::none(), handler, arguments, 10000)
			.unwrap();
		let result = tokio::runtime::Builder::new_current_thread()
			.enable_all()
			.build()
			.unwrap()
			.block_on(invocation.run());
		let close = invocation.close();
		if result.is_ok() {
			close.unwrap();
		}
		result
	}
	#[test]
	fn in_memory_source_takes_several_arguments() {
		let program = Program::compile_source(
			"<buffer 1>",
			"pub fn main(a, b) { a * 10 + b }\n",
			Extensions::none(),
		)
		.unwrap();
		let value = run(&program, "main", vec![Value::from(4i64), Value::from(2i64)]).unwrap();
		assert_eq!(rune::from_value::<i64>(value).unwrap(), 42);
	}
	#[test]
	fn single_argument_prepare_is_unchanged() {
		let program =
			Program::compile_source("<one>", "pub fn main(a) { a + 1 }\n", Extensions::none())
				.unwrap();
		let mut invocation = program
			.prepare(Extensions::none(), "main", Value::from(1i64), 10000)
			.unwrap();
		let value = tokio::runtime::Builder::new_current_thread()
			.enable_all()
			.build()
			.unwrap()
			.block_on(invocation.run())
			.unwrap();
		invocation.close().unwrap();
		assert_eq!(rune::from_value::<i64>(value).unwrap(), 2);
	}
	/// Record 0140: the server names a missing method on a type outside
	/// 0040's table, and a `Result` receiver gets the `?` hint.
	#[test]
	fn a_missing_method_on_a_result_is_named_with_the_hint() {
		let program = Program::compile_source(
			"<buffer 3>",
			"pub fn main(a) {\n    let r = Ok(a);\n    r.frobnicate()\n}\n",
			Extensions::none(),
		)
		.unwrap();
		let failure = run(&program, "main", vec![Value::from(1i64)])
			.err()
			.unwrap();
		assert!(
			failure.message().starts_with(
				"no method `frobnicate` on `::std::result::Result` (this value is a `Result`"
			),
			"{}",
			failure.message()
		);
		assert_eq!(failure.position().map(|p| p.0), Some(3));
	}
	#[test]
	fn in_memory_compile_failure_names_the_source() {
		let failure = Program::compile_source(
			"<buffer 7>",
			"pub fn main() {\n    unknown\n}\n",
			Extensions::none(),
		)
		.err()
		.unwrap();
		assert_eq!(failure.category(), "preparation");
		assert_eq!(failure.path(), Some(Path::new("<buffer 7>")));
		assert_eq!(failure.position().map(|p| p.0), Some(2));
		assert_eq!(failure.excerpt(), Some("    unknown"));
	}
	#[test]
	fn in_memory_source_refuses_module_declarations() {
		let failure = Program::compile_source(
			"<buffer 2>",
			"mod helpers;\npub fn main() {}\n",
			Extensions::none(),
		)
		.err()
		.unwrap();
		assert!(
			failure
				.message()
				.contains("an in-memory source has no directory"),
			"{failure}"
		);
		assert_eq!(failure.position().map(|p| p.0), Some(1));
	}
	#[test]
	fn an_exhausted_budget_is_named() {
		let program =
			Program::compile_source("<loop>", "pub fn main() { loop {} }\n", Extensions::none())
				.unwrap();
		let failure = run(&program, "main", vec![]).unwrap_err();
		assert_eq!(failure.category(), "vm");
		assert_eq!(
			failure.message(),
			"the budget of 10000 instructions was exhausted"
		);
	}
	#[test]
	fn in_memory_source_counts_against_the_allowance() {
		let text = format!(
			"pub fn main() {{}}\n//{}\n",
			"x".repeat(crate::program::SOURCE_ALLOWANCE)
		);
		let failure = Program::compile_source("<big>", &text, Extensions::none())
			.err()
			.unwrap();
		assert!(failure.message().contains("source allowance"), "{failure}");
	}
}

#[cfg(test)]
mod slot_tests {
	use super::*;
	use std::{
		cell::{Cell, RefCell},
		future::Future,
		pin::Pin,
		sync::atomic::{AtomicI64, Ordering},
		task::{Context, Poll, Waker},
	};
	thread_local! {
		static RELEASE: Cell<bool> = const { Cell::new(false) };
		static DONE: Cell<usize> = const { Cell::new(0) };
		static KEPT: RefCell<Vec<Value>> = const { RefCell::new(Vec::new()) };
		static HTTP_SEEN: RefCell<Option<Arc<std::sync::atomic::AtomicBool>>> = const { RefCell::new(None) };
	}
	struct Gate;
	impl Future for Gate {
		type Output = Result<i64, String>;
		fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
			if RELEASE.get() {
				DONE.set(DONE.get() + 1);
				Poll::Ready(Ok(42))
			} else {
				Poll::Pending
			}
		}
	}
	struct Bomb;
	impl Future for Bomb {
		type Output = Result<i64, String>;
		fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
			Poll::Pending
		}
	}
	impl Drop for Bomb {
		fn drop(&mut self) {
			panic!("injected destructor panic");
		}
	}
	fn poll<F: Future + Unpin>(f: &mut F) -> Poll<F::Output> {
		Pin::new(f).poll(&mut Context::from_waker(Waker::noop()))
	}
	fn extensions() -> Extensions {
		let count = Arc::new(AtomicI64::new(0));
		Extensions::none().with_lifecycle("fix", move |module, scope| {
			let wait = scope.clone();
			module
				.function("wait", move || wait.track(Gate))
				.build()
				.map_err(|e| e.to_string())?;
			module
				.function("bomb", move || scope.track(Bomb))
				.build()
				.map_err(|e| e.to_string())?;
			module
				.function("keep", |v: Value| {
					KEPT.with_borrow_mut(|k| k.push(v));
				})
				.build()
				.map_err(|e| e.to_string())?;
			module
				.function("keep_polled", |v: Value| -> Result<(), String> {
					let mut f =
						rune::from_value::<rune::runtime::Future>(v).map_err(|e| e.to_string())?;
					assert!(poll(&mut f).is_pending());
					KEPT.with_borrow_mut(|k| k.push(rune::to_value(f).unwrap()));
					Ok(())
				})
				.build()
				.map_err(|e| e.to_string())?;
			module
				.function("keep_http_pending", |v: Value| async move {
					let seen = HTTP_SEEN.with_borrow(|s| s.as_ref().unwrap().clone());
					let mut f =
						rune::from_value::<rune::runtime::Future>(v).map_err(|e| e.to_string())?;
					let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
					while !seen.load(Ordering::SeqCst) {
						assert!(poll(&mut f).is_pending());
						assert!(
							std::time::Instant::now() < deadline,
							"HTTP fixture was not reached"
						);
						tokio::time::sleep(std::time::Duration::from_millis(1)).await;
					}
					KEPT.with_borrow_mut(|k| k.push(rune::to_value(f).unwrap()));
					Ok::<(), String>(())
				})
				.build()
				.map_err(|e| e.to_string())?;
			module
				.function("count", move || count.fetch_add(1, Ordering::Relaxed) + 1)
				.build()
				.map_err(|e| e.to_string())?;
			Ok(vec![])
		})
	}
	const SOURCE: &str = r#"
		const ITEMS = [1];
		pub fn add(n) { n + 1 }
		pub fn mutate() {
            let v = ITEMS;
            let original = v.len();
            v.push(2);
            let object = #{value: original};
            object.value += 2;
            let read = || object.value;
            original * 100 + v.len() * 10 + read()
        }
		pub fn panics() { panic!("fixture panic") }
		pub fn spin() { loop {} }
		pub fn count() { fix::count() }
		pub fn escape() { fix::wait() }
		pub fn http_escape(url) { http::get(url) }
		pub fn http_keep_fail(url) { fix::keep(http::get(url)); panic!("fixture panic"); }
        pub async fn http_pending_fail(url) {
            fix::keep_http_pending(http::get(url)).await?;
            panic!("fixture panic");
        }
		pub fn keep_pending(fail) { fix::keep_polled(fix::wait())?; if fail { panic!("fixture panic"); } }
		pub fn keep_then_fail() { fix::keep(fix::wait()); panic!("fixture panic"); }
		pub fn keep_bomb() { fix::keep(fix::bomb()); }
		pub fn polled_bomb_then_fail() { fix::keep_polled(fix::bomb())?; panic!("fixture panic"); }
		pub async fn hold(n) { fix::wait().await?; n }
        pub async fn http_hold(url, n) {
            time::sleep(1).await?;
            let response = http::get(url).await?;
            if response.body != "held" { panic!("wrong response"); }
            n
        }
	"#;
	fn program() -> Program {
		Program::compile_source("<slots>", SOURCE, extensions()).unwrap()
	}
	fn runtime() -> tokio::runtime::Runtime {
		tokio::runtime::Builder::new_current_thread()
			.enable_all()
			.build()
			.unwrap()
	}
	fn prepare(slot: Slot, handler: &str, args: Vec<Value>, budget: usize) -> SlotInvocation {
		slot.prepare(handler, args, budget)
			.unwrap_or_else(|(e, _)| panic!("{e}"))
	}
	fn returned(inv: SlotInvocation) -> Slot {
		match inv.close() {
			Ok(s) | Err((_, Some(s))) => s,
			Err((e, None)) => panic!("{e}"),
		}
	}
	fn run(slot: Slot, handler: &str, args: Vec<Value>) -> (Slot, Result<Value, Failure>) {
		let mut inv = prepare(slot, handler, args, 10000);
		let result = runtime().block_on(inv.run());
		(returned(inv), result)
	}
	fn reset() {
		RELEASE.set(false);
		DONE.set(0);
		KEPT.with_borrow_mut(Vec::clear);
	}
	fn cancelled(v: Value) {
		let mut f = rune::from_value::<rune::runtime::Future>(v).unwrap();
		let Poll::Ready(value) = poll(&mut f) else {
			panic!("revoked future remained pending");
		};
		let value = value.into_result().unwrap();
		let result = value.borrow_ref::<Result<Value, Value>>().unwrap();
		let Err(error) = &*result else {
			panic!("revoked future succeeded");
		};
		assert_eq!(&*error.borrow_string_ref().unwrap(), "operation cancelled");
	}
	#[test]
	fn retired_lifecycle_is_never_returned_and_begin_failure_retires() {
		let p = program();
		let slot = p.slot(extensions()).unwrap();
		slot.owner.life.close().unwrap();
		let (failure, returned) = slot.prepare("add", vec![], 0).err().unwrap();
		assert_eq!(failure.category(), "cleanup");
		assert!(returned.is_none());
		let slot = p.slot(extensions()).unwrap();
		slot.owner.life.close().unwrap();
		let mut inv = prepare(slot, "add", vec![rune::to_value(1).unwrap()], 1000);
		assert_eq!(
			runtime().block_on(inv.run()).unwrap_err().category(),
			"cleanup"
		);
		let (failure, returned) = inv.close().err().unwrap();
		assert_eq!(failure.category(), "cleanup");
		assert!(returned.is_none());
	}
	#[test]
	fn slots_match_existing_prepare_results_and_failures() {
		let p = program();
		let mut slot = p.slot(extensions()).unwrap();
		for (name, args) in [
			("add", vec![Value::from(41i64)]),
			("panics", vec![]),
			("spin", vec![]),
			("missing", vec![]),
		] {
			let mut old = p
				.prepare_with(extensions(), name, args.clone(), 10000)
				.unwrap();
			let a = runtime().block_on(old.run());
			let mut new = prepare(slot, name, args, 10000);
			let b = runtime().block_on(new.run());
			match (a, b) {
				(Ok(a), Ok(b)) => assert_eq!(
					a.as_integer::<i64>().unwrap(),
					b.as_integer::<i64>().unwrap()
				),
				(Err(a), Err(b)) => {
					assert_eq!(a.category(), b.category());
					assert_eq!(a.message(), b.message());
					assert_eq!(a.position(), b.position());
				}
				_ => panic!("different outcomes"),
			}
			let _ = old.close();
			slot = returned(new);
		}
	}
	#[test]
	fn requests_are_fresh_budgets_reset_and_intentional_slot_state_persists() {
		let p = program();
		let mut s = p.slot(extensions()).unwrap();
		for i in 1..=3 {
			let (next, v) = run(s, "mutate", vec![]);
			s = next;
			assert_eq!(v.unwrap().as_integer::<i64>().unwrap(), 123);
			let (next, v) = run(s, "count", vec![]);
			s = next;
			assert_eq!(v.unwrap().as_integer::<i64>().unwrap(), i);
		}
		for name in ["panics", "spin"] {
			let (next, v) = run(s, name, vec![]);
			s = next;
			assert_eq!(v.unwrap_err().category(), "vm");
		}
		let (next, v) = run(s, "add", vec![Value::from(41i64)]);
		s = next;
		assert_eq!(v.unwrap().as_integer::<i64>().unwrap(), 42);
		let (error, next) = s.prepare("add", vec![Value::from(1i64)], 0).err().unwrap();
		assert_eq!(error.category(), "preparation");
		let (_, v) = run(next.unwrap(), "add", vec![Value::from(41i64)]);
		assert_eq!(v.unwrap().as_integer::<i64>().unwrap(), 42);
	}
	#[test]
	fn escaped_unpolled_and_pending_operations_cannot_resurrect() {
		reset();
		let p = program();
		let mut s = p.slot(extensions()).unwrap();
		for (name, args) in [
			("escape", vec![]),
			("keep_then_fail", vec![]),
			("keep_pending", vec![Value::from(false)]),
			("keep_pending", vec![Value::from(true)]),
		] {
			let (next, result) = run(s, name, args);
			s = next;
			let mut retained = KEPT.with_borrow_mut(std::mem::take);
			if name == "escape" {
				retained.push(result.unwrap());
			} else {
				drop(result);
			}
			let mut next = prepare(s, "hold", vec![Value::from(7i64)], 10000);
			let mut future = Box::pin(next.run());
			assert!(poll(&mut future).is_pending());
			RELEASE.set(true);
			for v in retained {
				cancelled(v);
			}
			assert_eq!(DONE.get(), 0);
			let v = runtime().block_on(future).unwrap();
			assert_eq!(v.as_integer::<i64>().unwrap(), 7);
			drop(v);
			s = returned(next);
			RELEASE.set(false);
			DONE.set(0);
		}
	}
	#[test]
	fn unpolled_polled_completed_and_abandoned_runs_obey_one_shot_lifecycle() {
		reset();
		let p = program();
		let mut inv = prepare(
			p.slot(extensions()).unwrap(),
			"add",
			vec![Value::from(1i64)],
			10000,
		);
		drop(inv.run());
		assert_eq!(
			runtime()
				.block_on(inv.run())
				.unwrap()
				.as_integer::<i64>()
				.unwrap(),
			2
		);
		assert_eq!(
			runtime().block_on(inv.run()).unwrap_err().message(),
			"invocation has already run"
		);
		let s = returned(inv);
		let mut inv = prepare(s, "hold", vec![Value::from(1i64)], 10000);
		let mut future = Box::pin(inv.run());
		assert!(poll(&mut future).is_pending());
		drop(future);
		let (error, s) = inv.close().err().unwrap();
		assert_eq!(error.category(), "cancelled");
		let inv = prepare(s.unwrap(), "add", vec![Value::from(1i64)], 10000);
		let s = returned(inv);
		let mut inv = prepare(s, "escape", vec![], 10000);
		let v = runtime().block_on(inv.run()).unwrap();
		drop(inv);
		RELEASE.set(true);
		cancelled(v);
		assert_eq!(DONE.get(), 0);
	}
	#[test]
	fn cleanup_failures_win_and_retire_instead_of_returning_slots() {
		for name in ["keep_bomb", "polled_bomb_then_fail"] {
			reset();
			let p = program();
			let mut inv = prepare(p.slot(extensions()).unwrap(), name, vec![], 10000);
			let result = runtime().block_on(inv.run());
			if name == "polled_bomb_then_fail" {
				assert_eq!(result.unwrap_err().category(), "cleanup");
			} else {
				drop(result);
			}
			let (e, s) = inv.close().err().unwrap();
			assert_eq!(e.category(), "cleanup");
			assert!(s.is_none());
			assert!(e.message().contains("injected destructor panic"));
			KEPT.with_borrow_mut(Vec::clear);
		}
		reset();
		let p = program();
		let s = p.slot(extensions()).unwrap();
		let bomb = s.owner.life.scope("fixture").track(Bomb);
		#[derive(rune::Any)]
		struct BombArgument {
			_owned: Pin<Box<dyn Future<Output = Result<i64, String>>>>,
		}
		let argument = Value::new(BombArgument {
			_owned: Box::pin(bomb),
		})
		.unwrap();
		let (e, s) = s.prepare("add", vec![argument], 0).err().unwrap();
		assert_eq!(e.category(), "cleanup");
		assert!(s.is_none());
	}
	#[test]
	fn held_http_operations_are_cancelled_before_the_next_generation() {
		use std::{
			io::{Read, Write},
			net::TcpListener,
			sync::{atomic::AtomicBool, mpsc},
			time::{Duration, Instant},
		};
		for failed in [false, true] {
			for pending in [false, true] {
				reset();
				let listener = TcpListener::bind("127.0.0.1:0").unwrap();
				listener.set_nonblocking(true).unwrap();
				let url = format!("http://{}/held", listener.local_addr().unwrap());
				let observed = Arc::new(AtomicBool::new(false));
				HTTP_SEEN.with_borrow_mut(|s| *s = Some(observed.clone()));
				let seen = observed.clone();
				let (release, wait) = mpsc::channel();
				let server = std::thread::spawn(move || {
					let end = Instant::now() + Duration::from_secs(5);
					loop {
						match listener.accept() {
							Ok((mut stream, _)) => {
								stream
									.set_read_timeout(Some(Duration::from_secs(3)))
									.unwrap();
								let mut request = Vec::new();
								let mut b = [0u8; 1];
								while !request.ends_with(b"\r\n\r\n") {
									if stream.read(&mut b).unwrap_or(0) == 0 {
										return false;
									}
									request.push(b[0]);
								}
								seen.store(true, Ordering::SeqCst);
								wait.recv_timeout(Duration::from_secs(5)).unwrap();
								let _=stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
								return true;
							}
							Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
								if wait.try_recv().is_ok() {
									return false;
								}
								assert!(Instant::now() < end);
								std::thread::sleep(Duration::from_millis(1));
							}
							Err(e) => panic!("{e}"),
						}
					}
				});
				let p = program();
				let mut inv = prepare(
					p.slot(extensions()).unwrap(),
					if failed && pending {
						"http_pending_fail"
					} else if failed {
						"http_keep_fail"
					} else {
						"http_escape"
					},
					vec![rune::to_value(url).unwrap()],
					10000,
				);
				let rt = runtime();
				let result = rt.block_on(inv.run());
				if failed && pending {
					assert!(observed.load(Ordering::SeqCst));
				}
				let v = if failed {
					assert!(result.is_err());
					KEPT.with_borrow_mut(|v| v.pop().unwrap())
				} else {
					result.unwrap()
				};
				let mut f = rune::from_value::<rune::runtime::Future>(v).unwrap();
				if pending {
					rt.block_on(async {
						let end = Instant::now() + Duration::from_secs(3);
						while !observed.load(Ordering::SeqCst) {
							assert!(poll(&mut f).is_pending());
							assert!(Instant::now() < end);
							tokio::time::sleep(Duration::from_millis(1)).await;
						}
					});
				}
				let s = returned(inv);
				let mut next = prepare(s, "hold", vec![Value::from(1i64)], 10000);
				let mut nf = Box::pin(next.run());
				assert!(poll(&mut nf).is_pending());
				release.send(()).unwrap();
				cancelled(rune::to_value(f).unwrap());
				assert_eq!(server.join().unwrap(), pending);
				drop(nf);
				let _ = returned(next);
			}
		}
	}
	#[test]
	fn four_interleaved_slots_keep_cancellation_and_retirement_local() {
		reset();
		let p = program();
		let [mut a, mut b, mut c, mut d] = std::array::from_fn(|i| {
			prepare(
				p.slot(extensions()).unwrap(),
				"hold",
				vec![Value::from(i as i64)],
				10000,
			)
		});
		let mut fa = Box::pin(a.run());
		let mut fb = Box::pin(b.run());
		let mut fc = Box::pin(c.run());
		let mut fd = Box::pin(d.run());
		assert!(poll(&mut fa).is_pending());
		assert!(poll(&mut fb).is_pending());
		assert!(poll(&mut fc).is_pending());
		assert!(poll(&mut fd).is_pending());
		drop(fa);
		drop(a);
		drop(fb);
		let (e, s) = b.close().err().unwrap();
		assert_eq!(e.category(), "cancelled");
		assert!(s.is_some());
		RELEASE.set(true);
		assert_eq!(
			runtime().block_on(fc).unwrap().as_integer::<i64>().unwrap(),
			2
		);
		assert_eq!(
			runtime().block_on(fd).unwrap().as_integer::<i64>().unwrap(),
			3
		);
		assert_eq!(DONE.get(), 2);
		assert!(c.close().is_ok());
		assert!(d.close().is_ok());
	}
	#[test]
	fn four_slots_on_one_localset_interleave_timers_and_http() {
		use tokio::io::{AsyncReadExt, AsyncWriteExt};
		runtime().block_on(tokio::task::LocalSet::new().run_until(async {
            tokio::time::timeout(std::time::Duration::from_secs(5), async {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let url = format!("http://{}/", listener.local_addr().unwrap());
                let (seen, mut received) = tokio::sync::mpsc::unbounded_channel();
                let release = std::rc::Rc::new(tokio::sync::Notify::new());
                let server_release = release.clone();
                let server = tokio::task::spawn_local(async move {
                    let mut tasks = tokio::task::JoinSet::new();
                    for _ in 0..4 {
                        let (mut stream, _) = listener.accept().await.unwrap();
                        let release = server_release.clone();
                        let seen = seen.clone();
                        tasks.spawn_local(async move {
                            let mut request = Vec::new();
                            while !request.ends_with(b"\r\n\r\n") {
                                let mut byte = [0];
                                stream.read_exact(&mut byte).await.unwrap();
                                request.push(byte[0]);
                                assert!(request.len() < 8192);
                            }
                            let released = release.notified();
                            tokio::pin!(released);
                            released.as_mut().enable();
                            seen.send(()).unwrap();
                            released.await;
                            // Cancelled clients may have closed their sockets.
                            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\nheld").await;
                        });
                    }
                    while let Some(result) = tasks.join_next().await { result.unwrap(); }
                });
                let p = program();
                let [mut a, mut b, mut c, mut d] = std::array::from_fn(|i| {
                    prepare(p.slot(extensions()).unwrap(), "http_hold",
                        vec![rune::to_value(url.clone()).unwrap(), Value::from(i as i64)], 10000)
                });
                let mut fa = Box::pin(a.run());
                let mut fb = Box::pin(b.run());
                let mut fc = Box::pin(c.run());
                let mut fd = Box::pin(d.run());
                let mut count = 0;
                while count < 4 {
                    assert!(poll(&mut fa).is_pending());
                    assert!(poll(&mut fb).is_pending());
                    assert!(poll(&mut fc).is_pending());
                    assert!(poll(&mut fd).is_pending());
                    while received.try_recv().is_ok() { count += 1; }
                    tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                }
                drop(fa);
                drop(a); // Retire one owner.
                drop(fb);
                let (failure, slot) = b.close().err().unwrap(); // Clear another.
                assert_eq!(failure.category(), "cancelled");
                let mut next = prepare(slot.unwrap(), "add", vec![Value::from(41i64)], 10000);
                assert_eq!(next.run().await.unwrap().as_integer::<i64>().unwrap(), 42);
                assert!(next.close().is_ok());
                release.notify_waiters();
                assert_eq!(fc.await.unwrap().as_integer::<i64>().unwrap(), 2);
                assert_eq!(fd.await.unwrap().as_integer::<i64>().unwrap(), 3);
                assert!(c.close().is_ok());
                assert!(d.close().is_ok());
                server.await.unwrap();
            }).await.expect("four-slot HTTP fixture timed out");
        }));
	}
}
