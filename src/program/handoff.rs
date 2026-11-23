//! The image and the legacy executable do not share a Rust allocator.
//! Transfer source text once and reference it by index in each lookup queue.
use super::*;
use rune::item::ComponentRef;
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;

#[derive(Deserialize, Serialize)]
enum Part {
	Name(String),
	Crate(String),
	Id(usize),
}

#[derive(Deserialize, Serialize)]
struct Lookup {
	parent: Option<Vec<u8>>,
	item: Vec<Part>,
	outcomes: VecDeque<Result<usize, String>>,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct Journal {
	texts: Vec<(Vec<u8>, String)>,
	lookups: Vec<Lookup>,
	mounts: Vec<(Vec<String>, PathBuf)>,
	limit: usize,
	remaining: usize,
	exhausted: bool,
}

impl Journal {
	#[cfg(any(test, feature = "engine-image"))]
	pub(crate) fn take(loader: Loader) -> Result<Self, String> {
		let reads = loader.modules.ok_or("file observation did not start")?;
		if reads.entry_texts != 1 || reads.replayed_texts.is_some() || loader.memory {
			return Err("expected a single observed file entry".into());
		}
		let lookups = reads
			.outcomes
			.into_iter()
			.map(|((parent, item), outcomes)| Lookup {
				parent: parent.map(|path| path.into_os_string().into_vec()),
				item: item
					.iter()
					.map(|part| match part {
						ComponentRef::Str(s) => Part::Name(s.into()),
						ComponentRef::Crate(s) => Part::Crate(s.into()),
						ComponentRef::Id(id) => Part::Id(id),
					})
					.collect(),
				outcomes,
			})
			.collect();
		Ok(Self {
			texts: loader
				.texts
				.into_iter()
				.map(|t| (t.path.into_os_string().into_vec(), t.text))
				.collect(),
			mounts: loader.mounts.into_entries(),
			limit: loader.limit,
			lookups,
			remaining: loader.remaining,
			exhausted: loader.exhausted,
		})
	}

	pub(crate) fn restore(self) -> Result<(Sources, Loader), String> {
		let mut loader = Loader::with_mounts(Mounts::new(self.mounts)?);
		if self.limit > SOURCE_ALLOWANCE || self.remaining > self.limit {
			return Err("invalid saved source allowance".into());
		}
		loader.limit = self.limit;
		let mut charged = self.remaining;
		for (path, text) in self.texts {
			charged = charged
				.checked_add(text.len())
				.filter(|&n| n <= self.limit)
				.ok_or("invalid saved source allowance")?;
			loader.texts.push(Text {
				path: PathBuf::from(OsString::from_vec(path)),
				text,
			});
		}
		let entry = loader.texts.first().ok_or("missing saved entry")?;
		let mut sources = Sources::new();
		let source = Source::with_path(entry.path.to_string_lossy(), &entry.text, &entry.path)
			.map_err(|e| e.to_string())?;
		sources.insert(source).map_err(|e| e.to_string())?;
		let mut outcomes = BTreeMap::new();
		for lookup in self.lookups {
			if lookup.outcomes.is_empty()
				|| lookup
					.outcomes
					.iter()
					.filter_map(|result| result.as_ref().ok())
					.any(|&index| index == 0 || index >= loader.texts.len())
			{
				return Err("invalid observed source index".into());
			}
			let mut item = ItemBuf::new();
			for part in &lookup.item {
				item.push(match part {
					Part::Name(s) => ComponentRef::Str(s),
					Part::Crate(s) => ComponentRef::Crate(s),
					Part::Id(id) => ComponentRef::Id(*id),
				})
				.map_err(|e| e.to_string())?;
			}
			let parent = lookup
				.parent
				.map(|bytes| PathBuf::from(OsString::from_vec(bytes)));
			if outcomes.insert((parent, item), lookup.outcomes).is_some() {
				return Err("duplicate observed source key".into());
			}
		}
		loader.remaining = self.remaining;
		loader.exhausted = self.exhausted;
		loader.modules = Some(ModuleReads {
			entry_texts: 1,
			outcomes,
			replayed_texts: None,
		});
		loader.replay_module_reads();
		Ok((sources, loader))
	}
}
