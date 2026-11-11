//! One-shot fresh evaluation. No persistent state or runtime fallback.
use super::*;
use rune::experimental_core::{
	execute::{Executor, PrepareError},
	frontend,
	heap::{FormatLimits, Store, ViewKind, ViewLimits},
	value::{Root, Scalar},
};

pub(crate) struct FreshValue {
	store: Store,
	root: Root,
}

impl FreshValue {
	/// Render the wrapper's result without converting it through stock Value.
	pub(crate) fn show(&self, fields: &crate::declared::Fields) -> std::result::Result<(), String> {
		let fault =
			|e: rune::experimental_core::error::Error| format!("{}: {:?}", e.operation, e.kind);
		let view = self
			.store
			.root_view(
				&self.root,
				ViewLimits {
					nodes: usize::MAX,
					depth: usize::MAX,
				},
			)
			.map_err(fault)?;
		let node = view.root().map_err(fault)?.child(1).map_err(fault)?;
		let returned = if let ViewKind::Result { ok } = node.kind().map_err(fault)? {
			let child = node.child(0).map_err(fault)?;
			if !ok {
				return Err(crate::format::execution::error_text(child, Some(fields))?);
			}
			child
		} else {
			node
		};
		if returned.kind().map_err(fault)? != ViewKind::Scalar(Scalar::Unit) {
			let text = crate::format::execution::render(
				returned,
				Some(fields),
				&crate::format::Limits::complete(),
				crate::presentation::stdout(),
			)?;
			println!("{text}");
		}
		Ok(())
	}
}

impl Drop for FreshValue {
	fn drop(&mut self) {
		let _ = self.store.close();
	}
}

impl Session {
	pub(super) fn try_fresh_eval(
		&self,
		generated: &Generated,
		options: crate::engine::Options,
	) -> Option<std::result::Result<FreshValue, Failure>> {
		let refused = |reason| {
			options.selected("legacy", Some(reason));
			None
		};
		if crate::engine::requires_loader(&generated.text) {
			return refused("SourceLoader");
		}
		let runtime_error = |message: String| Failure::Runtime {
			message,
			origin: None,
		};
		let mut sources = Sources::new();
		let source = match Source::memory(&generated.text) {
			Ok(s) => s,
			Err(e) => return Some(Err(runtime_error(e.to_string()))),
		};
		if let Err(e) = sources.insert(source) {
			return Some(Err(runtime_error(e.to_string())));
		}
		let program = match frontend::compile(
			&mut sources,
			&self.context,
			&mut rune::Diagnostics::new(),
			1,
		) {
			Ok(p) => p,
			Err(e) => {
				return match e.refusal {
					Some(r) => refused(r.name()),
					None => Some(Err(runtime_error(format!("{}: {:?}", e.operation, e.kind)))),
				};
			}
		};
		let Some(entry) = program.function(rune::Hash::type_hash(["main"])) else {
			return refused("Entry");
		};
		let setup = || -> std::result::Result<_, String> {
			let mut store = Store::new(Default::default()).map_err(|e| format!("{e:?}"))?;
			let state = store.root_record(None, &[]).map_err(|e| format!("{e:?}"))?;
			let mut executor = Executor::new().map_err(|e| format!("{e:?}"))?;
			executor.set_native_enabled(false);
			executor.set_format_limits(FormatLimits::permissive());
			executor
				.set_host_runtime(Some(
					self.context
						.execution_runtime()
						.map_err(|e| e.to_string())?,
				))
				.map_err(|e| format!("{e:?}"))?;
			Ok((store, state, executor))
		};
		let (mut store, state, mut executor) = match setup() {
			Ok(v) => v,
			Err(e) => return Some(Err(runtime_error(e))),
		};
		let args = [&state];
		let prepared = match executor.prepare(&program, &mut store, entry, &args) {
			Ok(p) => p,
			Err(PrepareError::Refused(r)) => return refused(r.name()),
			Err(PrepareError::Invalid(e)) => return Some(Err(runtime_error(format!("{e:?}")))),
		};
		if let Err(message) = self.lifecycle.begin() {
			return Some(Err(runtime_error(message)));
		}
		if options.debug {
			eprintln!(
				"rnx: engine=execution native-requested={} execution=sliced-interpreted reason=eval-native-slicing-deferred",
				if options.native { "on" } else { "off" }
			);
		}
		let outcome = crate::execute::slice_fresh(prepared, self.budget);
		let result = match outcome {
			crate::execute::FreshOutcome::Complete(root) => Ok(FreshValue { store, root }),
			crate::execute::FreshOutcome::Interrupted => Err(Failure::Interrupted),
			crate::execute::FreshOutcome::Budget => Err(Failure::Budget(self.budget)),
			crate::execute::FreshOutcome::Failed(error) => {
				let origin = error
					.error
					.location
					.and_then(|at| generated.map.locate(at.start as usize))
					.and_then(|(input, offset)| self.origin(input, offset));
				let mut message = crate::method::named_execution(&error, &program)
					.unwrap_or_else(|| error.message(&program).to_string());
				if error.remaining_work == Some(0) {
					message.push_str(&format!(
						"; the budget of {} instructions was exhausted at that point",
						self.budget
					));
				}
				Err(Failure::Runtime { message, origin })
			}
		};
		Some(result)
	}
}
