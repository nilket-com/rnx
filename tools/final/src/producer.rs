//! The producer's side of the stage protocol, as `child_result.publish` and
//! `bind` write it: a semantic receipt bound to the validation context the
//! controller handed the stage, then one terminal envelope naming it.
use crate::child::{CONTEXT, environment, head, source, value_error};
use crate::failure::Environment;
use crate::pyjson;
use crate::receipt::{atomic_json, digest, sync_directory};
use anyhow::{Context as _, Result, bail};
use serde_json::{Value, json};
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// Publish `value` as the stage's semantic receipt at `path`. Inside a
/// final (the context variable set), it carries the context's identity; a
/// receipt already there from before this run is replaced only on an
/// explicit resume of a PASS whose bytes have not changed, after its bytes
/// are archived beside the context. Then the receipt is bound. Outside a
/// final the receipt is written unbound, and no final accepts it.
pub fn publish(path: &Path, value: &Value, env: &Environment) -> Result<()> {
	let mut value = value.clone();
	if let Some(context_path) = env.get(CONTEXT).filter(|v| !v.is_empty()) {
		let context_path = PathBuf::from(context_path);
		let ctx: Value = serde_json::from_slice(&std::fs::read(&context_path)?)?;
		let resolved = resolve(path)?;
		if let Some(prior) = ctx["existing_results"].get(&*resolved.to_string_lossy()) {
			let before: Value = serde_json::from_slice(&std::fs::read(path)?)?;
			if ctx["resume"] != json!(true) || before["status"] != "PASS" {
				bail!(value_error(
					"cannot overwrite a completed failure or implicitly resume"
				));
			}
			let now = digest(path)?;
			if Some(now.as_str()) != prior.as_str() {
				bail!(value_error("prior result changed"));
			}
			let archive = context_path.with_file_name(format!("PRIOR-{now}.json"));
			let mut file = std::fs::OpenOptions::new()
				.write(true)
				.create_new(true)
				.open(&archive)?;
			file.write_all(&std::fs::read(path)?)?;
			file.sync_all()?;
			sync_directory(archive.parent().context("archive has no directory")?)?;
		}
		value["validation_context"] = json!(pyjson::identity(&ctx)?);
	}
	atomic_json(path, &value)?;
	bind(path, env)
}

/// Bind an existing semantic receipt: the terminal envelope beside the
/// context, written once, after the receipt is durable, and only while the
/// candidate, source and declared environment are those of the context.
pub fn bind(path: &Path, env: &Environment) -> Result<()> {
	let Some(context_path) = env.get(CONTEXT).filter(|v| !v.is_empty()) else {
		return Ok(());
	};
	let context_path = PathBuf::from(context_path);
	let ctx: Value = serde_json::from_slice(&std::fs::read(&context_path)?)?;
	let terminal = context_path.with_file_name("CHILD.json");
	if terminal.exists() {
		bail!(value_error("child terminal receipt already exists"));
	}
	let path = std::fs::canonicalize(path)?;
	let value: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
	if !matches!(value["status"].as_str(), Some("PASS" | "FAIL")) {
		bail!(value_error("invalid producer status"));
	}
	let root = Path::new(ctx["root"].as_str().context("context has no root")?);
	let mut declared = env.clone();
	declared.remove(CONTEXT);
	if json!(head(root)) != ctx["candidate"]
		|| json!(source(root)?) != ctx["source"]
		|| environment(&declared)? != ctx["environment"]
	{
		bail!(value_error("producer source/environment changed"));
	}
	// A Rust stage's context names its executable; it must still be this one.
	if !ctx["executable"].is_null() {
		let program = std::fs::canonicalize("/proc/self/exe")?;
		if ctx["executable"]["path"] != json!(program.to_string_lossy())
			|| ctx["executable"]["sha256"] != json!(digest(&program)?)
		{
			bail!(value_error("producer executable changed"));
		}
	}
	std::fs::File::open(&path)?.sync_all()?;
	sync_directory(path.parent().context("receipt has no directory")?)?;
	atomic_json(
		&terminal,
		&json!({"schema": 1, "context": pyjson::identity(&ctx)?, "invocation": ctx["invocation"],
			"source": ctx["source"], "status": value["status"], "receipt": path.to_string_lossy(), "sha256": digest(&path)?}),
	)
}

/// A path as Python's `Path.resolve()` gives it: symlinks of its existing
/// part followed, the rest kept.
fn resolve(path: &Path) -> Result<PathBuf> {
	let absolute = if path.is_absolute() {
		path.to_owned()
	} else {
		std::env::current_dir()?.join(path)
	};
	let mut existing = absolute.clone();
	let mut rest = Vec::new();
	while !existing.exists() {
		rest.push(
			existing
				.file_name()
				.context("no existing ancestor")?
				.to_owned(),
		);
		existing = existing
			.parent()
			.context("no existing ancestor")?
			.to_owned();
	}
	let mut out = std::fs::canonicalize(existing)?;
	for part in rest.into_iter().rev() {
		out.push(part);
	}
	Ok(out)
}

/// A final's owned root for `variable`, or `standalone` outside a final,
/// as `final_evidence.assigned_root` gives it: inside a final the variable
/// must name an existing canonical directory the final created.
pub fn assigned_root(variable: &str, standalone: &Path, env: &Environment) -> Result<PathBuf> {
	if env.get("RNX_FINAL").is_none_or(|v| v.is_empty()) {
		return Ok(standalone.to_owned());
	}
	let value = env
		.get(variable)
		.filter(|v| !v.is_empty())
		.with_context(|| format!("final output root not assigned: {variable}"))?;
	let path = PathBuf::from(value);
	let absolute = if path.is_absolute() {
		path
	} else {
		std::env::current_dir()?.join(path)
	};
	// Python resolves a missing path to itself, then finds no directory.
	if absolute.exists() && std::fs::canonicalize(&absolute)? != absolute {
		bail!(value_error("symlink or noncanonical owner path"));
	}
	if !absolute.is_dir() {
		bail!(value_error("final owner must create root"));
	}
	Ok(absolute)
}
