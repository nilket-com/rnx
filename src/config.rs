//! One bounded, in-memory Rune evaluation for presentation settings.
use crate::presentation::{Colour, Mode};
use rune::runtime::{Object, Value};
use rune::{Context, Source, Sources, Vm};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
const SOURCE_CAP: usize = 64 * 1024;
const BUDGET: usize = 100_000;
pub const ROLES: [&str; 8] = [
	"keyword",
	"string",
	"number",
	"comment",
	"prompt_number",
	"error",
	"result_number",
	"prompt_frame",
];
#[derive(Default)]
pub struct Settings {
	pub mode: Option<Mode>,
	pub splash: Option<bool>,
	pub palette: [Option<Colour>; 8],
}
fn variable(name: &str) -> Result<Option<String>, String> {
	std::env::var_os(name)
		.filter(|s| !s.is_empty())
		.map(|s| {
			s.into_string()
				.map_err(|s| format!("{name} is not Unicode: {s:?}"))
		})
		.transpose()
}
fn absolute(name: &str, value: String) -> Result<PathBuf, String> {
	let path = PathBuf::from(value);
	if path.is_absolute() {
		Ok(path)
	} else {
		Err(format!(
			"{name} must be an absolute path: {}",
			path.display()
		))
	}
}
fn location() -> Result<Option<PathBuf>, String> {
	if let Some(value) = variable("RNX_CONFIG")? {
		return absolute("RNX_CONFIG", value).map(Some);
	}
	#[cfg(unix)]
	let base = {
		let xdg = variable("XDG_CONFIG_HOME")?
			.map(PathBuf::from)
			.filter(|p| p.is_absolute());
		if xdg.is_some() {
			xdg
		} else {
			variable("HOME")?
				.map(|s| absolute("HOME", s).map(|p| p.join(".config")))
				.transpose()?
		}
	};
	#[cfg(windows)]
	let base = match variable("APPDATA")? {
		Some(s) => Some(absolute("APPDATA", s)?),
		None => variable("LOCALAPPDATA")?
			.map(|s| absolute("LOCALAPPDATA", s))
			.transpose()?,
	};
	Ok(base.map(|p| p.join("rnx/config.rn")))
}
fn warning(path: &Path, reason: &str) {
	eprintln!(
		"rnx: config `{}`: {}",
		crate::format::terminal_safe(&path.to_string_lossy()),
		crate::format::terminal_safe(reason)
	);
}
// Test-support counts actual attempts at the open boundary, not just evaluation.
// The guard is created before command dispatch, so fast-path returns report zero.
#[cfg(feature = "test-support")]
static READS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
#[cfg(feature = "test-support")]
pub struct ReadReport;
#[cfg(feature = "test-support")]
impl Drop for ReadReport {
	fn drop(&mut self) {
		report_reads();
	}
}
#[cfg(feature = "test-support")]
pub fn report_reads() {
	if let Some(path) = std::env::var_os("RNX_TEST_CONFIG_READS") {
		let _ = std::fs::write(
			path,
			READS.load(std::sync::atomic::Ordering::Relaxed).to_string(),
		);
	}
}
pub fn load() -> Settings {
	let path = match location() {
		Ok(Some(path)) => path,
		Ok(None) => return Settings::default(),
		Err(reason) => {
			warning(Path::new("discovery"), &reason);
			return Settings::default();
		}
	};
	#[cfg(feature = "test-support")]
	READS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
	let file = match crate::fs::regular(&path) {
		Ok(file) => file,
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Settings::default(),
		Err(e) => {
			warning(&path, &e.to_string());
			return Settings::default();
		}
	};
	let read = || -> Result<String, String> {
		let mut bytes = Vec::new();
		file.take((SOURCE_CAP + 1) as u64)
			.read_to_end(&mut bytes)
			.map_err(|e| e.to_string())?;
		if bytes.len() > SOURCE_CAP {
			return Err("source exceeds the 64 KiB limit".into());
		}
		String::from_utf8(bytes).map_err(|e| format!("source is not UTF-8: {e}"))
	};
	let source = match read() {
		Ok(s) => s,
		Err(e) => {
			warning(&path, &e);
			return Settings::default();
		}
	};
	// Only a blank file is silent: explicit () remains a shape refusal.
	if source.trim().is_empty() {
		return Settings::default();
	}
	match evaluate(&source) {
		Ok((settings, warnings)) => {
			for message in warnings {
				warning(&path, &message);
			}
			settings
		}
		Err(message) => {
			warning(&path, &message);
			Settings::default()
		}
	}
}
pub(crate) fn evaluate(source: &str) -> Result<(Settings, Vec<String>), String> {
	use rune::ast::Spanned;
	// A fixed synchronous wrapper: helper items may be written inside it.
	let wrapped = format!("pub fn main() {{\n{source}\n}}");
	let context = Context::with_config(false).map_err(|e| e.to_string())?;
	let mut sources = Sources::new();
	sources
		.insert(Source::memory(&wrapped).map_err(|e| e.to_string())?)
		.map_err(|e| e.to_string())?;
	let mut loader = rune::compile::NoopSourceLoader::default();
	let mut diagnostics = rune::Diagnostics::new();
	let unit = rune::prepare(&mut sources)
		.with_context(&context)
		.with_source_loader(&mut loader)
		.with_diagnostics(&mut diagnostics)
		.build()
		.map_err(|error| {
			diagnostics
				.diagnostics()
				.iter()
				.find_map(|d| match d {
					rune::diagnostics::Diagnostic::Fatal(f) => match f.kind() {
						rune::diagnostics::FatalDiagnosticKind::CompileError(e) => {
							let offset = e
								.span()
								.range()
								.start
								.saturating_sub("pub fn main() {\n".len())
								.min(source.len());
							let line = source.as_bytes()[..offset]
								.iter()
								.filter(|&&b| b == b'\n')
								.count() + 1;
							Some(format!("cannot compile config at line {line}: {e}"))
						}
						_ => None,
					},
					_ => None,
				})
				.unwrap_or_else(|| format!("cannot compile config: {error}"))
		})?;
	let mut vm = Vm::new(
		Arc::new(context.runtime().map_err(|e| e.to_string())?),
		Arc::new(unit),
	);
	let value = rune::runtime::budget::with(BUDGET, || vm.call(["main"], ()))
		.call()
		.map_err(|e| format!("cannot evaluate config (budget {BUDGET} instructions): {e}"))?;
	settings(&value)
}
fn settings(value: &Value) -> Result<(Settings, Vec<String>), String> {
	let object = value
		.borrow_ref::<Object>()
		.map_err(|_| "config must return an object".to_owned())?;
	let mut settings = Settings::default();
	let mut warnings = Vec::new();
	// Sort keys to make warnings deterministic, independent of object hashing.
	let mut entries: Vec<_> = object.iter().collect();
	entries.sort_by_key(|(key, _)| *key);
	for (key, value) in entries {
		match key.as_str() {
			"color" => match value
				.borrow_ref::<rune::alloc::String>()
				.ok()
				.and_then(|s| Mode::parse(s.as_str()))
			{
				Some(mode) => settings.mode = Some(mode),
				None => warnings.push("color must be auto, always, or never".into()),
			},
			"splash" => match value.as_bool() {
				Ok(v) => settings.splash = Some(v),
				Err(_) => warnings.push("splash must be a boolean".into()),
			},
			"palette" => match value.borrow_ref::<Object>() {
				Ok(palette) => {
					let mut entries: Vec<_> = palette.iter().collect();
					entries.sort_by_key(|(key, _)| *key);
					for (role, value) in entries {
						match ROLES.iter().position(|&r| r == role.as_str()) {
							Some(index) => {
								match value
									.borrow_ref::<rune::alloc::String>()
									.ok()
									.and_then(|s| Colour::parse(s.as_str()))
								{
									Some(colour) => settings.palette[index] = Some(colour),
									None => warnings.push(format!(
										"palette.{role} must be #rrggbb or an ANSI colour name"
									)),
								}
							}
							None => warnings.push(format!("unknown setting palette.{role}")),
						}
					}
				}
				Err(_) => warnings.push("palette must be an object".into()),
			},
			_ => warnings.push(format!("unknown setting {key}")),
		}
	}
	Ok((settings, warnings))
}

#[cfg(test)]
mod tests {
	#[test]
	fn extension_in_a_serving_context_is_absent_from_settings() {
		let mut serving = rune::Context::with_default_modules().unwrap();
		let mut module = rune::Module::with_crate("fixture").unwrap();
		module.function("answer", || 42i64).build().unwrap();
		serving.install(module).unwrap();
		let source = "pub fn main() { fixture::answer() }";
		let mut sources = rune::Sources::new();
		sources
			.insert(rune::Source::memory(source).unwrap())
			.unwrap();
		assert!(
			rune::prepare(&mut sources)
				.with_context(&serving)
				.build()
				.is_ok()
		);
		let error = super::evaluate(source).err().unwrap();
		assert!(error.contains("fixture"), "{error}");
	}
	use super::*;
	#[test]
	fn every_colour_name_and_rgb_value_is_validated_before_emission() {
		for (i, name) in [
			"black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
		]
		.iter()
		.enumerate()
		{
			assert_eq!(Colour::parse(name), Some(Colour::Ansi(30 + i as u8)));
			assert_eq!(
				Colour::parse(&format!("bright-{name}")),
				Some(Colour::Ansi(90 + i as u8))
			);
		}
		assert_eq!(Colour::parse("#Ab12fF"), Some(Colour::Rgb(171, 18, 255)));
		for invalid in [
			"#123",
			"#1234567",
			"#１２",
			"red\x1b[2J",
			"#gg0000",
			"bold red",
		] {
			assert_eq!(Colour::parse(invalid), None);
		}
	}
}
