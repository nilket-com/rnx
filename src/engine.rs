//! One-shot engine policy. Selection finishes before any script effect.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum EngineChoice {
	#[default]
	Auto,
	Legacy,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Options {
	pub choice: EngineChoice,
	pub native: bool,
	pub debug: bool,
}

impl Default for Options {
	fn default() -> Self {
		Self {
			choice: EngineChoice::Auto,
			native: true,
			debug: false,
		}
	}
}

impl Options {
	pub fn selected(self, engine: &str, refusal: Option<&str>) {
		if self.debug {
			eprint!("rnx: engine={engine}");
			if engine == "execution" {
				eprint!(
					" native-requested={}",
					if self.native { "on" } else { "off" }
				);
			}
			if let Some(reason) = refusal {
				eprint!(" refusal={reason}");
			}
			eprintln!();
		}
	}
}

/// Module declarations are read from Rune tokens, never text patterns. Strings
/// and comments cannot select a loader. A declaration inside a macro is also
/// conservatively loader-dependent before any macro handler can run.
pub(crate) fn requires_loader(text: &str) -> bool {
	use rune::ast::Kind;
	let mut parser = rune::parse::Parser::new(text, rune::SourceId::EMPTY, true);
	let mut state = 0;
	while let Ok(token) = parser.parse::<rune::ast::Token>() {
		state = match (state, token.kind) {
			(0, Kind::Mod) => 1,
			(1, Kind::Ident(..)) => 2,
			(2, Kind::SemiColon) => return true,
			(_, Kind::Mod) => 1,
			_ => 0,
		};
	}
	false
}

/// Temporary admission boundary until Rune exposes re-exported entry functions
/// on Program. Delete this wrapper when the corresponding backlog item lands.
pub(crate) fn legacy_alias_entry_refusal(
	program: &rune::experimental_core::frontend::Program,
	text: &str,
) -> Option<rune::experimental_core::error::Refusal> {
	legacy_alias_entry_refusal_using(program, || {
		let Ok(file) =
			rune::parse::parse_all::<rune::ast::File>(text, rune::SourceId::new(0), false)
		else {
			return false;
		};
		file.items.iter().any(|(item, _)| {
			matches!(item, rune::ast::Item::Use(import) if legacy_import_exposes_main(&import.path, text))
		})
	})
}

fn legacy_alias_entry_refusal_using(
	program: &rune::experimental_core::frontend::Program,
	inspect_root_imports: impl FnOnce() -> bool,
) -> Option<rune::experimental_core::error::Refusal> {
	if program.function(rune::Hash::type_hash(["main"])).is_some() {
		return None;
	}
	inspect_root_imports().then_some(rune::experimental_core::error::Refusal::Import)
}

fn legacy_import_exposes_main(path: &rune::ast::ItemUsePath, text: &str) -> bool {
	use rune::ast::{ItemUseSegment, PathSegment, Spanned};
	if let Some((_, alias)) = &path.alias {
		return text.get(alias.span().range()) == Some("main");
	}
	let last = path.segments.last().map_or(&path.first, |(_, part)| part);
	match last {
		ItemUseSegment::PathSegment(PathSegment::Ident(name)) => {
			text.get(name.span().range()) == Some("main")
		}
		ItemUseSegment::Group(group) => group
			.iter()
			.any(|(path, _)| legacy_import_exposes_main(path, text)),
		ItemUseSegment::Wildcard(_) => true,
		_ => false,
	}
}

// Keep Rune's typed failure intact without adding an allocation to invocation.
#[allow(clippy::result_large_err)]
pub(crate) fn startup(
	context: &rune::Context,
	lifecycle: &crate::lifecycle::Lifecycle,
) -> Result<(), String> {
	use rune::experimental_core::{
		execute::Executor,
		frontend,
		heap::{FormatLimits, Limits, Store},
		value::Scalar,
	};
	let mut sources = rune::Sources::new();
	sources
		.insert(rune::Source::memory("pub fn main(){42}").map_err(|e| e.to_string())?)
		.map_err(|e| e.to_string())?;
	let program = frontend::compile(&mut sources, context, &mut rune::Diagnostics::new(), 1)
		.map_err(|e| format!("{}: {:?}", e.operation, e.kind))?;
	let mut store =
		Store::new(Limits::default()).map_err(|e| format!("{}: {:?}", e.operation, e.kind))?;
	let result = (|| {
		let mut executor = Executor::new().map_err(|e| format!("{}: {:?}", e.operation, e.kind))?;
		executor.set_native_enabled(false);
		executor.set_format_limits(FormatLimits::permissive());
		executor
			.set_host_runtime(Some(
				rune::sync::Arc::try_new(context.runtime().map_err(|e| e.to_string())?)
					.map_err(|e| e.to_string())?,
			))
			.map_err(|e| format!("{}: {:?}", e.operation, e.kind))?;
		let entry = program
			.function(rune::Hash::type_hash(["main"]))
			.ok_or("startup entry missing")?;
		let prepared = executor
			.prepare(&program, &mut store, entry, &[])
			.map_err(|e| format!("{e:?}"))?;
		let driver =
			crate::execute::Runtime::new().map_err(|e| format!("cannot start the runtime: {e}"))?;
		lifecycle.begin()?;
		let outcome = driver.run_sync(|| prepared.invoke(crate::runner::BUDGET));
		lifecycle.finish(outcome.is_err())?;
		drop(driver);
		let root = outcome.map_err(|e| e.message(&program).to_string())?;
		let matched = store
			.root_scalar(&root)
			.map_err(|e| format!("{}: {:?}", e.operation, e.kind))?
			== Some(Scalar::I64(42));
		store.root_release(root).map_err(|e| format!("{e:?}"))?;
		if matched {
			Ok(())
		} else {
			Err("startup probe did not return 42".into())
		}
	})();
	store
		.close()
		.map_err(|e| format!("{}: {:?}", e.operation, e.kind))?;
	result
}

#[cfg(test)]
mod tests {
	use super::*;
	fn compile(text: &str) -> (rune::experimental_core::frontend::Program, rune::Sources) {
		let context = rune::Context::with_default_modules().unwrap();
		let mut sources = rune::Sources::new();
		sources.insert(rune::Source::memory(text).unwrap()).unwrap();
		let program = rune::experimental_core::frontend::compile(
			&mut sources,
			&context,
			&mut rune::Diagnostics::new(),
			1,
		)
		.unwrap();
		(program, sources)
	}

	#[test]
	fn direct_entry_never_parses_imports_again() {
		let (program, _) = compile("pub fn main(args){7}");
		assert_eq!(
			legacy_alias_entry_refusal_using(&program, || panic!("direct entry parsed imports")),
			None
		);
	}

	#[test]
	fn missing_entry_without_root_entry_import_remains_terminal() {
		for source in [
			"pub fn work(args){7}",
			"mod inner{pub fn main(args){7}}",
			"// pub use inner::work as main;\npub fn work(args){\"use inner::*;\"}",
			"mod inner{pub fn work(args){7}}pub use inner::work as other;",
		] {
			let (program, _) = compile(source);
			assert_eq!(
				legacy_alias_entry_refusal(&program, source),
				None,
				"{source}"
			);
		}
	}

	#[test]
	fn root_entry_import_shapes_are_typed_refusals() {
		for source in [
			"mod inner{pub fn work(args){7}}pub use inner::work as main;",
			"mod inner{pub fn main(args){7}}pub use inner::main;",
			"mod inner{pub fn work(args){7}}pub use inner::{work as main};",
			"mod inner{pub fn main(args){7}}pub use inner::*;",
			"mod inner{pub mod nested{pub fn work(args){7}}}pub use inner::{nested::{work as main}};",
		] {
			let (program, _) = compile(source);
			assert_eq!(
				legacy_alias_entry_refusal(&program, source),
				Some(rune::experimental_core::error::Refusal::Import),
				"{source}"
			);
		}
	}

	#[test]
	fn loader_selection_uses_tokens_including_nested_modules() {
		assert!(requires_loader(
			"mod helper;pub fn main(args){helper::value()}"
		));
		assert!(requires_loader(
			"mod outer{mod /* nested */ inner;}pub fn main(args){}"
		));
		assert!(!requires_loader(
			"// mod false_positive;\npub fn main(args){\"mod ignored;\"}"
		));
		assert!(!requires_loader(
			"mod inline{pub fn value(){42}}pub fn main(args){inline::value()}"
		));
	}
}
