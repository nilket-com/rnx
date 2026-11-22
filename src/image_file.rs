//! Internal file entry used only by the startup-linked engine image.
use std::{
	cell::RefCell,
	fmt::{self, Write},
	path::Path,
};

use crate::{engine::Options, lifecycle::Lifecycle, program::Loader, runner::execution};
use rune::{Context, ContextError, Sources};

pub enum Outcome {
	Complete,
	Refused,
	Failed,
}
pub struct Diagnostic {
	bytes: Vec<u8>,
	capacity: usize,
	needed: usize,
}
impl Diagnostic {
	pub fn bytes(&self) -> &[u8] {
		&self.bytes
	}
	pub fn needed(&self) -> usize {
		self.needed
	}
}
impl Write for Diagnostic {
	fn write_str(&mut self, s: &str) -> fmt::Result {
		self.needed = self.needed.saturating_add(s.len());
		let take = s.len().min(self.capacity - self.bytes.len());
		self.bytes.extend_from_slice(&s.as_bytes()[..take]);
		Ok(())
	}
}
thread_local! { static REPORT: RefCell<Option<Diagnostic>> = const { RefCell::new(None) }; }
thread_local! { static STOCK_STREAM: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
pub(crate) fn diagnostic(line: fmt::Arguments<'_>) -> bool {
	if STOCK_STREAM.get() {
		eprintln!("{line}");
		return true;
	}
	REPORT.with_borrow_mut(|slot| {
		if let Some(report) = slot {
			let _ = writeln!(report, "{line}");
			true
		} else {
			false
		}
	})
}
struct Restore;
impl Drop for Restore {
	fn drop(&mut self) {
		REPORT.take();
	}
}

pub fn run(
	path: &Path,
	native: bool,
	debug: bool,
	fuel: usize,
	context: fn() -> Result<Context, ContextError>,
	capacity: usize,
) -> Result<(Outcome, Diagnostic), &'static str> {
	if REPORT.with_borrow(Option::is_some) {
		return Err("reentrant file invocation");
	}
	REPORT.set(Some(Diagnostic {
		bytes: Vec::new(),
		capacity,
		needed: 0,
	}));
	let _restore = Restore;
	let invoke = || -> Result<Option<i32>, Box<dyn std::error::Error>> {
		let context = context()?;
		let mut loader = Loader::new();
		let mut sources = Sources::new();
		sources.insert(loader.entry(path)?)?;
		let args = rune::to_value(Vec::<String>::new())?;
		let lifecycle = Lifecycle::new(false)?;
		let result = execution::run(
			&context,
			&sources,
			&args,
			false,
			fuel,
			Options {
				native,
				debug,
				..Options::default()
			},
			&lifecycle,
			None,
			&mut loader,
		);
		lifecycle.close()?;
		Ok(result)
	};
	let outcome = match invoke() {
		Ok(None) => Outcome::Refused,
		Ok(Some(0)) => Outcome::Complete,
		Ok(Some(code)) => {
			if !REPORT.with_borrow(|r| r.as_ref().is_some_and(|v| v.needed > 0)) {
				diagnostic(format_args!("execution exited with status {code}"));
			}
			Outcome::Failed
		}
		Err(error) => {
			diagnostic(format_args!("{error}"));
			Outcome::Failed
		}
	};
	Ok((outcome, REPORT.take().ok_or("missing file diagnostic")?))
}

/// Open the checked file entry and retain its loader for any module reads.
pub fn checked_sources(path: &Path) -> Result<(Sources, impl rune::compile::SourceLoader), String> {
	let mut sources = Sources::new();
	let mut loader = crate::program::Loader::new();
	sources
		.insert(loader.entry(path)?)
		.map_err(|error| error.to_string())?;
	Ok((sources, loader))
}

/// Render a checked terminal result once, through the CLI's own source and
/// value formatters. REPORT can copy the retained diagnostic without effects.
pub fn checked_result(
	file: &rune::experimental_core::domain::file::FileRun,
	outcome: rune::experimental_core::domain::file::FileOutcome,
	budget: usize,
) -> (i32, Diagnostic) {
	REPORT.set(Some(Diagnostic {
		bytes: Vec::new(),
		capacity: 262_144,
		needed: 0,
	}));
	let _restore = Restore;
	let code = execution::checked::render_outcome(file, outcome, budget, false);
	(code, REPORT.take().expect("scoped image report"))
}

/// Product file entry for a dedicated CLI process. A refusal precedes script
/// effects. Once execution begins the returned code is terminal.
pub fn product_file(
	path: &str,
	args: Vec<String>,
	native: bool,
	debug: bool,
	debug_source: bool,
	fuel: usize,
	provider: Option<Vec<rune::Module>>,
) -> crate::Result<Option<i32>> {
	use crate::{host, presentation, runner, terminal};
	struct Stream;
	impl Drop for Stream {
		fn drop(&mut self) {
			STOCK_STREAM.set(false);
		}
	}
	STOCK_STREAM.set(true);
	let _stream = Stream;
	presentation::initialize(presentation::Mode::Auto);
	let lifecycle = Lifecycle::new(true)?;
	let result = (|| {
		let (mut context, _, _http) = crate::serving_context(&lifecycle, provider.is_none())?;
		let _title = terminal::Title::new(&format!("rnx {path}"));
		let arguments = crate::file_arguments(&mut context, args)?;
		#[cfg(feature = "image-effect-controls")]
		let paired = provider.is_some();
		let mut installer = rune::experimental_core::domain::file::Installer::new(&mut context);
		for module in provider.into_iter().flatten() {
			installer.install(module)?;
		}
		let modules = installer
			.finish()
			.map_err(|error| format!("file extensions: {:?}", error.kind))?;
		#[cfg(feature = "image-effect-controls")]
		if paired {
			let (mut stock, _, _http) = crate::serving_context(&lifecycle, true)?;
			crate::file_arguments(&mut stock, vec![])?;
			let expected = stock.image_stock_surface();
			let actual = context.image_stock_surface();
			if expected != actual {
				let first = expected
					.iter()
					.zip(&actual)
					.position(|(a, b)| a != b)
					.unwrap_or(expected.len().min(actual.len()));
				return Err(format!(
					"stock surface differs at {first}: {:?} vs {:?}",
					expected.get(first),
					actual.get(first)
				)
				.into());
			}
		}
		rune::experimental_core::domain::file::initialize_image_thread();
		host::running_a_script();
		let mut loader = Loader::new();
		let Some(sources) = runner::file_sources(&mut loader, path) else {
			return Ok(Some(1));
		};
		loader.record_module_reads();
		Ok(execution::run(
			&context,
			&sources,
			&arguments,
			debug_source,
			fuel,
			Options {
				native,
				debug,
				..Options::default()
			},
			&lifecycle,
			Some(&modules),
			&mut loader,
		))
	})();
	lifecycle.close()?;
	result
}
