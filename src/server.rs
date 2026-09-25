//! Caller-owned, one-shot handler execution (record 0056).
//!
//! Enable `server-runtime`. This API installs no signals, runs no event loop,
//! and never terminates the host. Trusted native extensions are not sandboxed.
//! Construct fresh extensions with identical registrations for compilation and
//! each invocation. Values and invocations stay on their creating thread.
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
		crate::process::install_with_exit(&mut context, |_| {
			Err("cannot exit: this is a server/embedding context".into())
		})?;
		crate::fs::install(&mut context)?;
		crate::path::install(&mut context)?;
		crate::time::install(&mut context)?;
		crate::text::install(&mut context)?;
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
			let mut vm = Vm::new(this.runtime.clone(), this.program.0.unit.clone());
			let arguments = this.arguments.take().expect("one-shot arguments");
			match vm.execute(
				rune::Hash::type_hash(this.handler.split("::").collect::<Vec<_>>().as_slice()),
				arguments,
			) {
				Ok(execution) => {
					// Read as the CLI reads it (execute.rs): a halt with no
					// location while the budget is spent is the budget itself.
					let exhausted = std::rc::Rc::new(std::cell::Cell::new(false));
					let settled = crate::execute::Settled {
						inner: Box::pin(async move {
							let mut execution = execution;
							execution.async_resume().await
						}),
						exhausted: exhausted.clone(),
					};
					match rune::runtime::budget::with(this.budget, settled)
						.await
						.into_result()
					{
						Ok(rune::runtime::GeneratorState::Complete(value)) => Ok(value),
						Ok(_) => Err(Failure::new("vm", "handler yielded instead of completing")),
						Err(error) if error.first_location().is_none() && exhausted.get() => {
							Err(Failure::new(
								"vm",
								format!("the budget of {} instructions was exhausted", this.budget),
							))
						}
						Err(error) => {
							let mut failure = this.program.fault(error);
							if exhausted.get() {
								failure.message.push_str(&format!(
									"; the budget of {} instructions was exhausted at that point",
									this.budget
								));
							}
							Err(failure)
						}
					}
				}
				Err(error) => Err(this.program.fault(error)),
			}
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
		let failure =
			Program::compile_source("<buffer 2>", "mod helpers;\npub fn main() {}\n", Extensions::none())
				.err()
				.unwrap();
		assert!(
			failure.message().contains("an in-memory source has no directory"),
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
		assert_eq!(failure.message(), "the budget of 10000 instructions was exhausted");
	}
	#[test]
	fn in_memory_source_counts_against_the_allowance() {
		let text = format!("pub fn main() {{}}\n//{}\n", "x".repeat(crate::program::SOURCE_ALLOWANCE));
		let failure = Program::compile_source("<big>", &text, Extensions::none())
			.err()
			.unwrap();
		assert!(failure.message().contains("source allowance"), "{failure}");
	}
}
