//! File sources for `run`, with Rune 0.14.2's layout and one read allowance.
use rune::ast::Spanned;
use rune::compile::{self, SourceLoader};
use rune::{Item, Source, SourceId, Sources};
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
	#[cfg(test)]
	opens: usize,
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

	pub fn entry(&mut self, path: &Path) -> Result<Source, String> {
		self.file(path)
			.map_err(|error| format!("cannot read {}: {error}", path.display()))
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
	fn load(&mut self, root: &Path, item: &Item, span: &dyn Spanned) -> compile::Result<Source> {
		// Do not even resolve further candidates once a read crossed the bound.
		if self.exhausted {
			return Err(compile::Error::msg(span, self.limit_error()));
		}
		#[cfg(feature = "project-sources")]
		if let Some((base, at_root)) = self.mounts.resolve(item) {
			return self.candidate(&base, at_root, span);
		}
		let mut base = PathBuf::from(root);
		if !base.pop() {
			return Err(compile::Error::msg(
				span,
				format!("Cannot load modules relative to `{}`", root.display()),
			));
		}
		for component in item {
			if let rune::item::ComponentRef::Str(name) = component {
				base.push(name);
			} else {
				return Err(compile::Error::msg(
					span,
					format!("Cannot load module for `{item}`"),
				));
			}
		}
		self.candidate(&base, false, span)
	}
}

impl Loader {
	fn candidate(
		&mut self,
		base: &Path,
		at_mount: bool,
		span: &dyn Spanned,
	) -> compile::Result<Source> {
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
			return Err(compile::Error::msg(
				span,
				format!(
					"File not found, expected a module file like `{}`",
					expected.display()
				),
			));
		};
		self.file(path).map_err(|e| {
			compile::Error::msg(
				span,
				format!("Failed to load source at `{}`: {e}", path.display()),
			)
		})
	}
}

// Source text is private in Rune 0.14.2. Resolve the instruction's SourceId
// through Sources first, then find its retained read by the recorded path.
// Do not infer ids from load order: duplicate declarations can load a file
// without inserting it. If repeated reads disagree, attribution is ambiguous.
impl Loader {
	pub fn get(&self, sources: &Sources, id: SourceId) -> Option<&Text> {
		let path = sources.get(id)?.path()?;
		let mut matches = self.texts.iter().filter(|text| text.path == path);
		let first = matches.next()?;
		matches
			.all(|other| other.text == first.text)
			.then_some(first)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rune::{Context, Diagnostics, Vm};
	use std::sync::Arc;

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
					&& let rune::diagnostics::FatalDiagnosticKind::CompileError(error) =
						fatal.kind()
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
					Arc::new(context.runtime().unwrap()),
					Arc::new(built.unwrap()),
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
		let mut vm = Vm::new(Arc::new(context.runtime().unwrap()), Arc::new(unit));
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
				.load(&entry, &item, &rune::ast::Span::empty())
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
