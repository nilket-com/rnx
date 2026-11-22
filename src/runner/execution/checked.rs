//! Drive the sealed public file main with the file lifecycle and result policy.
use super::*;
use rune::experimental_core::domain::{
	StartError,
	file::{CheckedMain, FileOutcome},
};

#[allow(clippy::result_large_err)] // The file driver distinguishes typed preparation refusals.
pub(super) fn run(
	main: CheckedMain,
	arguments: &Value,
	debug_source: bool,
	budget: usize,
	options: crate::engine::Options,
	lifecycle: &crate::lifecycle::Lifecycle,
) -> Option<i32> {
	let argv = match rune::from_value::<Vec<String>>(arguments.clone()) {
		Ok(argv) => argv,
		Err(error) => {
			unplaced("error", &error.to_string());
			return Some(1);
		}
	};
	let mut file = match main.prepare(&argv, StoreLimits::default(), options.native) {
		Ok(file) => file,
		Err(error) => {
			options.selected("execution", None);
			unplaced("error", &format!("{}: {:?}", error.operation, error.kind));
			return Some(1);
		}
	};
	let driver = match crate::execute::Runtime::new() {
		Ok(driver) => driver,
		Err(error) => {
			unplaced("error", &error.to_string());
			return Some(1);
		}
	};
	let mut started = false;
	let mut setup_error = None;
	let outcome = driver.run_sync(|| {
		file.drive(
			budget,
			|sources| {
				options.selected("execution", None);
				if debug_source {
					for source in sources {
						debug_source_text(&source.name, &source.text);
					}
				}
				match lifecycle.begin() {
					Ok(()) => {
						started = true;
						true
					}
					Err(error) => {
						setup_error = Some(error);
						false
					}
				}
			},
			crate::host::interrupted,
		)
	});
	if started && let Err(error) = lifecycle.finish(!matches!(outcome, Ok(FileOutcome::Complete))) {
		unplaced("error", &error);
		return Some(1);
	}
	drop(driver);
	if let Some(error) = setup_error {
		unplaced("error", &error);
		return Some(1);
	}
	let code = match outcome {
		Err(StartError::Prepare(PrepareError::Refused(why))) => {
			options.refused(why, why.name(), None);
			None
		}
		Err(StartError::Prepare(PrepareError::Invalid(error))) => {
			options.selected("execution", None);
			unplaced("error", &format!("{}: {:?}", error.operation, error.kind));
			Some(1)
		}
		Err(StartError::Failed(failure)) => {
			options.selected("execution", None);
			unplaced("error", &format!("{:?}", failure.error.kind));
			Some(1)
		}
		Ok(outcome) => Some(render_outcome(&file, outcome, budget, options.debug)),
	};
	if let Err(error) = file.close() {
		unplaced("error", &format!("{}: {:?}", error.operation, error.kind));
		return Some(1);
	}
	code
}

pub(crate) fn render_outcome(
	file: &rune::experimental_core::domain::file::FileRun,
	outcome: FileOutcome,
	budget: usize,
	debug: bool,
) -> i32 {
	match outcome {
		FileOutcome::Interrupted => {
			eprintln!("interrupted");
			130
		}
		FileOutcome::Failed {
			error,
			message,
			remaining,
		} => {
			if error.kind == ErrorKind::FuelExhausted {
				unplaced(
					"halted",
					&format!(
						"{budget} work units exceeded; {} N raises it",
						budget_advice()
					),
				);
			} else {
				let source = error.location.and_then(|at| {
					file.sources()
						.iter()
						.find(|s| s.id == at.source && s.revision == at.revision)
						.map(|source| (source, at.start as usize))
				});
				match source {
					Some((s, offset)) => {
						located("runtime error", &s.name, &s.text, offset, &message)
					}
					None => unlocated("runtime error", &message),
				}
				if remaining == Some(0) {
					unplaced(
						"halted",
						&format!(
							"the budget of {budget} work units was exhausted at that point; {} N raises it",
							budget_advice()
						),
					);
				}
			}
			1
		}
		FileOutcome::Complete => {
			let mut fields = Fields::default();
			for source in file.sources() {
				crate::declared::into_fields(&source.text, &mut fields);
			}
			let shown = file
				.result(ViewLimits {
					nodes: usize::MAX,
					depth: usize::MAX,
				})
				.map_err(|e| format!("{}: {:?}", e.operation, e.kind))
				.and_then(|view| show_view(view, &fields, debug));
			shown.unwrap_or_else(|error| {
				unplaced("error", &cannot_show(&error));
				1
			})
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rune::experimental_core::domain::file::{FileModules, FileProgram, Installer};
	use rune::experimental_core::heap::ViewKind;
	use rune::experimental_core::value::Scalar;

	#[derive(rune::Any)]
	struct Count(i64);

	fn installed() -> (Context, FileModules) {
		let mut context = Context::with_default_modules().unwrap();
		let mut module = rune::Module::with_crate("fixture").unwrap();
		module.opaque_type::<Count>(|_| 8, |_, _| Ok(())).unwrap();
		module
			.opaque_method::<Count, _, _>("width", |v: &Count| v.0, None)
			.unwrap();
		module
			.file_opaque_result_function(
				"read",
				|_, _, _, _| panic!("unexpected legacy invocation"),
				|_, _| Ok(Ok(Count(-1))),
				|path, _| Ok(Ok(Count(path.len() as i64))),
			)
			.unwrap();
		module
			.opaque_result_function(
				"session_only",
				|_, _, _, _| panic!("legacy invoked"),
				|_, _| Ok(Ok(Count(-2))),
			)
			.unwrap();
		let mut installer = Installer::new(&mut context);
		installer.install(module).unwrap();
		let modules = installer.finish().unwrap();
		(context, modules)
	}

	fn compile(
		context: &Context,
		modules: &FileModules,
		text: &str,
	) -> rune::experimental_core::error::Result<FileProgram> {
		let mut sources = Sources::new();
		sources.insert(rune::Source::memory(text).unwrap()).unwrap();
		modules.compile(
			&mut sources,
			context,
			&mut Diagnostics::new(),
			1,
			&mut rune::compile::NoopSourceLoader::default(),
		)
	}

	#[test]
	fn file_producer_is_context_bound_and_executes_only_the_checked_main() {
		let (mut context, modules) = installed();
		let text = "pub fn main(args){let c=fixture::read(args[0],[])?; c.width()}";
		let FileProgram::Checked(main) = compile(&context, &modules, text).unwrap() else {
			panic!("file producer was not sealed")
		};
		let mut file = main
			.prepare(&["once".to_owned()], StoreLimits::default(), true)
			.unwrap();
		assert!(matches!(
			file.drive(5000, |_| true, || false).unwrap(),
			FileOutcome::Complete
		));
		assert_eq!(
			file.result(ViewLimits::default())
				.unwrap()
				.root()
				.unwrap()
				.kind()
				.unwrap(),
			ViewKind::Scalar(Scalar::I64(4))
		);
		assert!(
			file.drive(5000, |_| panic!("repeated invocation"), || false)
				.is_err()
		);
		file.close().unwrap();
		for bad in [
			"pub fn main(args){let a=fixture::read(args[0],[])?;let b=fixture::session_only(args[0],[])?;a.width()+b.width()}",
			"pub fn main(args){fixture::read(args[0],[])?}",
			"pub fn main(args){let c=fixture::read(args[0],[])?;return [c];}",
			"pub fn main(args){let c=fixture::read(args[0],[])?;||c}",
			"pub fn main(args){let c=fixture::read(args[0],[])?;pass(c)} fn pass(c){c}",
			"fn main(args){fixture::read(args[0],[])?;0}",
		] {
			assert!(compile(&context, &modules, bad).is_err(), "{bad}")
		}
		let mut sources = Sources::new();
		sources.insert(rune::Source::memory(text).unwrap()).unwrap();
		assert!(
			frontend::compile_file(&mut sources, &context, &mut Diagnostics::new(), 1).is_err()
		);
		let (other, _) = installed();
		assert!(compile(&other, &modules, text).is_err());
		context
			.install(rune::Module::with_crate("later").unwrap())
			.unwrap();
		assert!(compile(&context, &modules, text).is_err());
	}
	#[test]
	fn sealed_file_cap_failure_preserves_the_read_and_drops_the_payload() {
		use std::sync::{
			Arc,
			atomic::{AtomicUsize, Ordering::SeqCst},
		};

		#[derive(rune::Any)]
		struct Charged(Arc<AtomicUsize>);
		impl Drop for Charged {
			fn drop(&mut self) {
				self.0.fetch_add(1, SeqCst);
			}
		}

		let mut context = Context::with_default_modules().unwrap();
		let mut module = rune::Module::with_crate("charged").unwrap();
		module
			.opaque_type::<Charged>(|_| 65_536, |_, _| Ok(()))
			.unwrap();
		module
			.opaque_method::<Charged, _, _>("width", |_: &Charged| 2i64, None)
			.unwrap();
		let reads = Arc::new(AtomicUsize::new(0));
		let drops = Arc::new(AtomicUsize::new(0));
		let issued = reads.clone();
		let released = drops.clone();
		module
			.file_opaque_result_function(
				"read",
				|_, _, _, _| panic!("legacy executed"),
				|_, _| panic!("session reader executed"),
				move |_, _| {
					issued.fetch_add(1, SeqCst);
					Ok(Ok(Charged(released.clone())))
				},
			)
			.unwrap();
		let mut installer = Installer::new(&mut context);
		installer.install(module).unwrap();
		let modules = installer.finish().unwrap();
		let source = "pub fn main(args){let obj=charged::read(args[0],[])?;let alias=obj;alias.width()+obj.width()}";
		let run = |bytes| {
			let before = reads.load(SeqCst);
			let FileProgram::Checked(main) = compile(&context, &modules, source).unwrap() else {
				panic!("ordinary")
			};
			let mut file = main
				.prepare(
					&["p".to_owned()],
					StoreLimits {
						bytes,
						..StoreLimits::default()
					},
					true,
				)
				.unwrap();
			let fit = match file.drive(5000, |_| true, || false).unwrap() {
				FileOutcome::Complete => {
					assert_eq!(
						file.result(ViewLimits::default())
							.unwrap()
							.root()
							.unwrap()
							.kind()
							.unwrap(),
						ViewKind::Scalar(Scalar::I64(4))
					);
					true
				}
				FileOutcome::Failed {
					error, remaining, ..
				} => {
					assert_eq!(error.kind, ErrorKind::StoreCapacity);
					let loc = error.location.unwrap();
					assert_eq!(
						source[loc.start as usize..loc.end as usize].trim_end_matches('?'),
						"charged::read(args[0],[])"
					);
					assert!(remaining.unwrap() > 0);
					false
				}
				FileOutcome::Interrupted => panic!("unexpected interrupt"),
			};
			file.close().unwrap();
			assert_eq!(reads.load(SeqCst), before + 1);
			assert_eq!(drops.load(SeqCst), before + 1);
			fit
		};
		// This interval fits a single descriptor but cannot fit two. Find
		// the exact boundary including VM overhead, independently of layout.
		let (mut low, mut high) = (65_536, 69_632);
		assert!(!run(low) && run(high));
		while high - low > 1 {
			let mid = low + (high - low) / 2;
			if run(mid) {
				high = mid;
			} else {
				low = mid;
			}
		}
		assert!(run(high));
		assert!(!run(high - 1));
	}
}
