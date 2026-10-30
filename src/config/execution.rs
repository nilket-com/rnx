use super::*;
use rune::experimental_core::{
	execute::{Executor, PrepareError},
	frontend,
	heap::{FormatLimits, Limits, Store, ViewLimits},
};

pub(super) fn evaluate(
	context: &Context,
	wrapped: &str,
	options: crate::engine::Options,
) -> Option<Result<(Settings, Vec<String>), String>> {
	let mut sources = Sources::new();
	if let Err(error) =
		Source::memory(wrapped).and_then(|source| sources.insert(source).map(|_| ()))
	{
		return Some(Err(error.to_string()));
	}
	let program = match frontend::compile(&mut sources, context, &mut rune::Diagnostics::new(), 1)
		.and_then(|program| {
			if let Some(refusal) = crate::engine::legacy_alias_entry_refusal(&program, wrapped) {
				return Err(rune::experimental_core::error::Error::refused(refusal));
			}
			Ok(program)
		}) {
		Ok(program) => program,
		Err(error) if error.refusal.is_some() => {
			options.selected("legacy", error.refusal.map(|r| r.name()));
			return None;
		}
		Err(error) => {
			return Some(Err(format!(
				"cannot compile config: {}: {:?}",
				error.operation, error.kind
			)));
		}
	};
	let mut store = match Store::new(Limits::default()) {
		Ok(store) => store,
		Err(error) => return Some(Err(format!("{}: {:?}", error.operation, error.kind))),
	};
	let result = (|| {
		let mut executor =
			Executor::new().map_err(|error| format!("{}: {:?}", error.operation, error.kind))?;
		executor
			.set_host_runtime(Some(
				Arc::try_new(context.runtime().map_err(|e| e.to_string())?)
					.map_err(|e| e.to_string())?,
			))
			.map_err(|error| format!("{}: {:?}", error.operation, error.kind))?;
		executor.set_format_limits(FormatLimits::permissive());
		executor.set_native_enabled(options.native);
		let entry = program
			.function(rune::Hash::type_hash(["main"]))
			.ok_or("config entry missing")?;
		let prepared = match executor.prepare(&program, &mut store, entry, &[]) {
			Ok(prepared) => prepared,
			Err(PrepareError::Refused(refusal)) => {
				options.selected("legacy", Some(refusal.name()));
				return Ok(None);
			}
			Err(PrepareError::Invalid(error)) => {
				return Err(format!(
					"cannot evaluate config: {}",
					error.message(&program)
				));
			}
		};
		options.selected("execution", None);
		let root = prepared.invoke(BUDGET).map_err(|error| {
			let message =
				if error.error.kind == rune::experimental_core::error::ErrorKind::FuelExhausted {
					// rnx owns this template; the live dual-engine config test
					// catches linked legacy presentation drift.
					"Exhausted the budget it was given".to_owned()
				} else {
					error.message(&program).to_string()
				};
			format!("cannot evaluate config (budget {BUDGET} work units): {message}")
		})?;
		let settings = (|| {
			let view = store
				.root_view(
					&root,
					ViewLimits {
						nodes: usize::MAX,
						depth: usize::MAX,
					},
				)
				.map_err(|error| format!("{}: {:?}", error.operation, error.kind))?;
			settings_node(ConfigNode::Execution(
				view.root()
					.map_err(|error| format!("{}: {:?}", error.operation, error.kind))?,
			))
		})();
		store
			.root_release(root)
			.map_err(|error| format!("{error:?}"))?;
		settings.map(Some)
	})();
	if let Err(error) = store.close() {
		return Some(Err(format!("{}: {:?}", error.operation, error.kind)));
	}
	match result {
		Ok(None) => None,
		Ok(Some(settings)) => Some(Ok(settings)),
		Err(error) => Some(Err(error)),
	}
}
