//! Read-only process inputs, with one immutable argv snapshot: record 0036.
use crate::host::HostFunction;
use rune::{
	Context, Module,
	runtime::{Object, Value},
};
use std::{ffi::OsString, sync::Arc};

/// Skip argv[0] as an OS string, without attempting to decode it. Positions
/// number the command word as 1. This runs before any command is dispatched.
pub fn command_line(argv: impl IntoIterator<Item = OsString>) -> Result<Vec<String>, String> {
	argv.into_iter()
		.skip(1)
		.enumerate()
		.map(|(index, raw)| {
			raw.into_string()
				.map_err(|raw| format!("rnx: argument {} is not Unicode: {raw:?}", index + 1))
		})
		.collect()
}
fn unicode(raw: OsString, source: &str) -> Result<String, String> {
	raw.into_string()
		.map_err(|raw| format!("cannot read {source}: it is not Unicode: {raw:?}"))
}
fn var(name: &str) -> Result<Option<String>, String> {
	let source = format!("environment variable {name:?}");
	if name.is_empty() || name.contains(['=', '\0']) {
		return Err(format!(
			"cannot read {source}: name must be nonempty and contain neither '=' nor NUL"
		));
	}
	std::env::var_os(name)
		.map(|raw| unicode(raw, &source))
		.transpose()
}
fn vars() -> Result<Value, String> {
	let mut object = Object::new();
	for (name, value) in std::env::vars_os() {
		let name = unicode(name, "environment variable name")?;
		// Validate every entry before deduplication, even an ignored duplicate.
		let value = unicode(value, &format!("environment variable {name:?}"))?;
		if !object.contains_key(name.as_str()) {
			let key = rune::alloc::String::try_from(name.as_str())
				.map_err(|e| format!("cannot list environment: {e}"))?;
			object
				.insert(
					key,
					rune::to_value(value).map_err(|e| format!("cannot list environment: {e}"))?,
				)
				.map_err(|e| format!("cannot list environment: {e}"))?;
		}
	}
	rune::to_value(object).map_err(|e| format!("cannot list environment: {e}"))
}
fn home_dir() -> Result<Option<String>, String> {
	#[cfg(windows)]
	let (variable, fallback) = ("USERPROFILE", "Windows user-profile API");
	#[cfg(not(windows))]
	let (variable, fallback) = ("HOME", "account database");
	let source = if std::env::var_os(variable).is_some_and(|v| !v.is_empty()) {
		variable
	} else {
		fallback
	};
	std::env::home_dir()
		.map(|path| unicode(path.into_os_string(), source))
		.transpose()
}
/// The native function captures ordinary immutable Rust strings. Each call's
/// conversion constructs new Rune strings as well as a new Rune vector.
pub fn install(
	context: &mut Context,
	arguments: Arc<[String]>,
) -> crate::Result<Vec<HostFunction>> {
	let mut module = Module::with_crate("env")?;
	module
		.function("args", move || arguments.to_vec())
		.build()?;
	module.function("var", var).build()?;
	module.function("vars", vars).build()?;
	module.function("home_dir", home_dir).build()?;
	context.install(module)?;
	Ok(vec![
		HostFunction {
			path: "env::args".into(),
			doc: "args() -> Vec<String>: independent copies of the script's arguments; empty in eval and the session",
		},
		HostFunction {
			path: "env::var".into(),
			doc: "var(name) -> Result<Option<String>>: missing is None, empty is Some(\"\"); invalid names and non-Unicode values are refused",
		},
		HostFunction {
			path: "env::vars".into(),
			doc: "vars() -> Result<Object>: the environment, first duplicate retained; any non-Unicode entry refuses the whole value",
		},
		HostFunction {
			path: "env::home_dir".into(),
			doc: "home_dir() -> Result<Option<String>>: the platform's home directory; account/profile fallback can block, non-Unicode paths are refused",
		},
	])
}
#[cfg(test)]
mod tests {
	#[test]
	fn exactly_four_names_and_no_setter() {
		let mut context = rune::Context::with_default_modules().unwrap();
		let names = super::install(&mut context, std::sync::Arc::from([])).unwrap();
		assert_eq!(
			names.iter().map(|n| n.path.as_str()).collect::<Vec<_>>(),
			["env::args", "env::var", "env::vars", "env::home_dir"]
		);
		for function in names {
			crate::compile(&context, &format!("pub fn main() {{ {} }}", function.path)).unwrap();
		}
	}
}
