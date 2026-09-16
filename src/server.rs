//! Caller-owned, one-shot handler execution (record 0056).
//!
//! Enable `server-runtime`. This API installs no signals, runs no event loop,
//! and never terminates the host. Trusted native extensions are not sandboxed.
//! Construct fresh extensions with identical registrations for compilation and
//! each invocation. Values and invocations stay on their creating thread.
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
		let (context, owner) = context(extensions)?;
		let built = (|| {
			let mut loader = Loader::new();
			let mut sources = Sources::new();
			sources
				.insert(
					loader
						.entry(entry.as_ref())
						.map_err(|e| Failure::new("preparation", e))?,
				)
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
			argument: Some(argument),
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
	argument: Option<Value>,
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
			let argument = this.argument.take().expect("one-shot argument");
			match vm.execute(
				rune::Hash::type_hash(this.handler.split("::").collect::<Vec<_>>().as_slice()),
				(argument,),
			) {
				Ok(mut execution) => {
					match rune::runtime::budget::with(this.budget, execution.async_resume())
						.await
						.into_result()
					{
						Ok(rune::runtime::GeneratorState::Complete(value)) => Ok(value),
						Ok(_) => Err(Failure::new("vm", "handler yielded instead of completing")),
						Err(error) => Err(this.program.fault(error)),
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
		drop(self.argument.take());
		self.owner.close()?;
		match self.failure.take() {
			Some(error) => Err(error),
			None => Ok(()),
		}
	}
}
