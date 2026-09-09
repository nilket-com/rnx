use rune::runtime::Value;
use rune::{Context, Source, Sources, Vm};
use std::sync::Arc;
mod complete;
mod declared;
mod format;
mod host;
mod inspect;
mod json;
mod memory;
mod method;
mod platform;
mod repl;
mod runner;
mod session;
mod text;

// Installed for the whole process: the ceiling is enforced against what this
// counts. A build without the feature enforces no ceiling and says so.
#[cfg(feature = "count-allocations")]
#[global_allocator]
static ALLOCATOR: memory::Counting = memory::Counting;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

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

/// What rnx does, for someone who asked or who mistyped.
const USAGE: &str = "\
rnx — a Rune scripting environment

  rnx                       a session, the same as `rnx repl`
  rnx repl                  a session: line editing, history, `:help`
  rnx run [flags] FILE ...  run a file's `main`; arguments after FILE are its
  rnx eval SOURCE           evaluate one expression and exit
  rnx selfcheck             assert this build's own invariants and report
  rnx help                  this

Flags for `run`, before the file: --budget N, --debug-source.";

fn main() -> Result<()> {
	let mut context = Context::with_default_modules()?;
	let mut host_functions = host::install(&mut context)?;
	host_functions.extend(text::install(&mut context)?);
	let args: Vec<String> = std::env::args().skip(1).collect();
	if args.first().is_some_and(|s| s == "eval") {
		let source = args.get(1).ok_or("eval needs source")?.clone();
		host::running_a_script();
		let mut session = session::Session::new(context)?;
		match session.eval(&source) {
			// What a returned value means is decided in one place, so `eval`
			// and `run` cannot disagree about what a failure is. The renderer
			// stays each entry point's own; record 0018 decision 4 says why.
			Ok(value) => match runner::returned(&value) {
				Ok(value) => {
					if !runner::is_unit(&value) {
						// A shell entry point renders the whole value or reports
						// why it could not, in the same words `run` uses.
						match format::render_complete(&value, Some(&session.fields())) {
							Ok(text) => println!("{text}"),
							Err(reason) => {
								eprintln!("error: {}", runner::cannot_show(&reason));
								std::process::exit(1);
							}
						}
					}
				}
				Err(error) => {
					// The same reporter `run` uses. Bare means without quotes and
					// without a wrapper; it does not mean unescaped, and it does
					// not mean unbounded.
					runner::report_error(&error, Some(&session.fields()));
					std::process::exit(1);
				}
			},
			Err(failure) => {
				eprintln!("{failure}");
				std::process::exit(1);
			}
		}
		return Ok(());
	}
	// A session is what someone typing `rnx` almost always wants, and it works
	// whether standard input is a terminal or a pipe.
	if args.is_empty() || args.first().is_some_and(|s| s == "repl") {
		return repl::run(context, host_functions);
	}
	if args
		.first()
		.is_some_and(|s| s == "help" || s == "--help" || s == "-h")
	{
		// An explicit question deserves an answer rather than an error.
		println!("{USAGE}");
		return Ok(());
	}
	if args.first().is_some_and(|s| s == "run") {
		// Flags are read only before the script path. Everything after the
		// path is the script's, verbatim, so an argument that happens to read
		// like a flag reaches the script instead of changing rnx's behaviour.
		let mut rest = &args[1..];
		let mut debug_source = false;
		let mut budget = runner::BUDGET;
		loop {
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
		let arguments = serde_json::from_str(&serde_json::to_string(&rest[1..])?)?;
		let code = runner::run(&context, path, arguments, debug_source, budget);
		std::process::exit(code);
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
		std::process::exit(2);
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
