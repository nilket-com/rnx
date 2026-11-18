//! File sources for `run`, with Rune 0.14.2's layout and one read allowance.
use rune::alloc::prelude::TryToOwned;
use rune::ast::Spanned;
use rune::compile::{self, SourceLoader};
use rune::{Item, ItemBuf, Source, SourceId, Sources};
use std::collections::{BTreeMap, VecDeque};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

pub const SOURCE_ALLOWANCE: usize = 8 * 1024 * 1024;

pub struct Text {
	pub path: PathBuf,
	pub text: String,
}

#[cfg(feature = "project-sources")]
mod mounts;
#[cfg(feature = "project-sources")]
mod source_map;
#[cfg(feature = "project-sources")]
use mounts::Mounts;

pub struct Loader {
	#[cfg(feature = "project-sources")]
	mounts: Mounts,
	pub texts: Vec<Text>,
	limit: usize,
	remaining: usize,
	exhausted: bool,
	// An in-memory entry has no directory, so `mod` declarations are refused.
	memory: bool,
	modules: Option<ModuleReads>,
	#[cfg(test)]
	opens: usize,
}

// Both compilers start at the same logical entry. Their assigned SourceIds
// and the interleaving of different module requests need not agree.
type ModuleRequest = (Option<PathBuf>, ItemBuf);

struct ModuleReads {
	entry_texts: usize,
	outcomes: BTreeMap<ModuleRequest, VecDeque<Result<usize, String>>>,
	replayed_texts: Option<Vec<usize>>,
}

impl Loader {
	pub fn new() -> Self {
		Self::with_limit(SOURCE_ALLOWANCE)
	}

	fn with_limit(limit: usize) -> Self {
		Self {
			#[cfg(feature = "project-sources")]
			mounts: Mounts::default(),
			texts: Vec::new(),
			limit,
			remaining: limit,
			exhausted: false,
			memory: false,
			modules: None,
			#[cfg(test)]
			opens: 0,
		}
	}

	// Only explicit run callers install a map; contexts never inherit it.
	#[cfg(feature = "project-sources")]
	fn with_mounts(mounts: Mounts) -> Self {
		Self {
			mounts,
			..Self::new()
		}
	}

	#[cfg(feature = "project-sources")]
	pub fn mapped(map: &Path, entry: &Path) -> Result<Self, String> {
		source_map::read(map, entry).map(Self::with_mounts)
	}

	fn limit_error(&self) -> io::Error {
		io::Error::other(format!(
			"the program's source allowance of {} bytes was exceeded",
			self.limit
		))
	}

	// Metadata is deliberately not used as a bound. Read at most one byte
	// beyond the remaining allowance, even if the file grows while reading.
	fn read(&mut self, reader: impl Read) -> io::Result<String> {
		if self.exhausted {
			return Err(self.limit_error());
		}
		let mut bytes = Vec::new();
		let result = reader
			.take(self.remaining as u64 + 1)
			.read_to_end(&mut bytes);
		if bytes.len() > self.remaining {
			self.exhausted = true;
			return Err(self.limit_error());
		}
		self.remaining -= bytes.len();
		result?;
		String::from_utf8(bytes).map_err(|_| {
			io::Error::new(
				io::ErrorKind::InvalidData,
				"stream did not contain valid UTF-8",
			)
		})
	}

	/// An entry supplied as text. `name` identifies it in diagnostics and is
	/// never opened; the text counts against the same allowance as a file.
	#[cfg(feature = "server-runtime")]
	pub fn memory(&mut self, name: &str, text: &str) -> Result<Source, String> {
		let text = self.read(text.as_bytes()).map_err(|e| e.to_string())?;
		let source = Source::with_path(name, &text, name).map_err(|e| e.to_string())?;
		self.memory = true;
		self.texts.push(Text {
			path: PathBuf::from(name),
			text,
		});
		Ok(source)
	}

	pub fn entry(&mut self, path: &Path) -> Result<Source, String> {
		self.file(path)
			.map_err(|error| format!("cannot read {}: {error}", path.display()))
	}

	pub(crate) fn record_module_reads(&mut self) {
		self.modules = Some(ModuleReads {
			entry_texts: self.texts.len(),
			outcomes: BTreeMap::new(),
			replayed_texts: None,
		});
	}

	pub(crate) fn replay_module_reads(&mut self) {
		if let Some(modules) = &mut self.modules {
			modules.replayed_texts = Some((0..modules.entry_texts).collect());
		}
	}

	pub(crate) fn loaded_texts(&self) -> impl Iterator<Item = &Text> {
		let replayed = self
			.modules
			.as_ref()
			.and_then(|modules| modules.replayed_texts.as_deref());
		(0..replayed.map_or(self.texts.len(), <[usize]>::len))
			.map(move |index| &self.texts[replayed.map_or(index, |texts| texts[index])])
	}

	fn file(&mut self, path: &Path) -> Result<Source, String> {
		if self.exhausted {
			return Err(self.limit_error().to_string());
		}
		#[cfg(test)]
		{
			self.opens += 1;
		}
		let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
		let text = self.read(file).map_err(|e| e.to_string())?;
		let source =
			Source::with_path(path.to_string_lossy(), &text, path).map_err(|e| e.to_string())?;
		self.texts.push(Text {
			path: path.to_owned(),
			text,
		});
		Ok(source)
	}
}

impl SourceLoader for Loader {
	fn load(
		&mut self,
		sources: &Sources,
		id: SourceId,
		item: &Item,
		span: &dyn Spanned,
	) -> compile::Result<Source> {
		let root = sources.get(id).and_then(Source::path);
		let Some(modules) = &mut self.modules else {
			return self
				.resolve(root, item)
				.map_err(|error| compile::Error::msg(span, error));
		};
		let request = (
			root.map(Path::to_owned),
			item.try_to_owned()
				.map_err(|error| compile::Error::msg(span, error))?,
		);
		if let Some(replayed) = &mut modules.replayed_texts
			&& let Some(outcome) = modules
				.outcomes
				.get_mut(&request)
				.and_then(VecDeque::pop_front)
		{
			let index = outcome.map_err(|error| compile::Error::msg(span, error))?;
			let text = &self.texts[index];
			let source = Source::with_path(text.path.to_string_lossy(), &text.text, &text.path)
				.map_err(|error| compile::Error::msg(span, error))?;
			replayed.push(index);
			return Ok(source);
		}
		let index = self.texts.len();
		let result = self.resolve(root, item);
		if let Some(modules) = &mut self.modules {
			if let Some(replayed) = &mut modules.replayed_texts {
				if result.is_ok() {
					replayed.push(index);
				}
			} else {
				// A duplicate declaration can read before its duplicate check.
				// Keep occurrences in order for this request alone.
				let outcome = result.as_ref().map(|_| index).map_err(Clone::clone);
				modules
					.outcomes
					.entry(request)
					.or_default()
					.push_back(outcome);
			}
		}
		result.map_err(|error| compile::Error::msg(span, error))
	}
}

impl Loader {
	fn resolve(&mut self, root: Option<&Path>, item: &Item) -> Result<Source, String> {
		// Do not even resolve further candidates once a read crossed the bound.
		if self.exhausted {
			return Err(self.limit_error().to_string());
		}
		if self.memory {
			return Err(format!(
				"module `{item}` cannot be loaded: an in-memory source has no directory"
			));
		}
		#[cfg(feature = "project-sources")]
		if let Some((base, at_root)) = self.mounts.resolve(item) {
			return self.candidate(&base, at_root);
		}
		// The fork passes the root source's id where 0.14.2 passed its path;
		// a root without a path was refused before the call there.
		let Some(root) = root else {
			return Err("Cannot load modules using a source without an associated URL".into());
		};
		let mut base = PathBuf::from(root);
		if !base.pop() {
			return Err(format!(
				"Cannot load modules relative to `{}`",
				root.display()
			));
		}
		for component in item {
			if let rune::item::ComponentRef::Str(name) = component {
				base.push(name);
			} else {
				return Err(format!("Cannot load module for `{item}`"));
			}
		}
		self.candidate(&base, false)
	}

	fn candidate(&mut self, base: &Path, at_mount: bool) -> Result<Source, String> {
		let candidates = [base.join("mod.rn"), base.with_extension("rn")];
		let candidates = if at_mount {
			&candidates[..1]
		} else {
			&candidates[..]
		};
		let Some(path) = candidates.iter().find(|path| path.is_file()) else {
			let expected = if at_mount {
				base.join("mod.rn")
			} else {
				base.with_extension("rn")
			};
			return Err(format!(
				"File not found, expected a module file like `{}`",
				expected.display()
			));
		};
		self.file(path)
			.map_err(|error| format!("Failed to load source at `{}`: {error}", path.display()))
	}
}

// Source text is private in Rune 0.14.2. Resolve the instruction's SourceId
// through Sources first, then find its retained read by the recorded path.
// Do not infer ids from load order: duplicate declarations can load a file
// without inserting it. If repeated reads disagree, attribution is ambiguous.
impl Loader {
	pub fn get(&self, sources: &Sources, id: SourceId) -> Option<&Text> {
		let path = sources.get(id)?.path()?;
		let mut matches = self.loaded_texts().filter(|text| text.path == path);
		let first = matches.next()?;
		matches
			.all(|other| other.text == first.text)
			.then_some(first)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rune::sync::Arc;
	use rune::{Context, Diagnostics, Vm};

	#[test]
	fn fallback_reuses_module_bytes_after_the_file_disappears() {
		use rune::alloc::prelude::TryClone as _;
		let root = std::env::temp_dir().join(format!("rnx-module-replay-{}", std::process::id()));
		std::fs::create_dir(&root).unwrap();
		let main = root.join("main.rn");
		let helper = root.join("helper.rn");
		std::fs::write(&main, "mod helper; pub fn main() { helper::value() }").unwrap();
		std::fs::write(&helper, r#"pub fn value() { let bytes = b"abc"; 42 }"#).unwrap();

		let mut loader = Loader::new();
		let entry = loader.entry(&main).unwrap();
		let mut fresh_sources = Sources::new();
		fresh_sources.insert(entry.try_clone().unwrap()).unwrap();
		let context = Context::with_default_modules().unwrap();
		loader.record_module_reads();
		let fresh = rune::experimental_core::frontend::compile_file_with_loader(
			&mut fresh_sources,
			&context,
			&mut Diagnostics::new(),
			1,
			&mut loader,
		);
		assert_eq!(
			fresh.err().unwrap().refusal,
			Some(rune::experimental_core::error::Refusal::ByteStr)
		);
		assert_eq!(loader.opens, 2);
		let remaining = loader.remaining;
		assert_eq!(
			remaining,
			SOURCE_ALLOWANCE
				- std::fs::read(&main).unwrap().len()
				- std::fs::read(&helper).unwrap().len()
		);
		std::fs::remove_file(&helper).unwrap();

		loader.replay_module_reads();
		let mut legacy_sources = Sources::new();
		legacy_sources.insert(entry).unwrap();
		let unit = rune::prepare(&mut legacy_sources)
			.with_context(&context)
			.with_source_loader(&mut loader)
			.build()
			.unwrap();
		let value = Vm::new(
			Arc::try_new(context.runtime().unwrap()).unwrap(),
			Arc::try_new(unit).unwrap(),
		)
		.call(["main"], ())
		.unwrap();
		assert_eq!(rune::from_value::<i64>(value).unwrap(), 42);
		assert_eq!(loader.opens, 2);
		assert_eq!(loader.remaining, remaining);
		assert_eq!(
			loader
				.loaded_texts()
				.map(|text| text.path.as_path())
				.collect::<Vec<_>>(),
			[&main, &helper]
		);
		std::fs::remove_dir_all(root).unwrap();
	}

	#[test]
	fn discovered_sources_share_the_frontend_count_name_and_byte_bounds() {
		use rune::experimental_core::{error::Refusal, frontend};

		struct Dependency(Source);
		impl SourceLoader for Dependency {
			fn load(
				&mut self,
				_: &Sources,
				_: SourceId,
				_: &Item,
				span: &dyn Spanned,
			) -> compile::Result<Source> {
				use rune::alloc::prelude::TryClone as _;
				self.0
					.try_clone()
					.map_err(|error| compile::Error::msg(span, error))
			}
		}

		let context = Context::with_default_modules().unwrap();
		for (entries, name, size, accepted) in [
			(255, 255, 0, true),
			(256, 1, 0, false),
			(1, 256, 0, false),
			(1, 1, 16 * 1024 * 1024, false),
		] {
			let mut inputs = Sources::new();
			inputs
				.insert(
					Source::with_path(
						"main.rn",
						"mod helper; pub fn main() { 42 }",
						"/tree/main.rn",
					)
					.unwrap(),
				)
				.unwrap();
			for _ in 1..entries {
				inputs.insert(Source::memory("").unwrap()).unwrap();
			}
			let mut dependency = Dependency(
				Source::with_path("n".repeat(name), " ".repeat(size), "/tree/helper.rn").unwrap(),
			);
			let compiled = frontend::compile_file_with_loader(
				&mut inputs,
				&context,
				&mut Diagnostics::new(),
				1,
				&mut dependency,
			);
			if accepted {
				let (program, _) = compiled.unwrap();
				assert_eq!(program.sources().len(), entries as usize + 1);
				assert_eq!(program.sources().last().unwrap().id, entries);
			} else {
				assert_eq!(compiled.err().unwrap().refusal, Some(Refusal::SourceLimit));
				assert!(inputs.get(SourceId::new(entries)).is_none());
			}
		}
	}

	#[test]
	fn a_failed_logical_lookup_replays_even_if_the_file_appears() {
		let root = std::env::temp_dir().join(format!("rnx-module-miss-{}", std::process::id()));
		std::fs::create_dir(&root).unwrap();
		let mut sources = Sources::new();
		let entry = sources
			.insert(Source::with_path("entry", "", root.join("main.rn")).unwrap())
			.unwrap();
		let item = ItemBuf::with_item(["helper"]).unwrap();
		let mut loader = Loader::new();
		loader.record_module_reads();
		let span = rune::ast::Span::empty();
		let old = loader.load(&sources, entry, &item, &span).err().unwrap();
		std::fs::write(root.join("helper.rn"), "pub fn value(){1}").unwrap();
		loader.replay_module_reads();
		let new = loader.load(&sources, entry, &item, &span).err().unwrap();
		assert_eq!(old.to_string(), new.to_string());
		assert_eq!(loader.opens, 0);
		assert_eq!(loader.remaining, SOURCE_ALLOWANCE);
		std::fs::remove_dir_all(root).unwrap();
	}

	fn fixture(name: &str) -> PathBuf {
		Path::new(env!("CARGO_MANIFEST_DIR"))
			.join("tests/fixtures/modules")
			.join(name)
			.join("main.rn")
	}

	type BuildResult = (
		Sources,
		Result<rune::Unit, String>,
		Vec<(usize, String, String)>,
	);

	fn build(path: &Path, loader: &mut dyn SourceLoader) -> BuildResult {
		let mut sources = Sources::new();
		sources.insert(Source::from_path(path).unwrap()).unwrap();
		let mut diagnostics = Diagnostics::new();
		let context = Context::with_default_modules().unwrap();
		let built = rune::prepare(&mut sources)
			.with_context(&context)
			.with_diagnostics(&mut diagnostics)
			.with_source_loader(loader)
			.build()
			.map_err(|e| e.to_string());
		let errors = diagnostics
			.diagnostics()
			.iter()
			.filter_map(|d| {
				if let rune::diagnostics::Diagnostic::Fatal(fatal) = d
					&& let Some(error) = fatal.as_compile_error()
				{
					return Some((
						error.span().range().start,
						sources.get(fatal.source_id()).unwrap().name().to_owned(),
						error.to_string(),
					));
				}
				None
			})
			.collect();
		(sources, built, errors)
	}

	#[test]
	fn every_layout_and_refusal_matches_the_pinned_loader() {
		for name in ["mixed", "misplaced", "missing", "duplicate"] {
			let entry = fixture(name);
			let mut loader = Loader::new();
			// build inserts its own entry; reserve the corresponding snapshot.
			loader.entry(&entry).unwrap();
			let (ours, built, errors) = build(&entry, &mut loader);
			let (theirs, reference, reference_errors) =
				build(&entry, &mut rune::compile::FileSourceLoader::new());
			assert_eq!(built.is_ok(), reference.is_ok(), "{name}");
			assert_eq!(errors, reference_errors, "{name}");
			for index in 0..=loader.texts.len() {
				let id = SourceId::try_from(index).unwrap();
				assert_eq!(
					ours.get(id).map(Source::path),
					theirs.get(id).map(Source::path),
					"{name} source {index}"
				);
				assert_eq!(loader.get(&ours, id).is_some(), ours.get(id).is_some());
			}
			if name == "mixed" {
				let context = Context::with_default_modules().unwrap();
				let mut vm = Vm::new(
					Arc::try_new(context.runtime().unwrap()).unwrap(),
					Arc::try_new(built.unwrap()).unwrap(),
				);
				assert_eq!(
					rune::from_value::<i64>(vm.call(["main"], ((),)).unwrap()).unwrap(),
					21111
				);
			}
		}
	}

	#[test]
	fn aggregate_exact_fit_runs_and_one_less_stops_further_opens() {
		let entry = fixture("mixed");
		let mut reference = Loader::new();
		reference.entry(&entry).unwrap();
		assert!(build(&entry, &mut reference).1.is_ok());
		let total = reference.texts.iter().map(|t| t.text.len()).sum::<usize>();
		let mut exact = Loader::with_limit(total);
		let mut sources = Sources::new();
		sources.insert(exact.entry(&entry).unwrap()).unwrap();
		let context = Context::with_default_modules().unwrap();
		let unit = rune::prepare(&mut sources)
			.with_context(&context)
			.with_source_loader(&mut exact)
			.build()
			.unwrap();
		let mut vm = Vm::new(
			Arc::try_new(context.runtime().unwrap()).unwrap(),
			Arc::try_new(unit).unwrap(),
		);
		assert_eq!(
			rune::from_value::<i64>(vm.call(["main"], ((),)).unwrap()).unwrap(),
			21111
		);
		assert_eq!(exact.remaining, 0);
		let crossing = exact.texts.last().unwrap().path.clone();
		let mut short = Loader::with_limit(total - 1);
		sources = Sources::new();
		sources.insert(short.entry(&entry).unwrap()).unwrap();
		let mut diagnostics = Diagnostics::new();
		assert!(
			rune::prepare(&mut sources)
				.with_context(&context)
				.with_diagnostics(&mut diagnostics)
				.with_source_loader(&mut short)
				.build()
				.is_err()
		);
		assert!(
			format!("{diagnostics:?}")
				.contains(&crossing.file_name().unwrap().to_string_lossy().to_string())
		);
		assert!(short.exhausted);
		let opens = short.opens;
		let item = rune::ItemBuf::with_item(["a"]).unwrap();
		assert!(
			short
				.load(
					&sources,
					SourceId::try_from(0usize).unwrap(),
					&item,
					&rune::ast::Span::empty()
				)
				.is_err()
		);
		assert!(short.entry(&entry).is_err());
		assert_eq!(short.opens, opens);
		// Exhaust the allowance on the first module while other declarations
		// remain queued in the compiler. None may cause another file open.
		let mut early = Loader::with_limit(std::fs::metadata(&entry).unwrap().len() as usize + 1);
		let mut queued_sources = Sources::new();
		queued_sources.insert(early.entry(&entry).unwrap()).unwrap();
		assert!(
			rune::prepare(&mut queued_sources)
				.with_context(&context)
				.with_source_loader(&mut early)
				.build()
				.is_err()
		);
		assert!(early.exhausted);
		assert_eq!(
			early.opens, 2,
			"only the entry and crossing module may open"
		);
	}

	#[test]
	fn understated_size_does_not_bypass_the_read_bound() {
		use std::io::{Seek, Write};
		let path = std::env::temp_dir().join(format!("rnx-growing-source-{}", std::process::id()));
		std::fs::write(&path, b" ").unwrap();
		let mut reader = std::fs::File::open(&path).unwrap();
		let reported_size = reader.metadata().unwrap().len();
		assert_eq!(reported_size, 1);
		// Deterministically grow the same opened file after the size check.
		std::fs::OpenOptions::new()
			.append(true)
			.open(&path)
			.unwrap()
			.write_all(&[b' '; 1000])
			.unwrap();
		let mut loader = Loader::with_limit(31);
		assert!(
			loader
				.read(&mut reader)
				.unwrap_err()
				.to_string()
				.contains("31 bytes")
		);
		assert_eq!(reader.stream_position().unwrap(), 32);
		assert!(loader.read(&mut reader).is_err());
		assert_eq!(reader.stream_position().unwrap(), 32);
		drop(reader);
		std::fs::remove_file(path).unwrap();
	}

	#[test]
	fn duplicate_snapshot_does_not_shift_source_ids() {
		let mut loader = Loader::new();
		let mut sources = Sources::new();
		let entry = fixture("mixed");
		sources.insert(loader.entry(&entry).unwrap()).unwrap();
		// A duplicate declaration loads successfully but Rune rejects it
		// before inserting. A following source must still map correctly.
		loader.entry(&entry).unwrap();
		let second_path = entry.parent().unwrap().join("a.rn");
		let second_id = sources.insert(loader.entry(&second_path).unwrap()).unwrap();
		assert_eq!(loader.get(&sources, second_id).unwrap().path, second_path);
		assert_eq!(
			loader.get(&sources, second_id).unwrap().text,
			std::fs::read_to_string(&second_path).unwrap()
		);
		loader.texts.push(Text {
			path: second_path,
			text: "different read".into(),
		});
		assert!(loader.get(&sources, second_id).is_none());
	}
}

#[cfg(all(test, feature = "project-sources"))]
mod project_tests;
