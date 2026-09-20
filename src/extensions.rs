//! Trusted extension assembly, separate from the pure settings context.
use crate::host::HostFunction;
use rune::{Context, Module, SourceId};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

type Builder = Box<
	dyn FnOnce(
		&mut Module,
		&crate::lifecycle::Lifecycle,
	) -> Result<Vec<(String, &'static str)>, String>,
>;
struct Extension {
	name: &'static str,
	build: Builder,
}
type Registrar = Box<dyn FnOnce(&mut crate::present::Presenters) -> Result<(), String>>;
struct Presentation {
	name: &'static str,
	register: Registrar,
}
/// What installation produced: help entries and the context's presenters.
pub(crate) struct Installed {
	pub functions: Vec<HostFunction>,
	pub presenters: crate::present::Presenters,
}

/// Ordered, lazy builders for trusted native modules.
///
/// A builder receives a module created under its declared crate name and
/// returns `(path, help)` entries. It must keep registrations in that namespace.
/// Name and help checks catch mistakes, not hostile Rust code: a builder can
/// replace the module, perform I/O, spawn threads, or terminate the process.
/// Only an ordinary panic unwinding through the builder call is converted.
pub struct Extensions {
	builders: Vec<Extension>,
	presentations: Vec<Presentation>,
}

impl Extensions {
	pub(crate) fn names(&self) -> Vec<String> {
		self.builders.iter().map(|e| e.name.to_owned()).collect()
	}
	/// No native extensions; this is the stock rnx executable.
	pub fn none() -> Self {
		Self {
			builders: Vec::new(),
			presentations: Vec::new(),
		}
	}

	/// Append a builder without running it. Closures may capture non-Send
	/// state, but must own it (`'static`). Invoke main_with from main's thread.
	pub fn with(
		mut self,
		name: &'static str,
		build: impl FnOnce(&mut Module) -> Result<Vec<(String, &'static str)>, String> + 'static,
	) -> Self {
		self.builders.push(Extension {
			name,
			build: Box::new(move |module, _| build(module)),
		});
		self
	}

	/// Append a trusted builder with a context-owned operation scope.
	pub fn with_lifecycle(
		mut self,
		name: &'static str,
		build: impl FnOnce(&mut Module, crate::Scope) -> Result<Vec<(String, &'static str)>, String>
		+ 'static,
	) -> Self {
		self.builders.push(Extension {
			name,
			build: Box::new(move |module, lifecycle| build(module, lifecycle.scope(name))),
		});
		self
	}

	/// Record 0068: append a presentation registrar for the extension `name`
	/// without running it. It runs during installation, after the named
	/// builder succeeded, and may register one presenter per native type.
	/// A registrar for a name with no builder is refused at installation.
	pub fn present(
		mut self,
		name: &'static str,
		register: impl FnOnce(&mut crate::present::Presenters) -> Result<(), String> + 'static,
	) -> Self {
		self.presentations.push(Presentation {
			name,
			register: Box::new(register),
		});
		self
	}
	#[cfg(test)]
	pub(crate) fn lifecycle(&self) -> Result<crate::lifecycle::Lifecycle, String> {
		crate::lifecycle::Lifecycle::new(true)
	}
	#[cfg(test)]
	pub(crate) fn install(self, context: &mut Context) -> Result<Vec<HostFunction>, String> {
		let lifecycle = self.lifecycle()?;
		Ok(self.install_with(context, &lifecycle)?.functions)
	}
	pub(crate) fn install_with(
		self,
		context: &mut Context,
		lifecycle: &crate::lifecycle::Lifecycle,
	) -> Result<Installed, String> {
		let mut installed = Vec::new();
		let mut functions = Vec::new();
		for extension in self.builders {
			let name = extension.name;
			let failure = |reason| format!("extension `{name}` could not be installed: {reason}");
			if !identifier(name) {
				return Err(failure("the name must be a Rune identifier".into()));
			}
			if [
				"std", "json", "io", "process", "fs", "path", "time", "text", "http", "env",
				"rnx_test",
			]
			.contains(&name)
			{
				return Err(failure("the name is reserved by rnx".into()));
			}
			if installed.contains(&name) {
				return Err(failure(
					"the name was already registered by an extension".into(),
				));
			}
			installed.push(name);
			let mut module = Module::with_crate(name).map_err(|e| failure(e.to_string()))?;
			let entries =
				build_catching(|| (extension.build)(&mut module, lifecycle)).map_err(&failure)?;
			let prefix = format!("{name}::");
			for (path, _) in &entries {
				if !path
					.strip_prefix(&prefix)
					.is_some_and(|rest| !rest.is_empty() && rest.split("::").all(identifier))
				{
					return Err(failure(format!(
						"help path `{path}` must name an item under `{name}::`"
					)));
				}
			}
			context
				.install(module)
				.map_err(|e| failure(e.to_string()))?;
			functions.extend(
				entries
					.into_iter()
					.map(|(path, doc)| HostFunction { path, doc }),
			);
		}
		let mut presenters = crate::present::Presenters::default();
		for presentation in self.presentations {
			let name = presentation.name;
			let failure =
				|reason| format!("presentation for `{name}` could not be installed: {reason}");
			if !installed.contains(&name) {
				return Err(failure("no extension of that name was installed".into()));
			}
			presenters.set_owner(name);
			build_catching(|| (presentation.register)(&mut presenters)).map_err(&failure)?;
		}
		Ok(Installed {
			functions,
			presenters,
		})
	}
}

fn identifier(name: &str) -> bool {
	use rune::ast::Spanned;
	rune::parse::parse_all::<rune::ast::Ident>(name, SourceId::empty(), false)
		.is_ok_and(|id| id.span().range() == (0..name.len()))
}

type Hook = Box<dyn Fn(&std::panic::PanicHookInfo<'_>) + Send + Sync + 'static>;
struct Hooks {
	active: usize,
	previous: Option<Arc<Hook>>,
}
static HOOKS: std::sync::Mutex<Hooks> = std::sync::Mutex::new(Hooks {
	active: 0,
	previous: None,
});
thread_local! { static QUIET: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }

pub(crate) fn build_catching<T>(build: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
	// Rust forbids changing hooks while unwinding. Cleanup can still catch a
	// destructor panic; leave the caller's hook intact on this path.
	if std::thread::panicking() {
		return panic_result(catch_unwind(AssertUnwindSafe(build)));
	}

	// Scope misuse can be refused on another thread, and dropping that supplied
	// future can panic. Share one dispatcher across overlapping catches; only
	// mutate the hook under this lock, never run adapter code under it. Nested
	// catches and a builder joining a catching child therefore cannot deadlock.
	{
		let mut hooks = HOOKS.lock().unwrap_or_else(|e| e.into_inner());
		if hooks.active == 0 {
			let previous = Arc::new(std::panic::take_hook());
			let delegate = previous.clone();
			std::panic::set_hook(Box::new(move |info| {
				if QUIET.with(|quiet| quiet.get() == 0) {
					delegate(info);
				}
			}));
			hooks.previous = Some(previous);
		}
		hooks.active += 1;
		QUIET.with(|quiet| quiet.set(quiet.get() + 1));
	}
	let result = catch_unwind(AssertUnwindSafe(build));
	{
		let mut hooks = HOOKS.lock().unwrap_or_else(|e| e.into_inner());
		QUIET.with(|quiet| quiet.set(quiet.get() - 1));
		hooks.active -= 1;
		if hooks.active == 0 {
			drop(std::panic::take_hook());
			let previous = hooks.previous.take().expect("active catch installed hook");
			std::panic::set_hook(match Arc::try_unwrap(previous) {
				Ok(hook) => hook,
				Err(hook) => Box::new(move |info| hook(info)),
			});
		}
	}
	panic_result(result)
}

fn panic_result<T>(result: std::thread::Result<Result<T, String>>) -> Result<T, String> {
	match result {
		Ok(result) => result,
		Err(payload) => {
			let message = payload
				.downcast_ref::<String>()
				.map(String::as_str)
				.or_else(|| payload.downcast_ref::<&str>().copied())
				.unwrap_or("non-string panic payload");
			Err(format!("panicked: {message}"))
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn cleanup_while_unwinding_keeps_the_original_panic() {
		struct Cleanup;
		impl Drop for Cleanup {
			fn drop(&mut self) {
				assert_eq!(build_catching(|| Ok(7)).unwrap(), 7);
				assert!(
					build_catching::<()>(|| panic!("inner cleanup"))
						.unwrap_err()
						.contains("inner cleanup")
				);
			}
		}
		let panic = catch_unwind(|| {
			let _cleanup = Cleanup;
			panic!("outer caller");
		})
		.unwrap_err();
		assert_eq!(panic.downcast_ref::<&str>(), Some(&"outer caller"));
	}

	#[test]
	fn names_and_help_are_checked_before_installation() {
		for name in [
			"",
			"two words",
			"fixture ",
			"fn",
			"std",
			"json",
			"io",
			"process",
			"fs",
			"path",
			"time",
			"text",
			"http",
			"env",
			"rnx_test",
		] {
			let extensions =
				Extensions::none().with(name, |_| panic!("invalid name reached its builder"));
			let error = extensions.install(&mut Context::new()).err().unwrap();
			assert!(error.contains(&format!("extension `{name}`")), "{error}");
			assert!(!error.contains("panicked"), "{error}");
		}
		for path in [
			"other::x",
			"fixture::",
			"fixture::x y",
			"fixture::x::",
			"fixture::fn",
		] {
			let extensions =
				Extensions::none().with("fixture", move |_| Ok(vec![(path.into(), "doc")]));
			let error = extensions.install(&mut Context::new()).err().unwrap();
			assert!(error.contains("help path"), "{error}");
		}
		let calls = std::rc::Rc::new(std::cell::Cell::new(0));
		let first = calls.clone();
		let second = calls.clone();
		let extensions = Extensions::none()
			.with("fixture", move |_| {
				first.set(first.get() + 1);
				Ok(vec![])
			})
			.with("fixture", move |_| {
				second.set(second.get() + 1);
				Ok(vec![])
			});
		assert!(extensions.install(&mut Context::new()).is_err());
		assert_eq!(calls.get(), 1);
	}
	#[test]
	fn overlapping_and_nested_catches_do_not_hold_a_lock_across_adapter_code() {
		let result = build_catching(|| {
			let child = std::thread::spawn(|| build_catching::<()>(|| panic!("child catch")));
			assert_eq!(
				build_catching::<()>(|| panic!("nested catch")).unwrap_err(),
				"panicked: nested catch"
			);
			assert_eq!(child.join().unwrap().unwrap_err(), "panicked: child catch");
			Ok(())
		});
		assert!(result.is_ok());
	}
}
