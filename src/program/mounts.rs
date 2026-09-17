//! Already-expanded logical prefixes. Manifest traversal belongs to the tool.
use rune::{Item, ast::Spanned, item::ComponentRef};
use std::path::PathBuf;

#[derive(Default)]
pub(super) struct Mounts(Vec<(Vec<String>, PathBuf)>);

impl Mounts {
	// Every handoff passes these structural limits.
	pub(super) fn new(entries: Vec<(Vec<String>, PathBuf)>) -> Result<Self, String> {
		if entries.len() > 256 {
			return Err("source map exceeds 256 mounts".into());
		}
		let mut entries = entries;
		for (prefix, root) in &entries {
			if prefix.is_empty() || prefix.len() > 16 {
				return Err("source prefix needs 1 to 16 components".into());
			}
			for part in prefix {
				let parsed = rune::parse::parse_all::<rune::ast::Ident>(
					part,
					rune::SourceId::empty(),
					false,
				);
				if !parsed.is_ok_and(|id| id.span().range() == (0..part.len()))
					|| matches!(part.as_str(), "self" | "super" | "crate" | "Self")
				{
					return Err(format!("invalid source alias `{part}`"));
				}
			}
			if !root.is_absolute() {
				return Err(format!("source root is not absolute: {}", root.display()));
			}
		}
		entries.sort_by(|a, b| a.0.cmp(&b.0));
		if entries.windows(2).any(|w| w[0].0 == w[1].0) {
			return Err("duplicate source prefix".into());
		}
		Ok(Self(entries))
	}

	pub(super) fn resolve(&self, item: &Item) -> Option<(PathBuf, bool)> {
		let (prefix, root) = self
			.0
			.iter()
			.filter(|(prefix, _)| {
				let mut components = item.iter();
				prefix
					.iter()
					.all(|p| matches!(components.next(), Some(ComponentRef::Str(s)) if s == p))
			})
			.max_by_key(|(prefix, _)| prefix.len())?;
		let mut path = root.clone();
		let mut at_root = true;
		for component in item.iter().skip(prefix.len()) {
			let ComponentRef::Str(s) = component else {
				return None;
			};
			path.push(s);
			at_root = false;
		}
		Some((path, at_root))
	}
}
