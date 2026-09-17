#![allow(dead_code)] // Staged input core: product lock/build/run arrive later.
use std::{io::Read, path::Path};
pub(crate) const MANIFEST_LIMIT: usize = 1024 * 1024;
pub(crate) const DOCUMENT_LIMIT: usize = 16 * 1024 * 1024;
pub(crate) fn read(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
	let run = || {
		let mut options = std::fs::OpenOptions::new();
		options.read(true);
		#[cfg(unix)]
		{
			use std::os::unix::fs::OpenOptionsExt;
			options.custom_flags(libc::O_NONBLOCK);
		}
		let file = options.open(path).map_err(|e| e.to_string())?;
		if !file.metadata().map_err(|e| e.to_string())?.is_file() {
			return Err("not a regular file".into());
		}
		let mut bytes = vec![];
		file.take(limit as u64 + 1)
			.read_to_end(&mut bytes)
			.map_err(|e| e.to_string())?;
		if bytes.len() > limit {
			return Err(format!("exceeds {limit} bytes"));
		}
		Ok(bytes)
	};
	run().map_err(|e| format!("cannot read {}: {e}", path.display()))
}
pub(crate) fn path(text: &str) -> Result<(), String> {
	if text.is_empty() || text.contains('\0') {
		return Err("path must be nonempty and contain no NUL".into());
	}
	Ok(())
}
pub(crate) fn identifier(text: &str) -> bool {
	use rune::ast::Spanned;
	rune::parse::parse_all::<rune::ast::Ident>(text, rune::SourceId::empty(), false)
		.is_ok_and(|id| id.span().range() == (0..text.len()))
		&& !matches!(text, "self" | "super" | "crate" | "Self")
}
pub(crate) fn reserved(text: &str) -> bool {
	matches!(
		text,
		"std"
			| "json" | "io"
			| "process"
			| "fs" | "path"
			| "time" | "text"
			| "http" | "env"
			| "rnx_test"
	)
}
