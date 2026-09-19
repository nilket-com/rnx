//! Describe Cargo's acquisition changes, never authenticate content before Cargo.
//! Only lock uses this observer. Verification paths never invoke acquisition.
use super::*;

struct Before {
	head: Option<String>,
	marker: Option<Marker>,
}
pub(crate) struct Observation(BTreeMap<PathBuf, Before>);
impl Observation {
	pub(crate) fn begin(home: &Path) -> Result<Self, String> {
		let base = home.join("git/checkouts");
		let mut found = BTreeMap::new();
		let mut allowance = fingerprint::ENTRIES;
		// Cargo owns this two-level layout. Observe existing directories only;
		// absent caches are normal. No URL hashing or checkout selection is ours.
		for repository in directories(&base, &mut allowance)? {
			for checkout in directories(&repository, &mut allowance)? {
				canonical_dir(&checkout)?;
				let head = git(&checkout, &["rev-parse", "HEAD"])
					.ok()
					.and_then(|b| String::from_utf8(b).ok())
					.map(|s| s.trim().to_owned());
				found.insert(
					checkout.clone(),
					Before {
						head,
						marker: marker(&checkout),
					},
				);
			}
		}
		Ok(Self(found))
	}
	pub(crate) fn report(&self, packages: &[GitPackage]) -> Result<(), String> {
		let mut seen = BTreeSet::new();
		for p in packages {
			let path = Path::new(&p.checkout);
			if !seen.insert(path) {
				continue;
			}
			let Some(before) = self.0.get(path) else {
				continue;
			};
			let after = marker(path);
			let head_changed = before.head.as_deref() != Some(p.revision.as_str());
			if (head_changed || before.marker.is_none())
				&& after.is_some()
				&& after != before.marker
			{
				eprintln!(
					"Cargo reacquired Git checkout {} at {} ({}); edits inside Cargo's checkout may have been discarded. Source verification follows before lock publication.",
					path.display(),
					p.revision,
					if head_changed {
						"HEAD differed or was unavailable"
					} else {
						"completion marker was missing"
					}
				);
			}
		}
		Ok(())
	}
}
#[derive(PartialEq)]
struct Marker {
	bytes: u64,
	modified: std::time::SystemTime,
	#[cfg(unix)]
	inode: u64,
}
fn marker(root: &Path) -> Option<Marker> {
	let m = fs::symlink_metadata(root.join(".cargo-ok")).ok()?;
	if !m.is_file() {
		return None;
	}
	#[cfg(unix)]
	use std::os::unix::fs::MetadataExt;
	Some(Marker {
		bytes: m.len(),
		modified: m.modified().ok()?,
		#[cfg(unix)]
		inode: m.ino(),
	})
}
fn directories(path: &Path, remaining: &mut usize) -> Result<Vec<PathBuf>, String> {
	let metadata = match fs::symlink_metadata(path) {
		Ok(m) => m,
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
		Err(e) => return Err(error(e)),
	};
	if !metadata.is_dir() {
		return Err(format!(
			"Cargo checkout directory is not regular: {}",
			path.display()
		));
	}
	let mut result = Vec::new();
	for entry in fs::read_dir(path).map_err(error)? {
		*remaining = remaining
			.checked_sub(1)
			.ok_or("Cargo checkout observation exceeds entry allowance")?;
		let entry = entry.map_err(error)?;
		if entry.file_type().map_err(error)?.is_dir() {
			result.push(entry.path());
		}
	}
	result.sort();
	Ok(result)
}
