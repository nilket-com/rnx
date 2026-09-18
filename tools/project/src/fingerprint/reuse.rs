//! Invocation-local nested-root reuse, proved by record 0065 gate 4.
use super::*;
use std::collections::BTreeSet;
use std::os::unix::fs::MetadataExt;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Stamp {
	dev: u64,
	ino: u64,
	mode: u32,
	len: u64,
	sec: i64,
	nano: i64,
}
fn stamp(m: &fs::Metadata) -> Stamp {
	Stamp {
		dev: m.dev(),
		ino: m.ino(),
		mode: m.mode(),
		len: m.len(),
		sec: m.mtime(),
		nano: m.mtime_nsec(),
	}
}
fn observed(p: &Path) -> Result<Option<Stamp>, String> {
	match fs::symlink_metadata(p) {
		Ok(m) => Ok(Some(stamp(&m))),
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
		Err(e) => Err(fail(p, e)),
	}
}
#[derive(Default)]
struct State {
	files: BTreeMap<PathBuf, Stamp>,
	boundaries: BTreeMap<PathBuf, Option<Stamp>>,
}
thread_local! { static ACTIVE:std::cell::Cell<bool>=const {std::cell::Cell::new(false)}; static STATE: std::cell::RefCell<State> = std::cell::RefCell::new(State::default()); }
// Called by the bounded reader before opening each file, including all parent
// directory boundaries. A nested repository anywhere on a represented path
// makes reuse ineligible, even when independent scoped Git would ignore it.
pub(super) fn before_read(p: &Path) -> Result<(), String> {
	if !ACTIVE.with(|a| a.get()) {
		return Ok(());
	}
	STATE.with(|s| {
		let mut s = s.borrow_mut();
		for dir in p.parent().unwrap().ancestors() {
			// A directory's ancestors were observed on its first visit. Reuse
			// rechecks the complete boundary map before deriving any child.
			if s.boundaries.contains_key(dir) {
				break;
			}
			let directory = observed(dir)?;
			let git_path = dir.join(".git");
			let git_entry = observed(&git_path)?;
			let boundary = git_entry.is_some();
			s.boundaries.insert(dir.to_owned(), directory);
			s.boundaries.insert(git_path, git_entry);
			if boundary {
				break;
			}
		}
		Ok(())
	})
}
pub(super) fn after_read(p: &Path, m: &fs::Metadata) {
	if !ACTIVE.with(|a| a.get()) {
		return;
	}
	STATE.with(|s| {
		s.borrow_mut().files.insert(p.to_owned(), stamp(m));
	});
}
struct Observation {
	tree: Tree,
	top: PathBuf,
	gitdir: PathBuf,
	index: PathBuf,
	admin: Vec<(PathBuf, Option<Stamp>)>,
	files: BTreeMap<PathBuf, Stamp>,
	boundaries: BTreeMap<PathBuf, Option<Stamp>>,
	eligible: bool,
}
fn check(items: impl IntoIterator<Item = (PathBuf, Option<Stamp>)>) -> Result<(), String> {
	for (p, old) in items {
		if observed(&p)? != old {
			return Err(fail(&p, "observation changed before reuse"));
		}
	}
	Ok(())
}
fn independent(p: &Path, a: &mut Allowance, plain: bool) -> Result<Observation, String> {
	STATE.with(|s| *s.borrow_mut() = State::default());
	let info = std::cell::RefCell::new(None);
	let tree = native_using(p, a, |r, args, limit| {
		if plain || args[0] != "rev-parse" {
			return git(r, args, limit);
		}
		let Ok(bytes) = git(
			r,
			&[
				"rev-parse",
				"--path-format=absolute",
				"--show-toplevel",
				"--absolute-git-dir",
				"--git-path",
				"index",
				"--show-superproject-working-tree",
			],
			limit,
		) else {
			return git(r, args, limit);
		};
		let Ok(reply) = std::str::from_utf8(&bytes) else {
			return git(r, args, limit);
		};
		let lines = reply.lines().collect::<Vec<_>>();
		if !(3..=4).contains(&lines.len()) {
			return git(r, args, limit);
		}
		let top = PathBuf::from(lines[0]);
		let gd = PathBuf::from(lines[1]);
		let idx = PathBuf::from(lines[2]);
		let mut admin = vec![];
		for q in [
			gd.clone(),
			idx.clone(),
			gd.join("config"),
			gd.join("commondir"),
			gd.join("objects/info/alternates"),
		] {
			admin.push((q.clone(), observed(&q)?));
		}
		*info.borrow_mut() = Some((top, gd, idx, admin));
		Ok(if lines.len() == 4 {
			lines[3].as_bytes().to_vec()
		} else {
			vec![]
		})
	})?;
	let (top, gitdir, index, admin) = info.into_inner().unwrap_or_default();
	let eligible = !plain
		&& top.is_absolute()
		&& gitdir == top.join(".git")
		&& index == gitdir.join("index")
		&& observed(&gitdir)?.is_some_and(|s| s.mode & libc::S_IFMT == libc::S_IFDIR)
		&& !gitdir.join("commondir").exists();
	let (files, boundaries) = STATE.with(|s| {
		let s = s.take();
		(s.files, s.boundaries)
	});
	Ok(Observation {
		tree,
		top,
		gitdir,
		index,
		admin,
		files,
		boundaries,
		eligible,
	})
}
fn can_reuse(parent: &Observation, child: &Path) -> Result<bool, String> {
	if !parent.eligible || child == parent.tree.root || !child.starts_with(&parent.tree.root) {
		return Ok(false);
	}
	check(parent.admin.clone())?;
	check(
		parent
			.boundaries
			.iter()
			.map(|(p, s)| (p.clone(), s.clone())),
	)?;
	// Same discovery root follows from a checked enclosing --show-toplevel and
	// no intervening .git entry; oracle independently verifies child discovery.
	for dir in child.ancestors().take_while(|p| *p != parent.top) {
		if observed(&dir.join(".git"))?.is_some() {
			return Ok(false);
		}
	}
	// Inspect descendants too, including ignored directories: a nested repository
	// need not contain a file in the parent's index. This discovery is bounded;
	// exhaustion or unusual entries means independent fallback, never refusal.
	let mut pending = vec![child.to_path_buf()];
	let mut budget = 4096usize;
	while let Some(dir) = pending.pop() {
		if observed(&dir.join(".git"))?.is_some() {
			return Ok(false);
		}
		let Ok(entries) = fs::read_dir(&dir) else {
			return Ok(false);
		};
		for entry in entries {
			if budget == 0 {
				return Ok(false);
			}
			budget -= 1;
			let Ok(entry) = entry else { return Ok(false) };
			let Ok(ty) = entry.file_type() else {
				return Ok(false);
			};
			if ty.is_dir() {
				pending.push(entry.path());
			} else if !ty.is_file() {
				return Ok(false);
			}
		}
	}
	for (p, m) in &parent.boundaries {
		if p.starts_with(child) && p.file_name().is_some_and(|n| n == ".git") && m.is_some() {
			return Ok(false);
		}
	}
	Ok(true)
}
fn derive(parent: &Observation, child: &Path, a: &mut Allowance) -> Result<Tree, String> {
	check(parent.admin.clone())?;
	check(
		parent
			.boundaries
			.iter()
			.map(|(p, s)| (p.clone(), s.clone())),
	)?;
	let mut files = vec![];
	let prefix = child.strip_prefix(&parent.tree.root).unwrap();
	let selected = parent
		.tree
		.files
		.iter()
		.filter_map(|f| Path::new(&f.path).strip_prefix(prefix).ok().map(|r| (f, r)))
		.collect::<Vec<_>>();
	for _ in &selected {
		a.entry()?;
	}
	for (f, relative) in selected {
		let p = parent.tree.root.join(&f.path);
		let old = parent
			.files
			.get(&p)
			.ok_or("missing validated file observation")?;
		if observed(&p)?.as_ref() != Some(old) {
			return Err(fail(&p, "file observation changed before reuse"));
		}
		if f.bytes > a.bytes {
			return Err(fail(&p, "fingerprint exceeds byte allowance"));
		}
		a.charge_bytes(f.bytes)?;
		let mut f = f.clone();
		f.path = text(relative)?;
		files.push(f);
	}
	if files.is_empty() {
		return Err(fail(child, "native root has no tracked files"));
	}
	let mut tree = blake3::Hasher::new();
	tree.update(b"rnx-tree-v2\0");
	for f in &files {
		tree.update(&(f.path.len() as u64).to_be_bytes());
		tree.update(f.path.as_bytes());
		tree.update(&[u8::from(f.executable)]);
		tree.update(&f.bytes.to_be_bytes());
		let h = blake3::Hash::from_hex(&f.blake3).map_err(|e| e.to_string())?;
		tree.update(h.as_bytes());
	}
	#[cfg(feature = "test-support")]
	super::trace::event(
		serde_json::json!({"reuse":child,"parent":parent.tree.root,"top":parent.top,"gitdir":parent.gitdir,"index":parent.index}),
	);
	Ok(Tree {
		root: child.to_owned(),
		blake3: tree.finalize().to_hex().to_string(),
		files,
	})
}
pub(crate) fn many(roots: Vec<PathBuf>, a: &mut Allowance) -> Result<Vec<Tree>, String> {
	struct Active;
	impl Drop for Active {
		fn drop(&mut self) {
			ACTIVE.with(|a| a.set(false));
			STATE.with(|s| *s.borrow_mut() = State::default());
		}
	}
	let roots = roots.into_iter().collect::<BTreeSet<_>>();
	let nested = roots
		.iter()
		.any(|p| p.ancestors().skip(1).any(|a| roots.contains(a)));
	let _active = Active;
	if !nested {
		return roots
			.into_iter()
			.map(|p| {
				let tree = native(&p, a)?;
				#[cfg(feature = "test-support")]
				super::trace::between(&p)?;
				Ok(tree)
			})
			.collect();
	}
	ACTIVE.with(|a| a.set(true));
	let plain = std::env::vars_os().any(|(k, _)| k.to_string_lossy().starts_with("GIT_"));
	let mut observations = vec![];
	let mut trees = vec![];
	for p in roots {
		let p = root(&p)?;
		let mut reused = None;
		for parent in &observations {
			if can_reuse(parent, &p)? {
				reused = Some(derive(parent, &p, a)?);
				break;
			}
		}
		if let Some(tree) = reused {
			trees.push(tree);
		} else {
			let o = independent(&p, a, plain)?;
			trees.push(o.tree.clone());
			observations.push(o);
		}
		#[cfg(feature = "test-support")]
		super::trace::between(&p)?;
	}
	Ok(trees)
}
