//! Record 0044: launch settings over the existing host supervisor.
use crate::host_input::Input;
use rune::runtime::Value;
use rune::{Context, Module};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

type Result<T> = std::result::Result<T, String>;

pub(crate) struct Launch {
	program: OsString,
	pub(crate) cwd: Option<PathBuf>,
	env: Vec<(String, Option<String>)>,
	clear: bool,
}

impl Launch {
	pub(crate) fn command(&self) -> Command {
		let mut command = Command::new(&self.program);
		if let Some(cwd) = &self.cwd {
			command.current_dir(cwd);
		}
		if self.clear {
			command.env_clear();
		}
		for (key, value) in &self.env {
			match value {
				Some(value) => {
					command.env(key, value);
				}
				None => {
					command.env_remove(key);
				}
			}
		}
		command
	}
}

fn string(value: Input<'_>, name: &str) -> Result<String> {
	value
		.string()
		.map_err(|_| format!("{name} must be a string"))
}

fn no_nul(text: &str, name: &str) -> Result<()> {
	if text.contains('\0') {
		Err(format!("{name} contains NUL"))
	} else {
		Ok(())
	}
}

fn explicit(path: &Path) -> bool {
	path.as_os_str()
		.to_str()
		.is_some_and(|s| s.chars().any(std::path::is_separator) || s == "." || s == "..")
		|| path.has_root()
		|| has_prefix(path)
}

#[cfg(windows)]
fn has_prefix(path: &Path) -> bool {
	matches!(
		path.components().next(),
		Some(std::path::Component::Prefix(_))
	)
}
#[cfg(not(windows))]
fn has_prefix(_: &Path) -> bool {
	false
}

fn validate_path(path: &Path, name: &str) -> Result<()> {
	if has_prefix(path) && !path.has_root() {
		return Err(format!("{name} is drive-relative; use an absolute path"));
	}
	Ok(())
}

fn resolve(path: &Path, parent: &Path) -> PathBuf {
	// PathBuf::push supplies the parent's Windows prefix for a rooted path,
	// and keeps .. for ordinary paths. Verbatim paths retain std's rules.
	if path.is_absolute() {
		path.to_owned()
	} else {
		parent.join(path)
	}
}

struct Request {
	arguments: Vec<String>,
	launch: Launch,
	timeout: u64,
	input: Option<Vec<u8>>,
}

impl Request {
	fn parse(program: &str, args: Input<'_>, options: Input<'_>) -> Result<Self> {
		if program.is_empty() {
			return Err("program is empty".into());
		}
		no_nul(program, "program")?;
		let mut arguments = Vec::new();
		args.vector("args must be a vector of strings", |index, value| {
			let name = format!("argument {}", index + 1);
			let text = string(value, &name)?;
			no_nul(&text, &name)?;
			arguments.push(text);
			Ok(())
		})?;
		let mut timeout = 30_000;
		let mut input = None;
		let mut cwd = None;
		let mut env = Vec::new();
		let mut clear = false;
		options.object("options must be an object", |key, value| {
			match key {
				"timeout_ms" => {
					timeout = value
						.integer::<u64>()
						.map_err(|_| "timeout_ms must be a non-negative integer")?;
					if !(1..=90_000).contains(&timeout) {
						return Err("the deadline must be between 1 and 90000 ms".into());
					}
				}
				"input" => {
					input = Some(
						value.with_bytes("input must be a String or Bytes", |b| Ok(b.to_vec()))?,
					);
				}
				"cwd" => {
					let text = string(value, "cwd")?;
					if text.is_empty() {
						return Err("cwd is empty".into());
					}
					no_nul(&text, "cwd")?;
					cwd = Some(PathBuf::from(text));
				}
				"env_clear" => {
					clear = value.boolean().ok_or("env_clear must be a boolean")?;
				}
				"env" => {
					// Command's own key comparison is authoritative, notably on
					// Windows. This only builds settings; it never starts a child.
					let mut keys = Command::new(program);
					value.object("env must be an object", |key, value| {
						if key.is_empty() || key.contains(['=', '\0']) {
							return Err(format!(
								"environment variable {key:?}: name must be nonempty and contain neither '=' nor NUL"
							));
						}
						let before = keys.get_envs().count();
						keys.env(key, "");
						if keys.get_envs().count() == before {
							return Err(format!(
								"environment variable {key:?}: duplicate name under the platform's comparison"
							));
						}
						let setting = if let Ok(text) = value.string() {
							no_nul(&text, &format!("environment variable {key:?}: value"))?;
							Some(text.to_string())
						} else if value.none() {
							None
						} else {
							return Err(format!(
								"environment variable {key:?}: value must be a string or None"
							));
						};
						env.push((key.to_string(), setting));
						Ok(())
					})?;
				}
				_ => return Err(format!("unknown option {key:?}")),
			}
			Ok(())
		})?;
		let path = Path::new(program);
		validate_path(path, "program")?;
		if let Some(path) = &cwd {
			validate_path(path, "cwd")?;
		}
		let explicit = explicit(path);
		let needs_parent = (explicit && !path.is_absolute())
			|| cwd.as_ref().is_some_and(|path| !path.is_absolute());
		let parent = if needs_parent {
			std::env::current_dir()
				.map_err(|e| format!("cannot read rnx's working directory: {e}"))?
		} else {
			PathBuf::new()
		};
		Ok(Self {
			arguments,
			launch: Launch {
				program: if explicit {
					resolve(path, &parent).into_os_string()
				} else {
					program.into()
				},
				cwd: cwd.map(|path| resolve(&path, &parent)),
				env,
				clear,
			},
			timeout,
			input,
		})
	}
}

pub(crate) fn run(program: &str, args: Value, options: Value, bytes: bool) -> Result<Value> {
	let request = Request::parse(program, Input::Legacy(&args), Input::Legacy(&options))
		.map_err(|e| format!("cannot run {program}: {e}"))?;
	launch(program, request, bytes)
}
fn launch(program: &str, request: Request, bytes: bool) -> Result<Value> {
	crate::host::configured_process(
		program,
		&request.arguments,
		request.timeout,
		request.input,
		&request.launch,
		bytes,
	)
}
fn run_execution(
	program: &str,
	args: rune::host::OwnedTreeInput,
	options: rune::host::OwnedTreeInput,
) -> std::result::Result<Result<Value>, rune::runtime::VmError> {
	let request = Request::parse(program, Input::owned(&args), Input::owned(&options))
		.map_err(|e| format!("cannot run {program}: {e}"));
	// No packet or borrowed view lives across child spawn/wait/cleanup.
	drop(args);
	drop(options);
	finish_execution(request.and_then(|request| launch(program, request, false)))
}
pub(crate) fn finish_execution(
	result: Result<Value>,
) -> std::result::Result<Result<Value>, rune::runtime::VmError> {
	if let Some(error) = crate::execute::interrupted_host_call() {
		return Err(error);
	}
	Ok(result)
}

const STRING_PROCESS: rune::host::OutputCertificate = {
	use rune::host::{HostType as T, OutputShape as S};
	rune::host::OutputCertificate {
		nodes: &[
			S::Scalar(T::Unit),
			S::Scalar(T::I64),
			S::Scalar(T::Bool),
			S::Scalar(T::String),
			S::Union(&[0, 1, 2, 3]),
			S::Object(4),
			S::Result(5, 3),
		],
		root: 6,
	}
};

macro_rules! contract { () => { "(program, args, options) -> Result<#{code, stdout, stderr, timed_out, cancelled, truncated, cut_short, unreadable}>: synchronous supervised child; run returns strict UTF-8 stdout/stderr, run_bytes returns Bytes; options: timeout_ms (default 30000, 1..=90000), input (String or Bytes), cwd, env (String values set, None removes), env_clear (default false). Explicit relative program paths always resolve against rnx's directory, even without cwd; bare names use platform lookup, whose PATH-override behavior is platform-specific. The deadline starts after spawn. Check timed_out/cancelled before code, then truncated/cut_short/unreadable before trusting capture completeness. Killed children have no code on Unix and code 1 on Windows. Nonzero exit is Ok; launch/validation failures are Err. Input delivery does not certify consumption. No global environment or directory changes." }; }

pub(crate) fn install(context: &mut Context) -> crate::Result<Vec<crate::host::HostFunction>> {
	install_with_exit(context, crate::host::exit)
}

pub(crate) fn install_with_exit(
	context: &mut Context,
	exit: fn(i64) -> Result<()>,
) -> crate::Result<Vec<crate::host::HostFunction>> {
	let mut module = Module::with_crate("process")?;
	module.host_function_tree_inputs(
		"run",
		|program: &str, args: Value, options: Value| run(program, args, options, false),
		run_execution,
		Some(STRING_PROCESS),
	)?;
	module
		.function("run_bytes", |program: &str, args: Value, options: Value| {
			run(program, args, options, true)
		})
		.build()?;
	module.host_function("exit", exit)?;
	context.install(module)?;
	Ok(vec![
		crate::host::HostFunction {
			path: "process::exit".into(),
			doc: "exit(code) -> Result<()>: end the script with this status, which must be 0 to 255; a status outside that ends the script with 1 rather than being truncated; in run or eval it never returns, and in a session or worker it is refused with Err",
		},
		crate::host::HostFunction {
			path: "process::run".into(),
			doc: concat!("run", contract!()),
		},
		crate::host::HostFunction {
			path: "process::run_bytes".into(),
			doc: concat!("run_bytes", contract!()),
		},
	])
}

#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn the_module_registers_exactly_the_three_documented_names() {
		let mut context = Context::with_default_modules().unwrap();
		let functions = install(&mut context).unwrap();
		assert_eq!(
			functions
				.iter()
				.map(|f| f.path.as_str())
				.collect::<Vec<_>>(),
			["process::exit", "process::run", "process::run_bytes"]
		);
	}
	#[test]
	fn path_preparation_keeps_parent_components_and_bare_names_distinct() {
		assert!(!explicit(Path::new("git")));
		assert!(explicit(Path::new("./git")));
		assert!(explicit(Path::new("..")));
		#[cfg(unix)]
		{
			assert_eq!(
				resolve(Path::new("link/../tool"), Path::new("/parent")),
				Path::new("/parent/link/../tool")
			);
			assert!(!explicit(Path::new("a\\b")));
		}
		#[cfg(windows)]
		{
			assert!(explicit(Path::new("a\\b")));
			assert!(validate_path(Path::new("C:tool"), "program").is_err());
			assert_eq!(
				resolve(Path::new(r"\tool"), Path::new(r"C:\parent")),
				Path::new(r"C:\tool")
			);
			assert_eq!(
				resolve(Path::new(r"\\?\C:\tool"), Path::new(r"D:\parent")),
				Path::new(r"\\?\C:\tool")
			);
		}
	}
}
