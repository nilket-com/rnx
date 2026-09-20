//! Record 0068: native presenters for top-level interactive results.
//!
//! A presenter is trusted Rust registered by an extension for one concrete
//! native type, matched by Rune's type hash. The generic renderer in
//! `format.rs` still invokes no VM protocol; the REPL and the notebook worker
//! consult this registry first, only for a top-level value, and fall back to
//! the renderer for everything else. Presenters write through [`Output`],
//! which enforces a byte budget and reserves room for its omission marker;
//! bounding the work behind each push remains the trusted callback's duty.
use rune::runtime::Value;
use std::collections::BTreeMap;

/// The bounded writer a presenter receives. It bounds what it accepts; a
/// trusted callback still owes the plan's bounded-work duty and must not
/// build an unbounded intermediate string before pushing.
pub struct Output {
	text: String,
	limit: usize,
	marker: &'static str,
	truncated: bool,
}
const OMITTED: &str = "\n[presentation byte limit; remainder omitted]\n";
const SHORT: &str = "…";
/// The largest omission marker that fits a budget, possibly none.
fn marker_for(limit: usize) -> &'static str {
	if limit >= OMITTED.len() {
		OMITTED
	} else if limit >= SHORT.len() {
		SHORT
	} else {
		""
	}
}
impl Output {
	/// A writer that never holds more than `limit` bytes, marker included.
	pub(crate) fn new(limit: usize) -> Self {
		Self {
			text: String::new(),
			limit,
			marker: marker_for(limit),
			truncated: false,
		}
	}
	/// Append one whole token. Returns `false` once the budget is exhausted;
	/// the marker is reserved so it always fits. A presenter should stop at
	/// the first `false`.
	pub fn push(&mut self, token: &str) -> bool {
		if self.truncated {
			return false;
		}
		if token.len() > self.limit - self.marker.len() - self.text.len() {
			self.truncated = true;
			self.text.push_str(self.marker);
			return false;
		}
		self.text.push_str(token);
		true
	}
	/// Whether the budget was reached.
	pub fn truncated(&self) -> bool {
		self.truncated
	}
	pub(crate) fn finish(self) -> String {
		self.text
	}
}

/// The final text a top-level result is shown as, escaped and bounded by
/// `budget` bytes: the presenter's text, or the opaque label with a bounded
/// note of why presentation failed. The REPL and the worker both use this,
/// so their guarantees cannot drift. Escaping can expand text, so the bound
/// is applied to the escaped bytes, and error text is neither read nor copied
/// beyond the budget.
pub(crate) fn finish(outcome: Result<String, String>, label: &str, budget: usize) -> String {
	let mut out = String::new();
	match outcome {
		Ok(text) => {
			let marker = marker_for(budget);
			let consumed =
				crate::format::terminal_safe_into(&text, budget - marker.len(), &mut out);
			if consumed < text.len() {
				out.push_str(marker);
			}
		}
		Err(error) => {
			let prefix = " (preview unavailable: ";
			let suffix = ")";
			let mut label_text = String::new();
			crate::format::terminal_safe_into(label, budget, &mut label_text);
			out.push_str(&label_text);
			let fixed = prefix.len() + suffix.len();
			if out.len() + fixed <= budget {
				out.push_str(prefix);
				let room = budget - out.len() - suffix.len();
				let marker = marker_for(room);
				let consumed =
					crate::format::terminal_safe_into(&error, room - marker.len(), &mut out);
				if consumed < error.len() {
					out.push_str(marker);
				}
				out.push_str(suffix);
			}
		}
	}
	debug_assert!(out.len() <= budget);
	out
}

type Presenter = Box<dyn Fn(&Value, &mut Output) -> Result<(), String>>;

/// Presenters keyed by native type hash, owned by one serving context.
#[derive(Default)]
pub struct Presenters {
	entries: BTreeMap<rune::Hash, (&'static str, Presenter)>,
	/// The extension currently registering, for diagnostics.
	owner: &'static str,
}
impl Presenters {
	/// Register `present` for the concrete native type `T`. A second
	/// registration for the same type is refused, whichever extension made it.
	pub fn register<T>(
		&mut self,
		present: impl Fn(&T, &mut Output) -> Result<(), String> + 'static,
	) -> Result<(), String>
	where
		T: rune::Any + rune::TypeHash + 'static,
	{
		let owner = self.owner;
		if let Some((previous, _)) = self.entries.get(&T::HASH) {
			return Err(format!(
				"presentation for `{}` was already registered by extension `{previous}`",
				std::any::type_name::<T>()
			));
		}
		let presenter: Presenter = Box::new(move |value, out| {
			let borrowed = value.borrow_ref::<T>().map_err(|e| e.to_string())?;
			present(&borrowed, out)
		});
		self.entries.insert(T::HASH, (owner, presenter));
		Ok(())
	}
	pub(crate) fn set_owner(&mut self, owner: &'static str) {
		self.owner = owner;
	}
	#[cfg(test)]
	pub(crate) fn len(&self) -> usize {
		self.entries.len()
	}
	/// Present a top-level value if its concrete type has a presenter. `None`
	/// means "not registered": the caller renders as before. `Some(Err)` means
	/// the presenter failed; the caller keeps the evaluation and reports the
	/// failure beside the opaque label.
	pub(crate) fn present(&self, value: &Value, limit: usize) -> Option<Result<String, String>> {
		let (_, presenter) = self.entries.get(&value.type_hash())?;
		let mut out = Output::new(limit);
		Some(match presenter(value, &mut out) {
			Ok(()) => Ok(out.finish()),
			Err(e) => Err(e),
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[derive(rune::Any)]
	#[rune(item = ::probe)]
	struct Cell(i64);
	#[derive(rune::Any)]
	#[rune(item = ::probe)]
	struct Other;

	#[test]
	fn presenters_match_type_identity_and_bound_output() {
		let mut p = Presenters::default();
		p.set_owner("probe");
		p.register::<Cell>(|c, out| {
			for _ in 0..1000 {
				if !out.push(&format!("cell {} ", c.0)) {
					break;
				}
			}
			Ok(())
		})
		.unwrap();
		assert!(
			p.register::<Cell>(|_, _| Ok(()))
				.unwrap_err()
				.contains("probe")
		);
		let cell = rune::to_value(Cell(7)).unwrap();
		let text = p.present(&cell, 200).unwrap().unwrap();
		assert!(text.starts_with("cell 7 cell 7 "));
		assert!(text.ends_with(OMITTED));
		assert!(text.len() <= 200);
		assert!(p.present(&rune::to_value(Other).unwrap(), 200).is_none());
		assert!(p.present(&rune::to_value(7i64).unwrap(), 200).is_none());
		assert!(
			p.present(&rune::to_value(String::from("x")).unwrap(), 200)
				.is_none()
		);
	}

	#[test]
	fn output_never_exceeds_its_limit_however_small() {
		for limit in 0..=OMITTED.len() + 4 {
			let mut out = Output::new(limit);
			while out.push("abc") {}
			let text = out.finish();
			assert!(
				text.len() <= limit,
				"limit {limit} produced {} bytes",
				text.len()
			);
			assert!(out_marker_ok(&text, limit));
		}
	}
	fn out_marker_ok(text: &str, limit: usize) -> bool {
		let marker = marker_for(limit);
		text.ends_with(marker)
	}

	#[test]
	fn finish_bounds_escaped_text_errors_and_labels() {
		// Expanding controls: 10,000 ESC bytes escape to 60,000; the final
		// text stays within the budget with the marker.
		let esc = "\u{1b}".repeat(10_000);
		let text = finish(Ok(esc.clone()), "<::x>", 16 * 1024);
		assert!(text.len() <= 16 * 1024);
		assert!(text.ends_with(OMITTED));
		assert!(text.starts_with("\\u{1b}"));
		assert!(!text.contains('\u{1b}'));
		// A long error is bounded too, with the label kept.
		let text = finish(Err(esc.clone()), "<::x>", 200);
		assert!(text.len() <= 200, "{}", text.len());
		assert!(text.starts_with("<::x> (preview unavailable: \\u{1b}"));
		assert!(text.ends_with(")"));
		assert!(!text.contains('\u{1b}'));
		// UTF-8 boundaries: multi-byte characters are never split.
		let crabs = "🦀".repeat(5_000);
		for budget in [0usize, 1, 2, 3, 4, 5, 7, 50, 51, 52, 1000] {
			for outcome in [Ok(crabs.clone()), Err(crabs.clone())] {
				let text = finish(outcome, "<::x>", budget);
				assert!(
					text.len() <= budget,
					"budget {budget} produced {}",
					text.len()
				);
				assert!(std::str::from_utf8(text.as_bytes()).is_ok());
			}
		}
		// A budget too small even for the label yields a prefix of it, never more.
		assert!(finish(Err("e".into()), "<::polars::DataFrame>", 5).len() <= 5);
		// Text that fits is untouched.
		assert_eq!(finish(Ok("ok\n".into()), "<::x>", 100), "ok\n");
	}
}

#[cfg(test)]
mod installation_tests {
	use super::*;
	use crate::Extensions;
	use std::sync::atomic::{AtomicUsize, Ordering};

	/// A host type with both formatting protocols instrumented.
	#[derive(rune::Any)]
	#[rune(item = ::frame)]
	struct Frame {
		rows: i64,
	}
	static DEBUG_RAN: AtomicUsize = AtomicUsize::new(0);
	static DISPLAY_RAN: AtomicUsize = AtomicUsize::new(0);
	static PRESENTED: AtomicUsize = AtomicUsize::new(0);
	impl Frame {
		#[rune::function(protocol = DEBUG_FMT)]
		fn debug_fmt(&self, f: &mut rune::runtime::Formatter) -> rune::runtime::VmResult<()> {
			use rune::alloc::fmt::TryWrite;
			DEBUG_RAN.fetch_add(1, Ordering::SeqCst);
			rune::vm_write!(f, "Frame?{}", self.rows)
		}
		#[rune::function(protocol = DISPLAY_FMT)]
		fn display_fmt(&self, f: &mut rune::runtime::Formatter) -> rune::runtime::VmResult<()> {
			use rune::alloc::fmt::TryWrite;
			DISPLAY_RAN.fetch_add(1, Ordering::SeqCst);
			rune::vm_write!(f, "Frame:{}", self.rows)
		}
	}
	fn build(m: &mut rune::Module) -> Result<Vec<(String, &'static str)>, String> {
		m.ty::<Frame>().map_err(|e| e.to_string())?;
		m.function("new", |rows: i64| Frame { rows })
			.build_associated::<Frame>()
			.map_err(|e| e.to_string())?;
		m.function_meta(Frame::debug_fmt)
			.map_err(|e| e.to_string())?;
		m.function_meta(Frame::display_fmt)
			.map_err(|e| e.to_string())?;
		Ok(vec![("frame::Frame::new".into(), "new(rows)")])
	}
	fn present(p: &mut Presenters) -> Result<(), String> {
		p.register::<Frame>(|frame, out| {
			PRESENTED.fetch_add(1, Ordering::SeqCst);
			out.push(&format!("Frame with {} rows\n", frame.rows));
			Ok(())
		})
	}
	fn session(extensions: Extensions) -> crate::session::Session {
		let mut context = rune::Context::with_default_modules().unwrap();
		crate::install_core(&mut context).unwrap();
		let lifecycle = crate::lifecycle::Lifecycle::new(true).unwrap();
		let installed = extensions.install_with(&mut context, &lifecycle).unwrap();
		let mut session = crate::session::Session::new(context)
			.unwrap()
			.with_lifecycle(lifecycle)
			.with_presenters(installed.presenters);
		session.set_budget(usize::MAX);
		session
	}

	#[test]
	fn a_presenter_needs_an_installed_extension_of_that_name() {
		let mut context = rune::Context::with_default_modules().unwrap();
		let lifecycle = crate::lifecycle::Lifecycle::new(true).unwrap();
		let error = Extensions::none()
			.present("frame", present)
			.install_with(&mut context, &lifecycle)
			.err()
			.expect("installation must fail");
		assert!(error.contains("presentation for `frame`"), "{error}");
		assert!(error.contains("no extension of that name"), "{error}");
		// Order does not matter: the builder may be appended after the presenter.
		let mut context = rune::Context::with_default_modules().unwrap();
		let installed = Extensions::none()
			.present("frame", present)
			.with("frame", build)
			.install_with(&mut context, &lifecycle)
			.unwrap();
		assert_eq!(installed.presenters.len(), 1);
	}

	#[test]
	fn a_second_registration_for_the_same_type_is_refused() {
		let mut context = rune::Context::with_default_modules().unwrap();
		let lifecycle = crate::lifecycle::Lifecycle::new(true).unwrap();
		let error = Extensions::none()
			.with("frame", build)
			.with("other", |m| {
				m.function("nothing", || 0i64)
					.build()
					.map_err(|e| e.to_string())?;
				Ok(vec![("other::nothing".into(), "nothing()")])
			})
			.present("frame", present)
			.present("other", present)
			.install_with(&mut context, &lifecycle)
			.err()
			.expect("installation must fail");
		assert!(error.contains("presentation for `other`"), "{error}");
		assert!(
			error.contains("already registered by extension `frame`"),
			"{error}"
		);
	}

	#[test]
	fn a_registrar_panic_is_an_installation_error() {
		let mut context = rune::Context::with_default_modules().unwrap();
		let lifecycle = crate::lifecycle::Lifecycle::new(true).unwrap();
		let error = Extensions::none()
			.with("frame", build)
			.present("frame", |_| panic!("registrar exploded"))
			.install_with(&mut context, &lifecycle)
			.err()
			.expect("installation must fail");
		assert!(error.contains("registrar exploded"), "{error}");
	}

	#[test]
	fn failure_only_and_panic_only_builders_still_compile_and_install_as_before() {
		let mut context = rune::Context::with_default_modules().unwrap();
		let lifecycle = crate::lifecycle::Lifecycle::new(true).unwrap();
		let error = Extensions::none()
			.with("fixture", |_| Err("registration failed".into()))
			.install_with(&mut context, &lifecycle)
			.err()
			.expect("installation must fail");
		assert!(error.contains("registration failed"), "{error}");
		let mut context = rune::Context::with_default_modules().unwrap();
		let error = Extensions::none()
			.with("fixture", |_| panic!("builder panicked"))
			.install_with(&mut context, &lifecycle)
			.err()
			.expect("installation must fail");
		assert!(error.contains("builder panicked"), "{error}");
	}

	#[test]
	fn a_top_level_result_is_presented_without_running_any_formatting_protocol() {
		let mut session = session(
			Extensions::none()
				.with("frame", build)
				.present("frame", present),
		);
		assert_eq!(session.presenter_count(), 1);
		let value = session.eval("frame::Frame::new(3)").unwrap();
		let text = session.present(&value, 16 * 1024).unwrap().unwrap();
		assert_eq!(text, "Frame with 3 rows\n");
		assert_eq!(PRESENTED.load(Ordering::SeqCst), 1);
		assert_eq!(
			DEBUG_RAN.load(Ordering::SeqCst),
			0,
			"DEBUG_FMT ran on the automatic path"
		);
		assert_eq!(
			DISPLAY_RAN.load(Ordering::SeqCst),
			0,
			"DISPLAY_FMT ran on the automatic path"
		);
		// The generic renderer is untouched and still opaque.
		assert_eq!(
			crate::format::render(&value, None, &crate::format::Limits::default()),
			"<::frame::Frame>"
		);
		// Unrelated values, and a frame inside a container, are not presented.
		let n = session.eval("42").unwrap();
		assert!(session.present(&n, 1024).is_none());
		let boxed = session.eval("[frame::Frame::new(1)]").unwrap();
		assert!(session.present(&boxed, 1024).is_none());
		assert_eq!(PRESENTED.load(Ordering::SeqCst), 1);
		// Explicit formatting is the only thing that runs the protocol.
		let text = session
			.eval("format!(\"{}\", frame::Frame::new(5))")
			.unwrap();
		assert_eq!(rune::from_value::<String>(text).unwrap(), "Frame:5");
		assert_eq!(DISPLAY_RAN.load(Ordering::SeqCst), 1);
		assert_eq!(DEBUG_RAN.load(Ordering::SeqCst), 0);
		// A reset keeps the presenter, as it keeps the extension.
		session.reset();
		assert_eq!(session.presenter_count(), 1);
		let value = session.eval("frame::Frame::new(9)").unwrap();
		assert_eq!(
			session.present(&value, 1024).unwrap().unwrap(),
			"Frame with 9 rows\n"
		);
	}

	#[test]
	fn a_presenter_failure_is_reported_beside_the_opaque_label_not_as_an_evaluation_error() {
		#[derive(rune::Any)]
		#[rune(item = ::broken)]
		struct Broken;
		let mut session = session(
			Extensions::none()
				.with("broken", |m| {
					m.ty::<Broken>().map_err(|e| e.to_string())?;
					m.function("make", || Broken)
						.build()
						.map_err(|e| e.to_string())?;
					Ok(vec![("broken::make".into(), "make()")])
				})
				.present("broken", |p| {
					p.register::<Broken>(|_, _| Err("no preview today".into()))
				}),
		);
		let value = session.eval("broken::make()").unwrap();
		assert_eq!(
			session.present(&value, 1024).unwrap().unwrap_err(),
			"no preview today"
		);
	}
}

#[cfg(test)]
mod failure_tests {
	use super::*;
	use crate::Extensions;

	#[derive(rune::Any)]
	#[rune(item = ::frame)]
	struct Frame {
		rows: i64,
	}
	fn build(m: &mut rune::Module) -> Result<Vec<(String, &'static str)>, String> {
		m.ty::<Frame>().map_err(|e| e.to_string())?;
		m.function("new", |rows: i64| Frame { rows })
			.build_associated::<Frame>()
			.map_err(|e| e.to_string())?;
		Ok(vec![("frame::Frame::new".into(), "new(rows)")])
	}
	fn session() -> crate::session::Session {
		let mut context = rune::Context::with_default_modules().unwrap();
		let lifecycle = crate::lifecycle::Lifecycle::new(true).unwrap();
		let installed = Extensions::none()
			.with("frame", build)
			.present("frame", |p| {
				p.register::<Frame>(|frame, out| {
					out.push(&format!("Frame with {} rows\n", frame.rows));
					Ok(())
				})
			})
			.install_with(&mut context, &lifecycle)
			.unwrap();
		let mut session = crate::session::Session::new(context)
			.unwrap()
			.with_lifecycle(lifecycle)
			.with_presenters(installed.presenters);
		session.set_budget(usize::MAX);
		session
	}

	/// A value that is mutably borrowed when presented cannot be borrowed
	/// shared: the presenter reports the failure, the caller falls back to
	/// the opaque label with a bounded reason, and the value stays intact.
	#[test]
	fn a_borrow_failure_is_reported_and_the_value_survives() {
		let mut session = session();
		let value = session.eval("frame::Frame::new(4)").unwrap();
		let held = value.borrow_mut::<Frame>().unwrap();
		let outcome = session.present(&value, 4096).expect("registered");
		let error = outcome.clone().unwrap_err();
		assert!(error.contains("Cannot read"), "{error}");
		let text = finish(outcome, "<::frame::Frame>", 4096);
		assert!(
			text.starts_with("<::frame::Frame> (preview unavailable: "),
			"{text}"
		);
		assert!(text.len() <= 4096);
		drop(held);
		assert_eq!(
			session.present(&value, 4096).unwrap().unwrap(),
			"Frame with 4 rows\n"
		);
	}

	/// Presenting is read-only and deterministic: the same value renders
	/// identically as many times as asked.
	#[test]
	fn repeated_presentation_is_identical() {
		let mut session = session();
		let value = session.eval("frame::Frame::new(7)").unwrap();
		let first = session.present(&value, 64).unwrap().unwrap();
		for _ in 0..100 {
			assert_eq!(session.present(&value, 64).unwrap().unwrap(), first);
		}
		assert_eq!(value.borrow_ref::<Frame>().unwrap().rows, 7);
	}
}

#[cfg(test)]
mod ownership_tests {
	use crate::Extensions;
	use std::sync::Arc;
	use std::sync::atomic::{AtomicUsize, Ordering};

	#[derive(rune::Any)]
	#[rune(item = ::owned)]
	struct Owned(i64);
	/// Dropped exactly when the registry that captured it is dropped.
	struct Sentinel(Arc<AtomicUsize>);
	impl Drop for Sentinel {
		fn drop(&mut self) {
			self.0.fetch_add(1, Ordering::SeqCst);
		}
	}
	fn build(m: &mut rune::Module) -> Result<Vec<(String, &'static str)>, String> {
		m.ty::<Owned>().map_err(|e| e.to_string())?;
		m.function("make", |n: i64| Owned(n))
			.build()
			.map_err(|e| e.to_string())?;
		Ok(vec![("owned::make".into(), "make(n)")])
	}
	fn session(extensions: Extensions) -> crate::session::Session {
		let mut context = rune::Context::with_default_modules().unwrap();
		crate::install_core(&mut context).unwrap();
		let lifecycle = crate::lifecycle::Lifecycle::new(true).unwrap();
		let installed = extensions.install_with(&mut context, &lifecycle).unwrap();
		let mut session = crate::session::Session::new(context)
			.unwrap()
			.with_lifecycle(lifecycle)
			.with_presenters(installed.presenters);
		session.set_budget(usize::MAX);
		session
	}

	#[test]
	fn reset_keeps_the_registry_and_retirement_drops_it_exactly_once() {
		let drops = Arc::new(AtomicUsize::new(0));
		let sentinel = Sentinel(drops.clone());
		let mut session = session(Extensions::none().with("owned", build).present(
			"owned",
			move |p| {
				let sentinel = sentinel;
				p.register::<Owned>(move |o, out| {
					let _keep = &sentinel;
					out.push(&format!("owned {}\n", o.0));
					Ok(())
				})
			},
		));
		let value = session.eval("owned::make(1)").unwrap();
		assert_eq!(session.present(&value, 64).unwrap().unwrap(), "owned 1\n");
		for _ in 0..3 {
			session.reset();
			assert_eq!(
				drops.load(Ordering::SeqCst),
				0,
				"reset must not drop the registry"
			);
			assert_eq!(session.presenter_count(), 1, "reset must not re-register");
			let value = session.eval("owned::make(2)").unwrap();
			assert_eq!(session.present(&value, 64).unwrap().unwrap(), "owned 2\n");
		}
		session.close().unwrap();
		assert_eq!(
			drops.load(Ordering::SeqCst),
			0,
			"close retires operations, not the context"
		);
		drop(session);
		assert_eq!(
			drops.load(Ordering::SeqCst),
			1,
			"dropping the session drops the registry once"
		);
	}

	#[test]
	fn a_lifecycle_builder_composes_with_presentation() {
		let session = session(
			Extensions::none()
				.with_lifecycle("owned", |m, _scope| build(m))
				.present("owned", |p| {
					p.register::<Owned>(|o, out| {
						out.push(&format!("lifecycle owned {}\n", o.0));
						Ok(())
					})
				}),
		);
		let mut session = session;
		let value = session.eval("owned::make(7)").unwrap();
		assert_eq!(
			session.present(&value, 64).unwrap().unwrap(),
			"lifecycle owned 7\n"
		);
	}

	#[test]
	fn settings_evaluation_sees_no_extension_and_no_presenter() {
		let presented = Arc::new(AtomicUsize::new(0));
		let counter = presented.clone();
		let mut session = session(Extensions::none().with("owned", build).present(
			"owned",
			move |p| {
				p.register::<Owned>(move |_, out| {
					counter.fetch_add(1, Ordering::SeqCst);
					out.push("x");
					Ok(())
				})
			},
		));
		let value = session.eval("owned::make(1)").unwrap();
		session.present(&value, 8);
		assert_eq!(presented.load(Ordering::SeqCst), 1);
		// The settings evaluator builds its own bare context: the extension
		// is a missing item there and no presenter can run.
		let error = crate::config::evaluate("owned::make(1)").err().unwrap();
		assert!(error.contains("owned"), "{error}");
		assert_eq!(presented.load(Ordering::SeqCst), 1);
	}
}
