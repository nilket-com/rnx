//! Expand parsed local source edges, without resolving versions or loading code.
// Gate 1 tests this internal core; manifest parsing is the next gate.
#![allow(dead_code)]
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub(crate) struct Package {
	pub root: PathBuf,
	/// Alias -> manifest path. The parser must resolve relative paths against
	/// the declaring manifest and validate identifiers before expansion.
	pub dependencies: BTreeMap<String, PathBuf>,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Mount {
	pub prefix: Vec<String>,
	pub root: PathBuf,
}

pub(crate) fn expand(
	entry: &Path,
	mut read: impl FnMut(&Path) -> Result<Package, String>,
) -> Result<Vec<Mount>, String> {
	struct State {
		packages: BTreeMap<PathBuf, Package>,
		ancestry: Vec<PathBuf>,
		mounts: Vec<Mount>,
	}
	fn visit(
		path: &Path,
		prefix: Vec<String>,
		state: &mut State,
		read: &mut impl FnMut(&Path) -> Result<Package, String>,
	) -> Result<(), String> {
		let edge = if prefix.is_empty() {
			"application".into()
		} else {
			prefix.join("::")
		};
		if prefix.len() > 16 {
			return Err(format!("source edge `{edge}` exceeds depth 16"));
		}
		if !prefix.is_empty() && state.mounts.len() == 256 {
			return Err(format!("source edge `{edge}` exceeds 256 mounts"));
		}
		let path = path
			.canonicalize()
			.map_err(|e| format!("source edge `{edge}` at {}: {e}", path.display()))?;
		if state.ancestry.contains(&path) {
			return Err(format!("source cycle at `{edge}`: {}", path.display()));
		}
		if !state.packages.contains_key(&path) {
			if state.packages.len() == 64 {
				return Err(format!("source edge `{edge}` exceeds 64 manifests"));
			}
			state.packages.insert(path.clone(), read(&path)?);
		}
		let package = &state.packages[&path];
		if !package.root.is_absolute() {
			return Err(format!("source edge `{edge}` has a non-absolute root"));
		}
		if !prefix.is_empty() {
			state.mounts.push(Mount {
				prefix: prefix.clone(),
				root: package.root.clone(),
			});
		}
		// Clone only after enforcing the parser's edge bound. The forthcoming
		// bounded parser cannot hand this core an arbitrarily sized manifest.
		if package.dependencies.len() > 256 {
			return Err(format!(
				"source edge `{edge}` declares more than 256 dependencies"
			));
		}
		let dependencies = package.dependencies.clone();
		state.ancestry.push(path);
		for (alias, child) in dependencies {
			let mut next = prefix.clone();
			next.push(alias);
			visit(&child, next, state, read)?;
		}
		state.ancestry.pop();
		Ok(())
	}
	let mut state = State {
		packages: BTreeMap::new(),
		ancestry: vec![],
		mounts: vec![],
	};
	visit(entry, vec![], &mut state, &mut read)?;
	Ok(state.mounts)
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::sync::atomic::{AtomicUsize, Ordering};
	struct Graph {
		root: PathBuf,
		edges: BTreeMap<usize, Vec<(String, usize)>>,
	}
	impl Graph {
		fn new() -> Self {
			static NEXT: AtomicUsize = AtomicUsize::new(0);
			let root = std::env::temp_dir().join(format!(
				"rnx-package-graph-{}-{}",
				std::process::id(),
				NEXT.fetch_add(1, Ordering::Relaxed)
			));
			std::fs::create_dir_all(&root).unwrap();
			Self {
				root: root.canonicalize().unwrap(),
				edges: BTreeMap::new(),
			}
		}
		fn node(&mut self, id: usize, edges: Vec<(String, usize)>) {
			std::fs::write(self.path(id), "fixture").unwrap();
			self.edges.insert(id, edges);
		}
		fn path(&self, id: usize) -> PathBuf {
			self.root.join(id.to_string())
		}
		fn run(&self) -> (Result<Vec<Mount>, String>, Vec<usize>) {
			let mut reads = vec![];
			let result = expand(&self.path(0), |p| {
				let id = p.file_name().unwrap().to_str().unwrap().parse().unwrap();
				reads.push(id);
				Ok(Package {
					root: self.root.join(format!("source{id}")),
					dependencies: self.edges[&id]
						.iter()
						.map(|(a, i)| (a.clone(), self.path(*i)))
						.collect(),
				})
			});
			(result, reads)
		}
	}
	impl Drop for Graph {
		fn drop(&mut self) {
			std::fs::remove_dir_all(&self.root).unwrap();
		}
	}
	#[test]
	fn diamond_expands_twice_but_reads_each_manifest_once() {
		let mut g = Graph::new();
		g.node(0, vec![("left".into(), 1), ("right".into(), 2)]);
		g.node(1, vec![("shared".into(), 3)]);
		g.node(2, vec![("shared".into(), 3)]);
		g.node(3, vec![]);
		let (result, reads) = g.run();
		let mounts = result.unwrap();
		assert_eq!(reads, [0, 1, 3, 2]);
		assert_eq!(mounts.len(), 4);
		assert_eq!(mounts[1].prefix, ["left", "shared"]);
		assert_eq!(mounts[3].prefix, ["right", "shared"]);
		assert_eq!(mounts[1].root, mounts[3].root);
	}
	#[test]
	fn cycle_names_edge_and_does_not_reread_ancestor() {
		let mut g = Graph::new();
		g.node(0, vec![("a".into(), 1)]);
		g.node(1, vec![("back".into(), 0)]);
		let (result, reads) = g.run();
		assert!(result.unwrap_err().contains("a::back"));
		assert_eq!(reads, [0, 1]);
	}
	#[test]
	fn lexical_back_edge_is_detected_by_canonical_identity() {
		let mut g = Graph::new();
		g.node(0, vec![]);
		std::fs::create_dir(g.root.join("sub")).unwrap();
		let mut reads = 0;
		let result = expand(&g.path(0), |_| {
			reads += 1;
			Ok(Package {
				root: g.root.clone(),
				dependencies: BTreeMap::from([("back".into(), g.root.join("sub/../0"))]),
			})
		});
		assert!(result.unwrap_err().contains("source cycle at `back`"));
		assert_eq!(reads, 1);
	}

	#[test]
	fn exactly_64_manifests_then_refuse_before_the_next_read() {
		let mut g = Graph::new();
		for i in 1..=64 {
			g.node(i, vec![]);
		}
		g.node(0, (1..64).map(|i| (format!("p{i:02}"), i)).collect());
		assert_eq!(g.run().0.unwrap().len(), 63);
		g.node(0, (1..=64).map(|i| (format!("p{i:02}"), i)).collect());
		let (r, reads) = g.run();
		assert!(r.unwrap_err().contains("p64"));
		assert_eq!(reads.len(), 64);
	}
	#[test]
	fn depth_16_passes_17_refuses_before_read() {
		let mut g = Graph::new();
		for i in 0..16 {
			g.node(i, vec![("next".into(), i + 1)]);
		}
		g.node(16, vec![]);
		assert_eq!(g.run().0.unwrap().len(), 16);
		g.node(16, vec![("over".into(), 17)]);
		g.node(17, vec![]);
		let (r, reads) = g.run();
		assert!(r.unwrap_err().contains("over` exceeds depth 16"));
		assert_eq!(reads.len(), 17);
	}
	#[test]
	fn mounts_256_can_share_one_manifest_and_257_refuses() {
		let mut g = Graph::new();
		g.node(1, vec![]);
		g.node(0, (0..256).map(|i| (format!("a{i:03}"), 1)).collect());
		assert_eq!(g.run().0.unwrap().len(), 256);
		// 255 leaves and one parent with a child crosses on expansion, not parsing.
		g.node(2, vec![("extra".into(), 1)]);
		g.edges.get_mut(&0).unwrap()[255] = ("z".into(), 2);
		assert!(
			g.run()
				.0
				.unwrap_err()
				.contains("z::extra` exceeds 256 mounts")
		);
	}
}
