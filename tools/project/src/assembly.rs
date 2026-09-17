//! Assembly primitives. Atomic lock/receipt orchestration is the next gate.
#![allow(dead_code)]
use crate::{fingerprint, generate, handshake, manifest::Manifest, wire::Handoff};
use std::{ffi::OsString, fs, path::Path, process::Command};

/// Write only into a fresh staging directory beneath the project's .rnx.
/// The caller owns the directory's lifetime and Cargo invocation.
pub(crate) fn prepare(manifest: &Path, stage: &Path) -> Result<(), String> {
	let manifest = manifest.canonicalize().map_err(|e| e.to_string())?;
	let output = manifest.parent().unwrap().join(".rnx");
	if stage.parent() != Some(output.as_path()) || stage.file_name().is_none() {
		return Err(
			"assembly staging must be directly below the application's .rnx directory".into(),
		);
	}
	let app = Manifest::read(&manifest)?;
	let handoff = Handoff::from_project(&manifest)?;
	let (cargo, main) = generate::wrapper(&app, manifest.parent().unwrap())?;
	fs::create_dir(stage).map_err(|e| format!("assembly {}: {e}", stage.display()))?;
	let write = || -> Result<(), String> {
		fs::create_dir(stage.join("src")).map_err(|e| e.to_string())?;
		fs::write(stage.join("Cargo.toml"), cargo).map_err(|e| e.to_string())?;
		fs::write(stage.join("src/main.rs"), main).map_err(|e| e.to_string())?;
		fs::write(stage.join("source-map.json"), handoff.encode()?).map_err(|e| e.to_string())?;
		Ok(())
	};
	if let Err(e) = write() {
		let _ = fs::remove_dir_all(stage);
		return Err(format!("assembly {}: {e}", stage.display()));
	}
	Ok(())
}
pub(crate) fn executable_hash(path: &Path) -> Result<String, String> {
	Ok(fingerprint::one(path, &mut fingerprint::Allowance::default())?.sha256)
}
pub(crate) fn verify(path: &Path, expected: &str) -> Result<(), String> {
	if executable_hash(path)? != expected {
		return Err(format!("executable {}: hash mismatch", path.display()));
	}
	Ok(())
}
/// Hash and capability checks precede script launch. These are trusted local
/// paths, not an atomic anti-replacement guarantee. The caller owns handoff life.
pub(crate) fn command(
	executable: &Path,
	expected: &str,
	map: Option<&Path>,
	entry: &Path,
	args: &[OsString],
) -> Result<Command, String> {
	verify(executable, expected)?;
	let mut command = Command::new(executable);
	command.arg("run");
	if let Some(path) = map {
		let bytes = crate::input::read(path, crate::input::DOCUMENT_LIMIT)?;
		let handoff = Handoff::decode(&bytes)?;
		if Path::new(&handoff.entry) != entry {
			return Err("source map entry does not match launch entry".into());
		}
		if !handoff.mounts.is_empty() {
			handshake::check(executable)?;
			command.arg("--source-map").arg(path);
		}
	}
	command.arg(entry).args(args);
	Ok(command)
}
