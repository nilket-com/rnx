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

/// Ordered, lazy builders for trusted native modules.
///
/// A builder receives a module created under its declared crate name and
/// returns `(path, help)` entries. It must keep registrations in that namespace.
/// Name and help checks catch mistakes, not hostile Rust code: a builder can
/// replace the module, perform I/O, spawn threads, or terminate the process.
/// Only an ordinary panic unwinding through the builder call is converted.
pub struct Extensions {
	builders: Vec<Extension>,
	lifecycle: bool,
}

impl Extensions {
	/// No native extensions; this is the stock rnx executable.
	pub fn none() -> Self {
		Self {
			builders: Vec::new(),
			lifecycle: false,
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
		self.lifecycle = true;
		self.builders.push(Extension {
			name,
			build: Box::new(move |module, lifecycle| build(module, lifecycle.scope(name))),
		});
		self
	}
	pub(crate) fn lifecycle(&self) -> Result<crate::lifecycle::Lifecycle, String> {
		crate::lifecycle::Lifecycle::new(self.lifecycle)
	}
	#[cfg(test)]
	pub(crate) fn install(self, context: &mut Context) -> Result<Vec<HostFunction>, String> {
		let lifecycle = self.lifecycle()?;
		self.install_with(context, &lifecycle)
	}
	pub(crate) fn install_with(
		self,
		context: &mut Context,
		lifecycle: &crate::lifecycle::Lifecycle,
	) -> Result<Vec<HostFunction>, String> {
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
		Ok(functions)
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
