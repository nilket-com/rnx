//! Explicit maintenance only. No launch path, identity or lifetime protocol.
#![cfg_attr(any(test, not(target_os = "linux")), allow(dead_code))]
use std::{ffi::OsString, path::PathBuf};
#[cfg(target_os = "linux")]
mod hooks;
#[cfg(target_os = "linux")]
mod linux;

struct Args {
	root: PathBuf,
	kind: String,
	id: Option<String>,
	list: bool,
	dry: bool,
	resume: bool,
	quiescent: bool,
	manifests: Vec<PathBuf>,
}
fn valid_id(s: &str) -> bool {
	s.len() == 64
		&& s.bytes()
			.all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn absolute(p: PathBuf) -> Result<PathBuf, String> {
	if !p.is_absolute() || p.to_str().is_none() {
		Err("maintenance root must be an absolute Unicode path".into())
	} else {
		Ok(p)
	}
}
fn envpath(key: &str) -> Result<Option<PathBuf>, String> {
	std::env::var_os(key)
		.map(|p| absolute(p.into()).map_err(|e| format!("{key}: {e}")))
		.transpose()
}
fn root(kind: &str) -> Result<PathBuf, String> {
	if kind == "cache" {
		if let Some(p) = envpath("RNX_PROJECT_CACHE")? {
			return Ok(p);
		}
		if let Some(p) = envpath("XDG_CACHE_HOME")? {
			return Ok(p.join("rnx/assemblies"));
		}
		Ok(envpath("HOME")?
			.ok_or("HOME or cache selection is required")?
			.join(".cache/rnx/assemblies"))
	} else {
		if let Some(p) = envpath("XDG_DATA_HOME")? {
			return Ok(p.join("rnx/runtimes"));
		}
		Ok(envpath("HOME")?
			.ok_or("HOME or runtime selection is required")?
			.join(".local/share/rnx/runtimes"))
	}
}
fn parse(args: &[OsString]) -> Result<Args, String> {
	let mut it = args.iter();
	let kind = it
		.next()
		.and_then(|s| s.to_str())
		.filter(|s| matches!(*s, "cache" | "runtime"))
		.ok_or("expected cache or runtime")?
		.to_owned();
	let action = it
		.next()
		.and_then(|s| s.to_str())
		.ok_or("expected list or remove")?;
	let list = action == "list";
	if !list && action != "remove" {
		return Err("expected list or remove".into());
	}
	let id = if list {
		None
	} else {
		Some(
			it.next()
				.and_then(|s| s.to_str())
				.filter(|s| valid_id(s))
				.ok_or("remove requires one full 64-character lowercase hexadecimal ID")?
				.to_owned(),
		)
	};
	let mut out = Args {
		root: PathBuf::new(),
		kind,
		id,
		list,
		dry: false,
		resume: false,
		quiescent: false,
		manifests: vec![],
	};
	let mut seen = std::collections::HashSet::new();
	let mut explicit = None;
	while let Some(arg) = it.next() {
		if arg != "--manifest" && !seen.insert(arg) {
			return Err(format!("duplicate maintenance option {arg:?}"));
		}
		match arg.to_str() {
			Some("--root") => {
				explicit = Some(absolute(PathBuf::from(
					it.next().ok_or("--root needs a path")?,
				))?)
			}
			Some("--manifest") => {
				if out.manifests.len() == 64 {
					return Err("at most 64 manifest paths may be named".into());
				}
				out.manifests.push(
					it.next()
						.filter(|v| !v.is_empty())
						.ok_or("--manifest needs a path")?
						.into(),
				);
			}
			Some("--dry-run") => out.dry = true,
			Some("--resume") => out.resume = true,
			Some("--quiescent") => out.quiescent = true,
			_ => return Err(format!("unexpected maintenance argument {arg:?}")),
		}
	}
	if list && (out.dry || out.resume || out.quiescent) {
		return Err("list accepts only --root and --manifest".into());
	}
	if !list && !out.dry && !out.manifests.is_empty() {
		return Err(
			"--manifest is for listing and --dry-run only; it does not authorize removal".into(),
		);
	}
	out.root = match explicit {
		Some(p) => p,
		None => root(&out.kind)?,
	};
	if !list && !out.dry && !out.quiescent {
		return Err(format!(
			"removal requires --quiescent: stop all consumers, including older tools, surviving build children, direct executions, sessions, servers and kernels; retained references may break and runtime source may be lost. Inspect first:\n{} {} remove {} --root {} --dry-run{}",
			shell(&std::env::current_exe().map_err(|e| e.to_string())?),
			out.kind,
			out.id.as_ref().unwrap(),
			shell(&out.root),
			if out.resume { " --resume" } else { "" }
		));
	}
	Ok(out)
}
fn shell(p: &std::path::Path) -> String {
	format!("'{}'", p.to_string_lossy().replace('\'', "'\\''"))
}
pub(crate) fn cli(args: &[OsString]) -> Result<(), String> {
	#[cfg(target_os = "linux")]
	{
		linux::run(parse(args)?)
	}
	#[cfg(not(target_os = "linux"))]
	{
		let _ = args;
		Err("cache/runtime maintenance requires Linux; no storage was changed".into())
	}
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
	use super::*;
	fn args(s: &[&str]) -> Vec<OsString> {
		s.iter().map(OsString::from).collect()
	}
	#[test]
	fn maintenance_refuses_ambiguous_or_mutating_annotations_before_open() {
		let id = "a".repeat(64);
		for a in [
			vec!["cache", "remove", "abc"],
			vec!["cache", "list", "--resume"],
			vec!["cache", "list", "--root", "relative"],
			vec!["cache", "list", "--root", "/a", "--root", "/b"],
			vec![
				"cache",
				"remove",
				&id,
				"--root",
				"/absent",
				"--quiescent",
				"--manifest",
				"x",
			],
		] {
			assert!(parse(&args(&a)).is_err());
		}
		assert!(
			parse(&args(&[
				"cache",
				"remove",
				&id,
				"--root",
				"/absent",
				"--dry-run",
				"--manifest",
				"x",
				"--manifest",
				"x"
			]))
			.is_ok()
		);
	}
}
