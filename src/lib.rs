//! Assemble the rnx executable with trusted native Rune extensions.
//!
//! Call [`main_with`] once from your executable's `main`. Use [`rune`] for
//! adapter types so they share rnx's pinned Rune version. All normal CLI,
//! REPL and worker behavior is retained; only serving contexts load extensions.
//! With the default `count-allocations` feature this library supplies the
//! process's global allocator. The executable must not supply a second one.
pub use rune;
mod extensions;
mod lifecycle;
pub use extensions::Extensions;
pub use lifecycle::Scope;

use rune::runtime::Value;
use rune::{Context, Source, Sources, Vm};
use std::sync::Arc;
mod complete;
mod config;
mod declared;
#[cfg(all(windows, feature = "test-support"))]
mod delivery_control;
mod env;
mod execute;
mod format;
mod fs;
mod fs_platform;
mod host;
mod http;
mod inspect;
mod io;
mod json;
mod memory;
mod method;
mod path;
mod platform;
mod presentation;
mod process;
// Record 0025's gate 5 mechanism control. Only where the mechanism it
// gates exists, and only under `test-support`: an ordinary build has no
// such module and no such command.
#[cfg(all(windows, feature = "test-support"))]
mod pipe_control;
mod program;
mod repl;
#[cfg(feature = "test-support")]
mod rnx_test;
mod runner;
#[cfg(feature = "server-runtime")]
pub mod server;
mod session;
mod terminal;
mod text;
mod time;
mod worker;
mod worker_transport;

// Installed for the whole process: the ceiling is enforced against what this
// counts. A build without the feature enforces no ceiling and says so.
#[cfg(feature = "count-allocations")]
#[global_allocator]
static ALLOCATOR: memory::Counting = memory::Counting;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Register the core host APIs and preserve interrupt setup for every caller.
/// The pure config evaluator deliberately does not call this function.
fn install_core(context: &mut Context) -> Result<Vec<host::HostFunction>> {
	platform::watch_for_interrupt();
	let mut functions = json::install(context)?;
	functions.extend(io::install(context)?);
	functions.extend(process::install(context)?);
	#[cfg(feature = "test-support")]
	functions.extend(rnx_test::install(context)?);
	Ok(functions)
}

fn compile(context: &Context, source: &str) -> Result<rune::Unit> {
	let mut sources = Sources::new();
	sources.insert(Source::memory(source)?)?;
	let mut diagnostics = rune::Diagnostics::new();
	let result = rune::prepare(&mut sources)
		.with_context(context)
		.with_diagnostics(&mut diagnostics)
		.build();
	match result {
		Ok(unit) => Ok(unit),
		Err(error) => Err(format!("{error}: {diagnostics:?}\nGenerated source:\n{source}").into()),
	}
}

fn call(context: &Context, source: &str, state: Value) -> Result<Value> {
	let unit = compile(context, source)?;
	let mut vm = Vm::new(Arc::new(context.runtime()?), Arc::new(unit));
	rune::runtime::budget::with(2_000_000, || vm.call(["main"], (state,)))
		.call()
		.map_err(|e| e.to_string().into())
}

/// This build. `0.0.0` means no release has happened, which is record 0026's
/// decision rather than an unset field.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The Rune this build is compiled against. It is written here rather than
/// read from the crate, which exposes no such constant — so a gate parses the
/// manifest's `=` pin and refuses to let the two drift apart.
const RUNE_VERSION: &str = "0.14.2";

/// What rnx does, for someone who asked or who mistyped.
const USAGE: &str = "\
rnx — a Rune scripting environment

  rnx                       a session, the same as `rnx repl`
  rnx repl                  a session: line editing, history, `:help`
  rnx run [flags] FILE ...  run a file's `main`; arguments after FILE are its
  rnx eval SOURCE           evaluate one expression and exit
  rnx selfcheck             assert this build's own invariants and report
  rnx version               this build, and the Rune it is pinned to
  rnx help                  this

Flags for `run`, before the file: --budget N, --debug-source.";

/// Run rnx's normal CLI with the supplied trusted native extensions.
///
/// Version, help, selfcheck and the pure settings evaluator do not invoke
/// builders. Serving contexts invoke each builder once and retain modules
/// across session resets. As with the stock executable, some dispatch paths
/// exit the process; ordinary returns preserve Rust's `Termination` behavior.
pub fn main_with(extensions: Extensions) -> std::result::Result<(), Box<dyn std::error::Error>> {
	main_inner(extensions)
}
fn main_inner(extensions: Extensions) -> Result<()> {
	#[cfg(feature = "test-support")]
	let _config_reads = config::ReadReport;
	let mut args = match env::command_line(std::env::args_os()) {
		Ok(args) => args,
		Err(message) => {
			eprintln!("{}", format::terminal_safe(&message));
			terminal::exit(2);
		}
	};
	let mut mode = None;
	let mut splash = true;
	loop {
		if args.first().is_some_and(|s| s == "--no-splash") {
			splash = false;
		} else if let Some(flag) = args.first().and_then(|a| a.strip_prefix("--color=")) {
			mode = Some(match flag {
				"auto" => presentation::Mode::Auto,
				"always" => presentation::Mode::Always,
				"never" => presentation::Mode::Never,
				_ => {
					eprintln!(
						"rnx: --color takes auto, always, or never, not `{}`",
						format::terminal_safe(flag)
					);
					terminal::exit(2);
				}
			});
		} else {
			break;
		}
		args.remove(0);
	}

	// Answered before a context exists, because neither needs one and the
	// context is three quarters of what a trivial command costs: record 0030
	// measured 4.2 ms for `version` against 0.56 ms for a binary that exits
	// at once, and all of the difference was `with_default_modules`. Only a
	// command that touches no Rune at all belongs above the context; anything
	// else goes below, where `context` is in scope.

	#[cfg(feature = "project-sources")]
	if args.first().is_some_and(|s| s == "project-source-version") {
		if args.len() != 1 {
			return Err("project-source-version takes no arguments".into());
		}
		println!("{{\"format\":1}}");
		return Ok(());
	}
	if args
		.first()
		.is_some_and(|s| s == "version" || s == "--version" || s == "-V")
	{
		// Both halves, because either alone leaves a question open: which rnx
		// this is, and which Rune it embeds. Record 0001 pins an exact upstream
		// version and asks each release to say which one; this is where a
		// person or a script reads it without unpacking the binary.
		println!("rnx {VERSION}");
		println!("rune {RUNE_VERSION}");
		return Ok(());
	}
	if args
		.first()
		.is_some_and(|s| s == "help" || s == "--help" || s == "-h")
	{
		// An explicit question deserves an answer rather than an error.
		presentation::initialize(mode.unwrap_or(presentation::Mode::Auto));
		println!("{}", presentation::help(USAGE));
		return Ok(());
	}
	// Take and seal inherited endpoints before constructing a session/context.
	let worker_transport = if args.first().is_some_and(|s| s == "worker") {
		mode = Some(presentation::Mode::Never);
		match worker::Transport::from_args(&args) {
			Ok(t) => Some(t),
			Err((code, message)) => {
				eprintln!("{message}");
				std::process::exit(code);
			}
		}
	} else {
		None
	};

	let settings = if args.is_empty() || args.first().is_some_and(|s| s == "repl") {
		config::load()
	} else {
		config::Settings::default()
	};
	presentation::set_palette(settings.palette);
	presentation::initialize(mode.or(settings.mode).unwrap_or(presentation::Mode::Auto));
	splash = splash && settings.splash.unwrap_or(true);
	// HTTP uses the same lifecycle as extensions, even in the stock executable.
	// Keep activation after version/help and pure settings evaluation.
	let lifecycle = lifecycle::Lifecycle::new(true)?;
	let result = serve(args, extensions, worker_transport, splash, &lifecycle);
	lifecycle.close()?;
	result
}
fn serve(
	args: Vec<String>,
	extensions: Extensions,
	worker_transport: Option<worker::Transport>,
	splash: bool,
	lifecycle: &lifecycle::Lifecycle,
) -> Result<()> {
	let mut extensions = Some(extensions);
	let mut context = Context::with_default_modules()?;
	let mut host_functions = install_core(&mut context)?;
	host_functions.extend(fs::install(&mut context)?);
	host_functions.extend(path::install(&mut context)?);
	host_functions.extend(time::install(&mut context)?);
	host_functions.extend(text::install(&mut context)?);
	let http = http::State::default();
	host_functions.extend(http::install(&mut context, &http, lifecycle.scope("http"))?);
	// File arguments are selected below, after the existing run flags. All
	// other entry points have an empty script-argument snapshot.
	if !args.first().is_some_and(|arg| arg == "run") {
		host_functions.extend(env::install(&mut context, Arc::from([]))?);
	}
	let serves_without_run = args.is_empty()
		|| args
			.first()
			.is_some_and(|arg| matches!(arg.as_str(), "repl" | "eval" | "worker"));
	if serves_without_run {
		host_functions.extend(install_extensions(
			extensions.take().unwrap(),
			&mut context,
			lifecycle,
		));
	}
	if let Some(transport) = worker_transport {
		// Fatal transport errors are not script stderr or a successful cell.
		if worker::run(transport, context, http, lifecycle.clone()).is_err() {
			terminal::exit(1);
		}
		return Ok(());
	}

	// Answered before anything else, because it is not a Rune command at
	// all: it drives a pipe of its own and reports what a stop did to a
	// write blocked in the kernel.
	#[cfg(all(windows, feature = "test-support"))]
	if args.first().is_some_and(|s| s == pipe_control::COMMAND) {
		return pipe_control::run(args.get(1).map(String::as_str));
	}
	if args.first().is_some_and(|s| s == "eval") {
		let source = args.get(1).ok_or("eval needs source")?.clone();
		host::running_a_script();
		let mut session = session::Session::new(context)?
			.with_http(http.clone())
			.with_lifecycle(lifecycle.clone());
		match session.eval(&source) {
			// What a returned value means is decided in one place, so `eval`
			// and `run` cannot disagree about what a failure is. The renderer
			// stays each entry point's own; record 0018 decision 4 says why.
			Ok(value) => match runner::returned(&value) {
				Ok(value) => {
					if !runner::is_unit(&value) {
						// A shell entry point renders the whole value or reports
						// why it could not, in the same words `run` uses.
						match format::render_complete_styled(
							&value,
							Some(&session.fields()),
							presentation::stdout(),
						) {
							Ok(text) => println!("{text}"),
							Err(reason) => {
								eprintln!(
									"{}",
									presentation::error(&format!(
										"error: {}",
										runner::cannot_show(&reason)
									))
								);
								terminal::exit(1);
							}
						}
					}
				}
				Err(error) => {
					// The same reporter `run` uses. Bare means without quotes and
					// without a wrapper; it does not mean unescaped, and it does
					// not mean unbounded.
					runner::report_error(&error, Some(&session.fields()));
					terminal::exit(1);
				}
			},
			Err(failure) => {
				eprintln!("{}", failure.presented());
				// 130 is what a shell reports for a process Ctrl-C ended, so a
				// script around rnx reads an interrupted eval the same way.
				terminal::exit(if matches!(failure, session::Failure::Interrupted) {
					130
				} else {
					1
				});
			}
		}
		return Ok(());
	}
	// A session is what someone typing `rnx` almost always wants, and it works
	// whether standard input is a terminal or a pipe.
	if args.is_empty() || args.first().is_some_and(|s| s == "repl") {
		return repl::run(context, host_functions, http, splash, lifecycle.clone());
	}
	if args.first().is_some_and(|s| s == "run") {
		// Flags are read only before the script path. Everything after the
		// path is the script's, verbatim, so an argument that happens to read
		// like a flag reaches the script instead of changing rnx's behaviour.
		let mut rest = &args[1..];
		let mut debug_source = false;
		#[cfg(feature = "project-sources")]
		let mut source_map = None;
		let mut budget = runner::BUDGET;
		loop {
			#[cfg(feature = "project-sources")]
			if rest.first().is_some_and(|a| a == "--source-map") {
				if source_map.is_some() {
					return Err("run accepts only one --source-map".into());
				}
				source_map = Some(rest.get(1).ok_or("--source-map needs a file")?);
				rest = &rest[2..];
				continue;
			}
			if rest.first().is_some_and(|a| a == runner::DEBUG_SOURCE) {
				debug_source = true;
				rest = &rest[1..];
				continue;
			}
			if rest.first().is_some_and(|a| a == runner::BUDGET_FLAG) {
				// Refused here, before the file is read or compiled, so a bad
				// value cannot be mistaken for something the script did.
				let value = rest.get(1).ok_or_else(|| {
					format!("{} needs a count of instructions", runner::BUDGET_FLAG)
				})?;
				// A number too large for `usize` fails to parse, and
				// `usize::MAX` is outside the range on purpose: it is Rune's
				// sentinel for no budget at all, so accepting it would remove
				// the bound rather than raise it.
				budget = value
					.parse::<usize>()
					.ok()
					.filter(|n| (1..=runner::LARGEST_BUDGET).contains(n))
					.ok_or_else(|| {
						format!(
							"{} takes a whole number of instructions from 1 to {}, not `{value}`",
							runner::BUDGET_FLAG,
							runner::LARGEST_BUDGET
						)
					})?;
				rest = &rest[2..];
				continue;
			}
			break;
		}
		let path = rest.first().ok_or("run needs a file")?;
		#[cfg(feature = "project-sources")]
		let loader = if let Some(map) = source_map {
			program::Loader::mapped(std::path::Path::new(map), std::path::Path::new(path))
				.map_err(|e| format::terminal_safe(&e))?
		} else {
			program::Loader::new()
		};
		let _title = terminal::Title::new(&format!("rnx {path}"));
		let snapshot: Arc<[String]> = Arc::from(rest[1..].to_vec());
		let arguments = rune::to_value(snapshot.to_vec())?;
		env::install(&mut context, snapshot)?;
		install_extensions(extensions.take().unwrap(), &mut context, lifecycle);
		#[cfg(feature = "project-sources")]
		let code = runner::run_loaded(
			&context,
			path,
			arguments,
			debug_source,
			budget,
			lifecycle,
			loader,
		);
		#[cfg(not(feature = "project-sources"))]
		let code = runner::run(&context, path, arguments, debug_source, budget, lifecycle);
		terminal::exit(code);
	}
	// Anything that is not a command says so. Falling through to the
	// self-check is how a typo used to print a page of diagnostics and exit 0.
	if args.first().is_none_or(|s| s != "selfcheck") {
		// Escaped, because it is text from outside: record 0019's rule is that
		// everything rnx prints on its own behalf goes through here, and a
		// mistyped command is no exception — `rnx $'bad\033[2J'` cleared the
		// terminal before this did.
		eprintln!(
			"rnx: `{}` is not a command\n\n{USAGE}",
			format::terminal_safe(&args[0])
		);
		terminal::exit(2);
	}
	// The self-check: it asserts what it prints, and calls the process and
	// session checks, so a broken invariant fails here rather than being
	// reported. Record 0024 gave it a name; before that it was what `rnx`
	// did when it was given nothing to do.
	let state = call(
		&context,
		r#"
        struct Boxed { value }
        fn helper() { 10 }
        pub fn main(_) {
            let shared = [1];
            let closure = || helper() + shared[0];
            #{shared, closure, instance: Boxed {value: 7}, function: helper}
        }
    "#,
		Value::empty(),
	)?;
	let result = call(
		&context,
		r#"
        struct Boxed { value }
        fn helper() { 20 }
        impl Boxed { fn read(self) { self.value } }
        pub fn main(state) {
            let c = state.closure; let f = state.function;
            (c(), f(), helper(), state.instance.value,
             state.instance is Boxed, state.instance.read())
        }
    "#,
		state.clone(),
	)?;
	assert_eq!(json::stringify(&result)?, "[11,10,20,7,true,7]");
	println!(
		"retained closure/function, new function, old struct field/type/method: {}",
		json::stringify(&result)?
	);
	assert!(compile(&context, "pub fn main(s) { let x = ; }").is_err());
	let result = call(
		&context,
		"pub fn main(s) { let c = s.closure; c() }",
		state.clone(),
	)?;
	assert_eq!(json::stringify(&result)?, "11");
	println!("after compile failure: {result:?}");
	let failure = call(
		&context,
		"pub fn main(s) { s.shared.push(2); panic!(\"after mutation\"); }",
		state.clone(),
	);
	println!("runtime failure: {}", failure.unwrap_err());
	println!(
		"shared object after failed input: {}",
		json::stringify(&call(
			&context,
			"pub fn main(s) { s.shared }",
			state.clone()
		)?)?
	);
	for (name, source) in [
		(
			"changed struct shape",
			"struct Boxed { other, value } pub fn main(s) { (s.instance is Boxed, s.instance.value) }",
		),
		(
			"new field on old instance",
			"struct Boxed { other, value } pub fn main(s) { s.instance.other }",
		),
		(
			"old closure without declarations",
			"pub fn main(s) { s.shared[0] = 5; let c = s.closure; c() }",
		),
	] {
		match call(&context, source, state.clone()) {
			Ok(value) => println!("{name}: {}", json::stringify(&value)?),
			Err(e) => println!("{name}: {e}"),
		}
	}
	host::process_checks(&context)?;
	session::checks()?;
	Ok(())
}

fn install_extensions(
	extensions: Extensions,
	context: &mut Context,
	lifecycle: &lifecycle::Lifecycle,
) -> Vec<host::HostFunction> {
	match extensions.install_with(context, lifecycle) {
		Ok(functions) => functions,
		Err(message) => {
			eprintln!("error: {}", format::terminal_safe(&message));
			terminal::exit(1);
		}
	}
}

#[cfg(test)]
mod namespace_tests {
	#[test]
	fn core_inventory_has_domain_names_and_no_host_crate() {
		let mut context = rune::Context::with_default_modules().unwrap();
		let functions = super::install_core(&mut context).unwrap();
		let mut names: Vec<_> = functions.iter().map(|f| f.path.as_str()).collect();
		names.sort();
		let mut expected = vec![
			"json::parse",
			"json::stringify",
			"io::stdin",
			"io::eprint",
			"process::exit",
			"process::run",
			"process::run_bytes",
		];
		#[cfg(feature = "test-support")]
		expected.extend([
			"rnx_test::test_shutdown_task",
			"rnx_test::test_pending",
			"rnx_test::test_allocation_peak",
			"rnx_test::test_reset_allocation_peak",
		]);
		expected.sort();
		assert_eq!(names, expected);
		for name in names {
			assert!(
				super::compile(&context, &format!("pub fn main() {{ {name} }}")).is_ok(),
				"{name}"
			);
		}
		for name in [
			"json_parse",
			"json_stringify",
			"stdin",
			"eprint",
			"exit",
			"process",
			"process_bytes",
			"process_bytes_input",
			"test_pending",
			"test_allocation_peak",
			"test_reset_allocation_peak",
		] {
			assert!(
				super::compile(&context, &format!("pub fn main() {{ host::{name} }}")).is_err(),
				"host::{name} survived"
			);
		}
		#[cfg(not(feature = "test-support"))]
		assert!(super::compile(&context, "pub fn main() { rnx_test::test_pending }").is_err());
	}
}
