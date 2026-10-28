//! Explicit dev-only cross-repository observer and controller process.
#![deny(clippy::all)]
mod binding;
mod census;
mod enrollment;

use serde_json::{Value, json};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub(super) const COMPILED_PINS: &str =
	include_str!(concat!(env!("OUT_DIR"), "/verification-source-pins.tsv"));

fn write_new(path: &Path, value: &Value) -> Result<()> {
	let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
	serde_json::to_writer_pretty(&mut file, value)?;
	file.write_all(b"\n")?;
	Ok(())
}

fn controller(path: &str) -> Result<()> {
	let request: Value = serde_json::from_slice(&fs::read(path)?)?;
	let output = Path::new(request["out"].as_str().ok_or("missing output root")?);
	let before = binding::before(&request)?;
	let args = vec![
		"--fixtures-root".to_owned(),
		request["fixtures_root"]
			.as_str()
			.ok_or("missing fixtures root")?
			.to_owned(),
		"--out".to_owned(),
		output.to_str().ok_or("non-UTF8 output")?.to_owned(),
	];
	let suite = request["suite"].as_str().ok_or("missing suite")?;
	let registration = if request["registration_fixture"].is_null() {
		"actual"
	} else {
		"explicit registration-fixture control"
	};
	let mut results = serde_json::Map::new();
	if matches!(suite, "census" | "all") {
		write_new(
			&output.join("CENSUS-SELF-CHECK.json"),
			&census::self_check()?,
		)?;
	}
	match suite {
		"census" => {
			results.insert("census".into(), census::run(&args)?);
		}
		"enrollment" => {
			results.insert("enrollment".into(), enrollment::run(&args)?);
		}
		"all" => {
			results.insert("census".into(), census::run(&args)?);
			results.insert("enrollment".into(), enrollment::run(&args)?);
		}
		_ => return Err("unknown suite; expected census, enrollment, or all".into()),
	}
	binding::after(&request, &before)?;
	write_new(
		&output.join("RESULT.json"),
		&json!({
			"status":"PASS", "suite":suite, "registration":registration,
			"binding":before, "results":results,
			"roles":"controller and isolated observers use the same bound dev executable"
		}),
	)
}

pub(crate) fn main() -> Result<()> {
	let args: Vec<String> = std::env::args().skip(1).collect();
	match args.as_slice() {
		[mode, path] if mode == "--host-check" => {
			let request = serde_json::from_slice(&fs::read(path)?)?;
			binding::host_check(&request)
		}
		[mode, path] if mode == "--controller" => controller(path),
		[mode, suite, rest @ ..] if mode == "--child" && suite == "census" => {
			let value = census::child(rest)?;
			serde_json::to_writer(std::io::stdout().lock(), &value)?;
			Ok(())
		}
		[mode, config, output] if mode == "enrollment-child" => {
			let value = enrollment::child(&[config.clone(), output.clone()])?;
			write_new(Path::new(output), &value)
		}
		_ => Err(
			"explicit verifier role required: --controller, --child census, or enrollment-child"
				.into(),
		),
	}
}
