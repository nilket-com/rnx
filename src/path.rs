//! Lexical helpers over Unicode strings, using the running platform: record 0037.
use crate::host::HostFunction;
use rune::{Context, Module};
use std::{ffi::OsStr, path::Path};

// Every input originates in &str. Path operations only remove components or
// append Unicode strings and ASCII punctuation; they cannot introduce invalid
// Unicode on Unix or unpaired surrogates on Windows. No lossy conversion.
fn text(path: &OsStr) -> String {
	path.to_str()
		.expect("path derived exclusively from Unicode strings")
		.to_owned()
}
fn join(base: &str, part: &str) -> String {
	text(Path::new(base).join(part).as_os_str())
}
fn parent(p: &str) -> Option<String> {
	Path::new(p).parent().map(|p| text(p.as_os_str()))
}
fn file_name(p: &str) -> Option<String> {
	Path::new(p).file_name().map(text)
}
fn file_stem(p: &str) -> Option<String> {
	Path::new(p).file_stem().map(text)
}
fn extension(p: &str) -> Option<String> {
	Path::new(p).extension().map(text)
}
fn with_extension(p: &str, ext: &str) -> Result<String, String> {
	// std panics on separators even when p has no filename. Check first.
	if ext.chars().any(std::path::is_separator) {
		return Err(format!(
			"cannot change extension of path {p:?}: extension {ext:?} contains a path separator"
		));
	}
	Ok(text(Path::new(p).with_extension(ext).as_os_str()))
}
fn is_absolute(p: &str) -> bool {
	Path::new(p).is_absolute()
}
fn separator() -> String {
	std::path::MAIN_SEPARATOR.to_string()
}
pub fn install(context: &mut Context) -> crate::Result<Vec<HostFunction>> {
	let mut module = Module::with_crate("path")?;
	module.function("join", join).build()?;
	module.function("parent", parent).build()?;
	module.function("file_name", file_name).build()?;
	module.function("file_stem", file_stem).build()?;
	module.function("extension", extension).build()?;
	module.function("with_extension", with_extension).build()?;
	module.function("is_absolute", is_absolute).build()?;
	module.function("separator", separator).build()?;
	context.install(module)?;
	Ok([
		("join", "join(base, part) -> String: lexical join; an absolute part replaces the base; Windows verbatim bases normalize . and .. for nonempty parts"),
		("parent", "parent(p) -> Option<String>: lexical parent; parent(\"a\") is Some(\"\"), parent(\"\") is None"),
		("file_name", "file_name(p) -> Option<String>: final name, ignoring trailing separators; .. has no name"),
		("file_stem", "file_stem(p) -> Option<String>: name without its last extension; a leading dot alone is not an extension"),
		("extension", "extension(p) -> Option<String>: last extension, without its dot; a trailing dot gives Some(\"\")"),
		("with_extension", "with_extension(p, ext) -> Result<String>: replace the last extension; empty removes only the last; separators in ext are refused"),
		("is_absolute", "is_absolute(p) -> bool: the running platform's rules; Windows requires a prefix and a root"),
		("separator", "separator() -> String: the running platform's main path separator"),
	].into_iter().map(|(name, doc)| HostFunction { path: format!("path::{name}"), doc }).collect())
}
#[cfg(test)]
mod tests {
	#[test]
	fn exact_surface_compiles() {
		let mut context = rune::Context::with_default_modules().unwrap();
		let names = super::install(&mut context).unwrap();
		assert_eq!(
			names.iter().map(|n| n.path.as_str()).collect::<Vec<_>>(),
			[
				"path::join",
				"path::parent",
				"path::file_name",
				"path::file_stem",
				"path::extension",
				"path::with_extension",
				"path::is_absolute",
				"path::separator"
			]
		);
		for name in names {
			crate::compile(&context, &format!("pub fn main() {{ {} }}", name.path)).unwrap();
		}
	}
}
