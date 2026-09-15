//! Trusted extension assembly, separate from the pure settings context.
use crate::host::HostFunction;
use rune::{Context, Module, SourceId};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

type Builder = Box<dyn FnOnce(&mut Module) -> Result<Vec<(String, &'static str)>, String>>;
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
}

impl Extensions {
	/// No native extensions; this is the stock rnx executable.
	pub fn none() -> Self {
		Self {
			builders: Vec::new(),
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
			build: Box::new(build),
		});
		self
	}

	pub(crate) fn install(self, context: &mut Context) -> Result<Vec<HostFunction>, String> {
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
			let entries = build_catching(|| (extension.build)(&mut module)).map_err(&failure)?;
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

fn build_catching<T>(build: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
	let owner = std::thread::current().id();
	let previous = Arc::new(std::panic::take_hook());
	let delegate = previous.clone();
	std::panic::set_hook(Box::new(move |info| {
		if std::thread::current().id() != owner {
			delegate(info);
		}
	}));
	let result = catch_unwind(AssertUnwindSafe(build));
	// Drop our hook before recovering the previous one. It owns the only
	// other Arc, so normal restoration returns the original hook unchanged.
	drop(std::panic::take_hook());
	std::panic::set_hook(match Arc::try_unwrap(previous) {
		Ok(hook) => hook,
		Err(hook) => Box::new(move |info| hook(info)),
	});
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
}
