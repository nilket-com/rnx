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
pub(crate) fn diagnostic(line: fmt::Arguments<'_>) -> bool {
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
