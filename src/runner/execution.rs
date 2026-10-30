//! A complete one-shot execution attempt. Only typed pre-effect refusals return
//! `None` to the caller; every invoked outcome returns its terminal exit status.
use super::*;
use rune::alloc::prelude::TryClone;
use rune::experimental_core::{
	error::{ErrorKind, Location},
	execute::{Executor, Failure, PrepareError},
	frontend::{self, Program},
	heap::{FormatLimits, Limits as StoreLimits, Store, ViewKind, ViewLimits},
	value::{Root, Scalar, Type},
};

#[allow(clippy::too_many_arguments)]
pub(super) fn run(
	context: &Context,
	originals: &Sources,
	arguments: &Value,
	debug_source: bool,
	budget: usize,
	options: crate::engine::Options,
	lifecycle: &crate::lifecycle::Lifecycle,
	loader: &Loader,
) -> Option<i32> {
	let mut store = match Store::new(StoreLimits::default()) {
		Ok(store) => store,
		Err(error) => {
			unplaced("error", &format!("{}: {:?}", error.operation, error.kind));
			return Some(1);
		}
	};
	let result = attempt(
		context,
		originals,
		arguments,
		debug_source,
		budget,
		options,
		lifecycle,
		loader,
		&mut store,
	);
	if let Err(error) = store.close() {
		unplaced("error", &format!("{}: {:?}", error.operation, error.kind));
		return Some(1);
	}
	result
}

#[allow(clippy::too_many_arguments)]
fn attempt(
	context: &Context,
	originals: &Sources,
	arguments: &Value,
	debug_source: bool,
	budget: usize,
	options: crate::engine::Options,
	lifecycle: &crate::lifecycle::Lifecycle,
	loader: &Loader,
	store: &mut Store,
) -> Option<i32> {
	// Compilation may expand Sources. Keep the original loader cache and source
	// identities untouched so a refusal never rereads or double-charges files.
	let copy = || -> Result<Sources, String> {
		let mut sources = Sources::new();
		let mut index = 0u32;
		while let Some(original) = originals.get(rune::SourceId::new(index)) {
			sources
				.insert(original.try_clone().map_err(|e| e.to_string())?)
				.map_err(|e| e.to_string())?;
			index = index.checked_add(1).ok_or("source identity overflow")?;
		}
		Ok(sources)
	};
	let mut sources = match copy() {
		Ok(sources) => sources,
		Err(error) => {
			unplaced("error", &error);
			return Some(1);
		}
	};
	let program = match frontend::compile(&mut sources, context, &mut Diagnostics::new(), 1)
		.and_then(|program| {
			if let Some(refusal) =
				crate::engine::legacy_alias_entry_refusal(&program, &loader.texts[0].text)
			{
				return Err(rune::experimental_core::error::Error::refused(refusal));
			}
			Ok(program)
		}) {
		Ok(program) => program,
		Err(error) => {
			if let Some(refusal) = error.refusal {
				options.selected("legacy", Some(refusal.name()));
				return None;
			}
			options.selected("execution", None);
			unplaced("error", &format!("{}: {:?}", error.operation, error.kind));
			return Some(1);
		}
	};
	let Some(entry) = program.function(rune::Hash::type_hash(["main"])) else {
		options.selected("execution", None);
		let hash = rune::Hash::type_hash(["main"]);
		unlocated(
			"runtime error",
			&format!("Missing entry `main` with hash `{hash}`"),
		);
		return Some(1);
	};
	let setup = || -> Result<Executor, String> {
		let mut executor = Executor::new().map_err(|e| format!("{}: {:?}", e.operation, e.kind))?;
		let runtime = Arc::try_new(context.runtime().map_err(|e| e.to_string())?)
			.map_err(|e| e.to_string())?;
		executor
			.set_host_runtime(Some(runtime))
			.map_err(|e| format!("{}: {:?}", e.operation, e.kind))?;
		executor.set_format_limits(FormatLimits::permissive());
		executor.set_native_enabled(options.native);
		Ok(executor)
	};
	let mut executor = match setup() {
		Ok(executor) => executor,
		Err(error) => {
			unplaced("error", &error);
			return Some(1);
		}
	};
	let argv = match argv(store, arguments) {
		Ok(root) => root,
		Err(error) => {
			unplaced("error", &error);
			return Some(1);
		}
	};
	let code = invoke(
		&mut executor,
		&program,
		store,
		entry,
		&argv,
		debug_source,
		budget,
		options,
		lifecycle,
		loader,
	);
	if let Err(error) = store.root_release(argv) {
		unplaced("error", &format!("{:?}", error));
		return Some(1);
	}
	code
}

fn argv(store: &mut Store, arguments: &Value) -> Result<Root, String> {
	let strings = rune::from_value::<Vec<String>>(arguments.clone()).map_err(|e| e.to_string())?;
	let mut roots = Vec::with_capacity(strings.len());
	for string in strings {
		roots.push(
			store
				.root_string(&string)
				.map_err(|e| format!("{}: {:?}", e.operation, e.kind))?,
		);
	}
	let root = store
		.root_list(Type::Vec, &roots.iter().collect::<Vec<_>>())
		.map_err(|e| format!("{}: {:?}", e.operation, e.kind))?;
	for child in roots {
		store.root_release(child).map_err(|e| format!("{e:?}"))?;
	}
	Ok(root)
}

#[allow(clippy::too_many_arguments)]
// Keep Rune's typed failure intact without boxing the invocation result.
#[allow(clippy::result_large_err)]
fn invoke(
	executor: &mut Executor,
	program: &Program,
	store: &mut Store,
	entry: u32,
	argv: &Root,
	debug_source: bool,
	budget: usize,
	options: crate::engine::Options,
	lifecycle: &crate::lifecycle::Lifecycle,
	loader: &Loader,
) -> Option<i32> {
	let args = [argv];
	let prepared = match executor.prepare(program, store, entry, &args) {
		Ok(prepared) => prepared,
		Err(PrepareError::Refused(refusal)) => {
			options.selected("legacy", Some(refusal.name()));
			return None;
		}
		Err(PrepareError::Invalid(error)) => {
			options.selected("execution", None);
			located_error(
				"runtime error",
				program,
				error.location,
				&error.message(program).to_string(),
			);
			return Some(1);
		}
	};
	options.selected("execution", None);
	if debug_source {
		debug_sources(loader);
	}
	let driver = match crate::execute::Runtime::new() {
		Ok(driver) => driver,
		Err(error) => {
			unplaced("error", &format!("cannot start the runtime: {error}"));
			return Some(1);
		}
	};
	if let Err(error) = lifecycle.begin() {
		unplaced("error", &error);
		return Some(1);
	}
	let outcome = driver.run_sync(|| prepared.invoke(budget));
	if let Err(error) = lifecycle.finish(outcome.is_err()) {
		unplaced("error", &error);
		return Some(1);
	}
	drop(driver);
	let root = match outcome {
		Ok(root) => root,
		Err(error) => {
			report_failure(&error, program, budget);
			return Some(1);
		}
	};
	let mut fields = Fields::default();
	for source in &loader.texts {
		crate::declared::into_fields(&source.text, &mut fields);
	}
	let code = match show(store, &root, &fields, options.debug) {
		Ok(code) => code,
		Err(reason) => {
			unplaced("error", &cannot_show(&reason));
			1
		}
	};
	if let Err(error) = store.root_release(root) {
		unplaced("error", &format!("{error:?}"));
		return Some(1);
	}
	Some(code)
}

fn located_error(kind: &str, program: &Program, location: Option<Location>, message: &str) {
	match location.and_then(|location| {
		program
			.sources()
			.iter()
			.find(|s| s.id == location.source)
			.map(|s| (s, location.start as usize))
	}) {
		Some((source, offset)) => located(kind, &source.name, &source.text, offset, message),
		None => unlocated(kind, message),
	}
}

fn report_failure(error: &Failure, program: &Program, budget: usize) {
	if error.error.kind == ErrorKind::FuelExhausted {
		unplaced(
			"halted",
			&format!(
				"{budget} work units exceeded; {} N raises it",
				budget_advice()
			),
		);
		return;
	}
	let message = crate::method::named_execution(error, program)
		.unwrap_or_else(|| error.message(program).to_string());
	located_error("runtime error", program, error.error.location, &message);
	if error.remaining_work == Some(0) {
		unplaced(
			"halted",
			&format!(
				"the budget of {budget} work units was exhausted at that point; {} N raises it",
				budget_advice()
			),
		);
	}
}

fn show(store: &Store, root: &Root, fields: &Fields, debug: bool) -> Result<i32, String> {
	let view = store
		.root_view(
			root,
			ViewLimits {
				nodes: usize::MAX,
				depth: usize::MAX,
			},
		)
		.map_err(|e| format!("{}: {:?}", e.operation, e.kind))?;
	let node = view
		.root()
		.map_err(|e| format!("{}: {:?}", e.operation, e.kind))?;
	let kind = node
		.kind()
		.map_err(|e| format!("{}: {:?}", e.operation, e.kind))?;
	// A separate debug invocation supplies typed root evidence for the named
	// L1 unit-clone divergence. Normal script streams remain unchanged.
	if debug && kind == ViewKind::Scalar(Scalar::Unit) {
		eprintln!("rnx: result=Unit");
	}
	let returned = if let ViewKind::Result { ok } = kind {
		let child = node
			.child(0)
			.map_err(|e| format!("{}: {:?}", e.operation, e.kind))?;
		if !ok {
			unplaced(
				"error",
				&crate::format::execution::error_text(child, Some(fields))?,
			);
			return Ok(1);
		}
		child
	} else {
		node
	};
	if returned
		.kind()
		.map_err(|e| format!("{}: {:?}", e.operation, e.kind))?
		== ViewKind::Scalar(Scalar::Unit)
	{
		return Ok(0);
	}
	let text = crate::format::execution::render(
		returned,
		Some(fields),
		&crate::format::Limits::complete(),
		presentation::stdout(),
	)?;
	println!("{text}");
	Ok(0)
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::sync::{
		Arc as StdArc,
		atomic::{AtomicUsize, Ordering},
	};
	static NEXT: AtomicUsize = AtomicUsize::new(0);
	fn run_text(context: &Context, text: &str, options: crate::engine::Options) -> i32 {
		run_text_with_budget(context, text, options, 100_000)
	}
	fn run_text_with_budget(
		context: &Context,
		text: &str,
		options: crate::engine::Options,
		budget: usize,
	) -> i32 {
		let path = std::env::temp_dir().join(format!(
			"rnx-routing-effects-{}-{}.rn",
			std::process::id(),
			NEXT.fetch_add(1, Ordering::Relaxed)
		));
		std::fs::write(&path, text).unwrap();
		let lifecycle = crate::lifecycle::Lifecycle::new(false).unwrap();
		let code = super::super::run_loaded(
			context,
			path.to_str().unwrap(),
			rune::to_value(Vec::<String>::new()).unwrap(),
			false,
			budget,
			options,
			&lifecycle,
			Loader::new(),
		);
		lifecycle.close().unwrap();
		std::fs::remove_file(path).unwrap();
		code
	}
	#[test]
	fn refusal_and_terminal_errors_never_repeat_host_or_macro_effects() {
		let mut context = Context::with_default_modules().unwrap();
		let typed = StdArc::new(AtomicUsize::new(0));
		let undeclared = StdArc::new(AtomicUsize::new(0));
		let macros = StdArc::new(AtomicUsize::new(0));
		let mut module = rune::Module::with_crate("probe").unwrap();
		let count = typed.clone();
		module
			.host_function("touch", move || {
				count.fetch_add(1, Ordering::SeqCst);
			})
			.unwrap();
		let count = undeclared.clone();
		module
			.function("legacy", move || {
				count.fetch_add(1, Ordering::SeqCst);
				42i64
			})
			.build()
			.unwrap();
		let count = macros.clone();
		module
			.macro_(
				"once",
				move |_: &mut rune::macros::MacroContext<'_, '_, '_>,
				      input: &rune::macros::TokenStream| {
					count.fetch_add(1, Ordering::SeqCst);
					Ok(input.try_clone()?)
				},
			)
			.unwrap();
		context.install(module).unwrap();
		for native in [false, true] {
			let options = crate::engine::Options {
				native,
				..Default::default()
			};
			for (body, exit, legacy, macro_count) in [
				("probe::touch();42", 0, 0, 0),
				("probe::touch();panic!(\"terminal\")", 1, 0, 0),
				("probe::touch();probe::legacy()", 0, 1, 0),
				("probe::touch();probe::once!(42)", 0, 0, 1),
			] {
				typed.store(0, Ordering::SeqCst);
				undeclared.store(0, Ordering::SeqCst);
				macros.store(0, Ordering::SeqCst);
				assert_eq!(
					run_text(&context, &format!("pub fn main(args){{{body}}}"), options),
					exit
				);
				assert_eq!(typed.load(Ordering::SeqCst), 1, "{body}");
				assert_eq!(undeclared.load(Ordering::SeqCst), legacy, "{body}");
				assert_eq!(macros.load(Ordering::SeqCst), macro_count, "{body}");
			}
		}
	}

	#[test]
	fn routing_budget_preserves_two_way_ordered_mutable_host_effect_prefixes() {
		let mut context = Context::with_default_modules().unwrap();
		let effects = StdArc::new(std::sync::Mutex::new(Vec::<i64>::new()));
		let log = effects.clone();
		let mut module = rune::Module::with_crate("probe").unwrap();
		module
			.host_function("log", move |value: i64| {
				log.lock().unwrap().push(value);
			})
			.unwrap();
		context.install(module).unwrap();
		for body in [
			"let i=0;loop{probe::log(i);i+=1;}",
			"for i in 0..20{probe::log(i);}42",
			"for i in 0..20{probe::log(i);}panic!(\"terminal\")",
		] {
			let source = format!("pub fn main(args){{{body}}}");
			for native in [false, true] {
				for work in [12, 80, 200] {
					let mut observations = Vec::new();
					for (choice, budget) in [
						(crate::engine::EngineChoice::Legacy, work),
						(crate::engine::EngineChoice::Auto, work),
						(crate::engine::EngineChoice::Legacy, work * 64),
					] {
						effects.lock().unwrap().clear();
						let code = run_text_with_budget(
							&context,
							&source,
							crate::engine::Options {
								choice,
								native,
								debug: false,
							},
							budget,
						);
						observations.push((code, effects.lock().unwrap().clone()));
					}
					let [old, new, larger]: [_; 3] = observations.try_into().unwrap();
					assert!(
						new.1.starts_with(&old.1),
						"new execution made less progress: {body}, {work}, native={native}"
					);
					assert!(
						larger.1.starts_with(&new.1),
						"new effects exceed live larger legacy: {body}, {work}, native={native}"
					);
					if old.0 == 0 {
						assert_eq!(new, old, "new stopped after legacy completed");
					}
				}
			}
		}
	}
}
