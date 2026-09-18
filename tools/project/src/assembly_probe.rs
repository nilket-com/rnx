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
mod graph;
mod handshake;
mod input;
mod inventory;
mod manifest;
mod wire;
use std::{ffi::OsString, path::Path};
fn run() -> Result<(), String> {
	let args: Vec<OsString> = std::env::args_os().skip(1).collect();
	match args.first().and_then(|a| a.to_str()) {
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
