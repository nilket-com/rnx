//! Reuse a validated derived map; public lock files are never repaired here.
use crate::input;
use std::{fs, io::Read, path::Path};

/// Invoke the existing atomic publisher only for missing or different maps.
pub(crate) fn ensure(
	path: &Path,
	bytes: &[u8],
	publish: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
	let read = || -> Result<Option<Vec<u8>>, String> {
		match fs::symlink_metadata(path) {
			Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
			Err(e) => return Err(e.to_string()),
			Ok(m) if !m.is_file() => return Err("not a regular file".into()),
			Ok(_) => (),
		}
		let mut options = fs::OpenOptions::new();
		options.read(true);
		#[cfg(unix)]
		{
			use std::os::unix::fs::OpenOptionsExt;
			options.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
		}
		let file = options.open(path).map_err(|e| e.to_string())?;
		if !file.metadata().map_err(|e| e.to_string())?.is_file() {
			return Err("not a regular file".into());
		}
		let mut current = Vec::new();
		file.take(input::DOCUMENT_LIMIT as u64 + 1)
			.read_to_end(&mut current)
			.map_err(|e| e.to_string())?;
		if current.len() > input::DOCUMENT_LIMIT {
			return Err(format!("exceeds {} bytes", input::DOCUMENT_LIMIT));
		}
		Ok(Some(current))
	};
	if read()
		.map_err(|e| format!("cannot reuse source map {}: {e}", path.display()))?
		.as_deref()
		== Some(bytes)
	{
		return Ok(());
	}
	publish()
}

#[cfg(test)]
mod tests;
