//! Test-only adapter around private core functions, not the product CLI.
mod artifact;
mod assembly;
mod cache_entry;
mod cache_identity;
mod cache_storage;
#[allow(dead_code)]
mod commands;
mod fingerprint;
mod generate;
mod git_inventory;
mod graph;
mod handshake;
mod input;
mod inventory;
mod manifest;
mod new_identity;
mod schemas;
mod wire;
use std::{ffi::OsString, path::Path};
fn run() -> Result<(), String> {
	let args: Vec<OsString> = std::env::args_os().skip(1).collect();
	match args.first().and_then(|a| a.to_str()) {
		Some("schema") if args.len() == 4 => {
			let bytes = input::read(Path::new(&args[2]), input::DOCUMENT_LIMIT)?;
			let output = schemas::validate_vector(args[1].to_str().ok_or("schema name")?, &bytes)?;
			std::fs::write(&args[3], &output).map_err(|e| e.to_string())?;
			println!("{}", blake3::hash(&output));
			Ok(())
		}
		// Record 0068 gate 1: print the generated wrapper for a manifest, so a
		// fixture can compare bytes with and without `presentation`.
		Some("wrapper") if args.len() == 3 => {
			let path = Path::new(&args[1]);
			let base = Path::new(&args[2]);
			let bytes = input::read(path, input::MANIFEST_LIMIT)?;
			let v: toml::Value =
				toml::from_str(std::str::from_utf8(&bytes).map_err(|e| e.to_string())?)
					.map_err(|e| e.to_string())?;
			let (cargo, main) = if v.get("format").and_then(toml::Value::as_integer) == Some(2) {
				generate::git_wrapper(&schemas::Declaration::parse(&bytes)?, base)?
			} else {
				generate::wrapper(&manifest::Manifest::parse(&bytes)?, base)?
			};
			print!("{main}");
			println!("---");
			print!("{cargo}");
			Ok(())
		}
		Some("git-verify") if args.len() == 2 => {
			let packages: Vec<schemas::GitPackage> =
				serde_json::from_slice(&input::read(Path::new(&args[1]), input::DOCUMENT_LIMIT)?)
					.map_err(|e| e.to_string())?;
			git_inventory::verify(&packages)
		}

		Some("nested-inventory") if args.len() == 2 => {
			#[derive(serde::Deserialize)]
			struct Config {
				roots: Vec<std::path::PathBuf>,
				candidate: bool,
				entries: usize,
				bytes: u64,
				#[serde(default)]
				repeat: bool,
			}
			let c: Config =
				serde_json::from_slice(&std::fs::read(&args[1]).map_err(|e| e.to_string())?)
					.map_err(|e| e.to_string())?;
			let mut a = fingerprint::Allowance::bounded(c.entries, c.bytes);
			let roots = c.roots.clone();
			let answer = if c.candidate {
				fingerprint::many(c.roots, &mut a)
			} else {
				c.roots
					.into_iter()
					.collect::<std::collections::BTreeSet<_>>()
					.into_iter()
					.map(|p| {
						let tree = fingerprint::native(&p, &mut a)?;
						fingerprint::trace::between(&p)?;
						Ok(tree)
					})
					.collect::<Result<Vec<_>, String>>()
			};
			let repeated = if c.repeat {
				Some(fingerprint::many(
					roots,
					&mut fingerprint::Allowance::bounded(c.entries, c.bytes),
				))
			} else {
				None
			};
			println!("{}",serde_json::to_string(&serde_json::json!({"answer":answer,"repeated":repeated,"events":fingerprint::trace::take(),"remaining_bytes":a.remaining_bytes()})).map_err(|e|e.to_string())?);
			Ok(())
		}

		Some(version @ ("fingerprint-v1" | "fingerprint-v2")) if args.len() == 5 => {
			let mode = args[1].to_str().ok_or("mode is not Unicode")?;
			let path = Path::new(&args[2]);
			let entries = args[3]
				.to_str()
				.ok_or("entries")?
				.parse()
				.map_err(|_| "entries")?;
			let bytes = args[4]
				.to_str()
				.ok_or("bytes")?
				.parse()
				.map_err(|_| "bytes")?;
			macro_rules! fingerprint {
				($m:path) => {{
					use $m as f;
					let mut allowance = f::Allowance::bounded(entries, bytes);
					match mode {
						"source" => wire::encode(&f::source(path, false, &mut allowance)?)?,
						"native" => wire::encode(&f::native(path, &mut allowance)?)?,
						"one" => wire::encode(&f::one(path, &mut allowance)?)?,
						_ => return Err("fingerprint mode".into()),
					}
				}};
			}
			let result = if version == "fingerprint-v1" {
				fingerprint!(crate::fingerprint::legacy)
			} else {
				fingerprint!(crate::fingerprint)
			};
			println!("{}", String::from_utf8(result).map_err(|e| e.to_string())?);
			Ok(())
		}
		Some("prepare") if args.len() == 3 => {
			assembly::prepare(Path::new(&args[1]), Path::new(&args[2]))
		}
		Some("hash") if args.len() == 2 => {
			println!("{}", assembly::executable_hash(Path::new(&args[1]))?);
			Ok(())
		}
		Some("run") if args.len() >= 5 => {
			let expected = args[2].to_str().ok_or("hash is not Unicode")?;
			let map = if args[3] == "-" {
				None
			} else {
				Some(Path::new(&args[3]))
			};
			let mut command = assembly::command(
				Path::new(&args[1]),
				expected,
				map,
				Path::new(&args[4]),
				&args[5..],
			)?;
			#[cfg(unix)]
			{
				use std::os::unix::process::CommandExt;
				Err(command.exec().to_string())
			}
			#[cfg(not(unix))]
			{
				let status = command.status().map_err(|e| e.to_string())?;
				std::process::exit(status.code().unwrap_or(1));
			}
		}
		_ => Err(
			"test probe: prepare MANIFEST STAGE | hash EXE | run EXE HASH MAP-OR-- ENTRY [ARGS]"
				.into(),
		),
	}
}
fn main() {
	if let Err(e) = run() {
		eprintln!("{e}");
		std::process::exit(1);
	}
}
