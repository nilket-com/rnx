//! Whole filesystem values and path-bearing refusals: record 0035.
use crate::host::HostFunction;
use rune::{
	Context, Module,
	runtime::{Bytes, Object, Value},
};
use std::{
	fs as disk,
	io::{Read, Write},
	path::{Path, PathBuf},
	time::{SystemTime, UNIX_EPOCH},
};

pub const READ_LIMIT: usize = 8 * 1024 * 1024;
const ENTRY_LIMIT: usize = 100_000;
type Result<T> = std::result::Result<T, String>;
fn about(verb: &str, path: &str, why: impl std::fmt::Display) -> String {
	format!("cannot {verb} {path}: {why}")
}
// Refusals use the word "directory"; metadata uses the stable value "dir".
fn kind(meta: &disk::Metadata) -> &'static str {
	if meta.is_file() {
		"file"
	} else if meta.is_dir() {
		"directory"
	} else {
		#[cfg(unix)]
		{
			use std::os::unix::fs::FileTypeExt;
			if meta.file_type().is_fifo() {
				return "FIFO";
			}
		}
		"special file"
	}
}
pub(crate) fn regular(path: impl AsRef<std::path::Path>) -> std::io::Result<disk::File> {
	let mut options = disk::OpenOptions::new();
	options.read(true);
	#[cfg(unix)]
	{
		use std::os::unix::fs::OpenOptionsExt;
		options.custom_flags(libc::O_NONBLOCK);
	}
	let file = options.open(path)?;
	let meta = file.metadata()?;
	if !meta.is_file() {
		return Err(std::io::Error::other(format!(
			"it is a {}, not a regular file",
			kind(&meta)
		)));
	}
	Ok(file)
}
fn contents(path: &str) -> Result<Vec<u8>> {
	let mut bytes = Vec::new();
	regular(path)
		.map_err(|e| about("read", path, e))?
		.take(READ_LIMIT as u64 + 1)
		.read_to_end(&mut bytes)
		.map_err(|e| about("read", path, e))?;
	if bytes.len() > READ_LIMIT {
		return Err(about("read", path, "it exceeds the 8 MiB limit"));
	}
	Ok(bytes)
}
pub(crate) fn read(path: &str) -> Result<String> {
	String::from_utf8(contents(path)?).map_err(|e| {
		about(
			"read",
			path,
			format!(
				"it is not UTF-8 at byte {}; use fs::read_bytes to read it",
				e.utf8_error().valid_up_to()
			),
		)
	})
}
fn read_bytes(path: &str) -> Result<Bytes> {
	Ok(Bytes::from_vec(
		rune::alloc::Vec::try_from(contents(path)?).map_err(|e| about("read", path, e))?,
	))
}
enum WriteMode {
	New,
	Replace,
	Append,
}
fn write_data(path: &str, data: Value, mode: WriteMode) -> Result<()> {
	// Validate and borrow before opening: a type error must not truncate a file.
	let write = |bytes: &[u8]| {
		let mut options = disk::OpenOptions::new();
		options.write(true);
		match mode {
			WriteMode::New => {
				options.create_new(true);
			}
			WriteMode::Replace => {
				options.create(true).truncate(true);
			}
			WriteMode::Append => {
				options.create(true).append(true);
			}
		}
		options
			.open(path)
			.and_then(|mut f| f.write_all(bytes))
			.map_err(|e| about("write", path, e))
	};
	if let Ok(text) = data.borrow_ref::<rune::alloc::String>() {
		write(text.as_bytes())
	} else if let Ok(bytes) = data.borrow_ref::<Bytes>() {
		write(bytes.as_slice())
	} else {
		Err(about("write", path, "data must be String or Bytes"))
	}
}
pub(crate) fn write_new(path: &str, data: Value) -> Result<()> {
	write_data(path, data, WriteMode::New)
}
fn write(path: &str, data: Value) -> Result<()> {
	write_data(path, data, WriteMode::Replace)
}
fn append(path: &str, data: Value) -> Result<()> {
	write_data(path, data, WriteMode::Append)
}
pub(crate) fn mkdir(path: &str) -> Result<()> {
	disk::create_dir(path).map_err(|e| about("create directory", path, e))
}
fn mkdir_all(path: &str) -> Result<()> {
	disk::create_dir_all(path).map_err(|e| about("create directories", path, e))
}
fn unicode(path: PathBuf, verb: &str, subject: &str) -> Result<String> {
	path.into_os_string()
		.into_string()
		.map_err(|name| about(verb, subject, format!("non-Unicode name {name:?}")))
}
pub(crate) fn absolute(path: &str) -> Result<String> {
	unicode(
		disk::canonicalize(path).map_err(|e| about("resolve", path, e))?,
		"resolve",
		path,
	)
}
fn cwd() -> Result<String> {
	unicode(
		std::env::current_dir().map_err(|e| about("resolve", ".", e))?,
		"resolve",
		".",
	)
}
fn temp_dir() -> Result<String> {
	unicode(std::env::temp_dir(), "resolve", "temporary directory")
}
fn exists(path: &str) -> Result<bool> {
	disk::exists(path).map_err(|e| about("inspect", path, e))
}
fn modified_ms(time: SystemTime) -> Result<i64> {
	let ms: i128 = match time.duration_since(UNIX_EPOCH) {
		Ok(d) => d.as_millis() as i128,
		Err(e) => {
			let d = e.duration();
			-(d.as_millis() as i128) - i128::from(d.subsec_nanos() % 1_000_000 != 0)
		}
	};
	i64::try_from(ms).map_err(|_| "modification time exceeds i64 milliseconds".into())
}
fn metadata(path: &str) -> Result<Value> {
	let link = disk::symlink_metadata(path).map_err(|e| about("inspect", path, e))?;
	let meta = disk::metadata(path).map_err(|e| {
		if link.is_symlink() && e.kind() == std::io::ErrorKind::NotFound {
			about("inspect", path, "symlink target is missing")
		} else {
			about("inspect", path, e)
		}
	})?;
	let ms = modified_ms(meta.modified().map_err(|e| about("inspect", path, e))?)
		.map_err(|e| about("inspect", path, e))?;
	let convert = |e| about("inspect", path, e);
	let mut object = Object::new();
	let kind = if meta.is_file() {
		"file"
	} else if meta.is_dir() {
		"dir"
	} else {
		"other"
	};
	for (name, value) in [
		("kind", rune::to_value(kind).map_err(convert)?),
		("size", Value::from(meta.len())),
		("modified_ms", Value::from(ms)),
		("readonly", Value::from(meta.permissions().readonly())),
		("symlink", Value::from(link.is_symlink())),
	] {
		object
			.insert(
				rune::alloc::String::try_from(name).map_err(|e| about("inspect", path, e))?,
				value,
			)
			.map_err(|e| about("inspect", path, e))?;
	}
	rune::to_value(object).map_err(convert)
}
fn read_dir(path: &str) -> Result<Vec<String>> {
	let mut names = Vec::new();
	for entry in disk::read_dir(path).map_err(|e| about("list", path, e))? {
		let entry = entry.map_err(|e| about("list", path, e))?;
		if names.len() == ENTRY_LIMIT {
			return Err(about("list", path, "it exceeds the 100000 entry limit"));
		}
		names.push(unicode(PathBuf::from(entry.file_name()), "list", path)?);
	}
	names.sort_unstable();
	Ok(names)
}
fn copy(from: &str, to: &str) -> Result<()> {
	let subject = format!("{from} to {to}");
	let source = regular(from).map_err(|e| about("copy", &subject, e))?;
	let permissions = source
		.metadata()
		.map_err(|e| about("copy", &subject, e))?
		.permissions();
	let mut target = disk::OpenOptions::new()
		.write(true)
		.create_new(true)
		.open(to)
		.map_err(|e| {
			about(
				"copy",
				&subject,
				if e.kind() == std::io::ErrorKind::AlreadyExists {
					"destination already exists".into()
				} else {
					e.to_string()
				},
			)
		})?;
	#[cfg(feature = "test-support")]
	let mut source = CopyReader {
		file: source,
		remaining: std::env::var("RNX_TEST_COPY_FAIL_AFTER")
			.ok()
			.and_then(|n| n.parse().ok()),
	};
	#[cfg(not(feature = "test-support"))]
	let mut source = source;
	std::io::copy(&mut source, &mut target)
		.and_then(|_| target.set_permissions(permissions))
		.map_err(|e| {
			about(
				"copy",
				&subject,
				format!("{e}; destination was created and left"),
			)
		})
}
#[cfg(feature = "test-support")]
struct CopyReader {
	file: disk::File,
	remaining: Option<usize>,
}
#[cfg(feature = "test-support")]
impl Read for CopyReader {
	fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
		if bytes.is_empty() {
			return Ok(0);
		}
		if self.remaining == Some(0) {
			return Err(std::io::Error::other("injected copy read failure"));
		}
		let len = self.remaining.unwrap_or(bytes.len()).min(bytes.len());
		let n = self.file.read(&mut bytes[..len])?;
		if let Some(left) = &mut self.remaining {
			*left -= n;
		}
		Ok(n)
	}
}
fn rename(from: &str, to: &str) -> Result<()> {
	crate::fs_platform::rename(from, to).map_err(|e| {
		about(
			"rename",
			&format!("{from} to {to}"),
			if e.kind() == std::io::ErrorKind::AlreadyExists {
				"destination already exists".into()
			} else {
				e.to_string()
			},
		)
	})
}
fn unlink(path: &Path, meta: &disk::Metadata) -> std::io::Result<()> {
	#[cfg(windows)]
	{
		use std::os::windows::fs::FileTypeExt;
		if meta.file_type().is_symlink_dir() {
			return disk::remove_dir(path);
		}
	}
	let _ = meta;
	disk::remove_file(path)
}
fn remove_file(path: &str) -> Result<()> {
	let meta = disk::symlink_metadata(path).map_err(|e| about("remove file", path, e))?;
	unlink(Path::new(path), &meta).map_err(|e| about("remove file", path, e))
}
fn remove_dir(path: &str) -> Result<()> {
	disk::remove_dir(path).map_err(|e| about("remove directory", path, e))
}
fn remove_dir_all(path: &str) -> Result<()> {
	let err = |e| about("remove tree", path, e);
	let meta = disk::symlink_metadata(path).map_err(err)?;
	if meta.is_symlink() {
		return unlink(Path::new(path), &meta).map_err(err);
	}
	let canonical = disk::canonicalize(path).map_err(err)?;
	if canonical.parent().is_none() {
		return Err(about("remove tree", path, "it is a filesystem root"));
	}
	let current = std::env::current_dir()
		.and_then(disk::canonicalize)
		.map_err(err)?;
	if current == canonical {
		return Err(about("remove tree", path, "it is the working directory"));
	}
	if current.starts_with(&canonical) {
		return Err(about(
			"remove tree",
			path,
			"it is an ancestor of the working directory",
		));
	}
	disk::remove_dir_all(path).map_err(err)
}
pub fn install(context: &mut Context) -> crate::Result<Vec<HostFunction>> {
	let mut module = Module::with_crate("fs")?;
	let mut names = Vec::new();
	macro_rules! reg {
		($name:ident, $doc:literal) => {{
			module.function(stringify!($name), $name).build()?;
			names.push(HostFunction {
				path: concat!("fs::", stringify!($name)).into(),
				doc: $doc,
			});
		}};
	}
	reg!(
		read,
		"read(path) -> Result<String>: a regular file as UTF-8, up to 8 MiB; use read_bytes for other bytes"
	);
	reg!(
		read_bytes,
		"read_bytes(path) -> Result<Bytes>: a regular file's bytes, up to 8 MiB"
	);
	reg!(
		write_new,
		"write_new(path, data) -> Result<()>: write String or Bytes, refusing to overwrite an existing file atomically"
	);
	reg!(
		write,
		"write(path, data) -> Result<()>: create or truncate in place and write String or Bytes"
	);
	reg!(
		append,
		"append(path, data) -> Result<()>: create or append String or Bytes"
	);
	reg!(
		exists,
		"exists(path) -> Result<bool>: false for missing or dangling; errors remain errors"
	);
	reg!(
		metadata,
		"metadata(path) -> Result<object>: kind, size, modified_ms since epoch, readonly, symlink; follows the target"
	);
	reg!(
		read_dir,
		"read_dir(path) -> Result<Vec<String>>: Unicode entry names, byte-sorted, up to 100000"
	);
	reg!(
		absolute,
		"absolute(path) -> Result<String>: canonicalize an existing path"
	);
	reg!(
		cwd,
		"cwd() -> Result<String>: the working directory, without lossy conversion"
	);
	reg!(
		temp_dir,
		"temp_dir() -> Result<String>: the temporary directory, without lossy conversion"
	);
	reg!(mkdir, "mkdir(path) -> Result<()>: create one directory");
	reg!(
		mkdir_all,
		"mkdir_all(path) -> Result<()>: create all missing parent directories"
	);
	reg!(
		copy,
		"copy(from, to) -> Result<()>: copy bytes and permissions to an exclusively created file; failure can leave a partial destination"
	);
	reg!(
		rename,
		"rename(from, to) -> Result<()>: atomically rename without replacing a destination; no fallback"
	);
	reg!(
		remove_file,
		"remove_file(path) -> Result<()>: remove a file or a symlink, not its target"
	);
	reg!(
		remove_dir,
		"remove_dir(path) -> Result<()>: remove an empty directory"
	);
	reg!(
		remove_dir_all,
		"remove_dir_all(path) -> Result<()>: remove a tree without following symlinks; refuse root, cwd and ancestors"
	);
	context.install(module)?;
	Ok(names)
}
#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn exactly_eighteen_names_have_help_and_resolve() {
		let mut context = Context::with_default_modules().unwrap();
		let names = super::install(&mut context).unwrap();
		let expected = [
			"read",
			"read_bytes",
			"write_new",
			"write",
			"append",
			"exists",
			"metadata",
			"read_dir",
			"absolute",
			"cwd",
			"temp_dir",
			"mkdir",
			"mkdir_all",
			"copy",
			"rename",
			"remove_file",
			"remove_dir",
			"remove_dir_all",
		];
		assert_eq!(names.len(), expected.len());
		for (function, name) in names.iter().zip(expected) {
			assert_eq!(function.path, format!("fs::{name}"));
			assert!(function.doc.starts_with(name) && function.doc.contains("Result"));
			crate::compile(&context, &format!("pub fn main() {{ fs::{name} }}")).unwrap();
		}
	}

	#[test]
	fn times_floor_before_epoch_and_refuse_overflow() {
		use std::time::Duration;
		assert_eq!(modified_ms(UNIX_EPOCH).unwrap(), 0);
		assert_eq!(
			modified_ms(UNIX_EPOCH - Duration::from_nanos(1)).unwrap(),
			-1
		);
		assert_eq!(
			modified_ms(UNIX_EPOCH - Duration::from_nanos(1_000_001)).unwrap(),
			-2
		);
		if let Some(t) = UNIX_EPOCH.checked_add(Duration::from_secs(i64::MAX as u64 / 1000 + 1)) {
			assert!(modified_ms(t).is_err());
		}
	}
}
