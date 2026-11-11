//! ADR0247: isolated executed witnesses; no ambient environment enters receipts.
use serde_json::Value;
const NAMES: &[&str] = &[
	"process::exit",
	"fs::metadata",
	"fs::read_dir",
	"time::parts",
	"text::split_max",
	"env::args",
	"env::var",
	"env::home_dir",
	"env::vars",
];
pub(crate) fn is_new(name: &str) -> bool {
	NAMES.contains(&name)
}

pub(crate) fn isolated_witness(name: &str) -> crate::routing_inventory::Result<Value> {
	static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
	let dir = Scratch(std::env::temp_dir().join(format!(
			"rnx-0247-{}-{}-{}",
			std::process::id(),
			std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)?
				.as_nanos(),
			NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
		)));
	std::fs::create_dir(dir.path())?;
	std::fs::write(dir.path().join("a.txt"), "abc")?;
	std::fs::create_dir(dir.path().join("empty"))?;
	let receipt = dir.path().join("receipt.json");
	let output = std::process::Command::new(std::env::current_exe()?)
		.args([
			"--exact",
			"session::host_enrollment::child_witness",
			"--nocapture",
		])
		.env_clear()
		.env("RNX_0247_HOST", name)
		.env("RNX_0247_RECEIPT", &receipt)
		.env("RNX_0247_VALUE", "fixture")
		.env("RNX_0247_EMPTY", "")
		.env("HOME", "/0247-fixture-home")
		.env("USERPROFILE", "/0247-fixture-home")
		.current_dir(dir.path())
		.output()?;
	if !output.status.success() {
		return Err(format!(
			"isolated host witness {name}: {}",
			String::from_utf8_lossy(&output.stderr)
		)
		.into());
	}
	let result: Value = serde_json::from_slice(&std::fs::read(receipt)?)?;
	assert_eq!(result["class"], "Equal", "different host result: {result}");
	Ok(result)
}

#[test]
fn child_witness() {
	let Ok(name) = std::env::var("RNX_0247_HOST") else {
		return;
	};
	let input = match name.as_str() {
		"process::exit" => "process::exit(0)",
		"fs::metadata" => {
			"let m=fs::metadata(\"a.txt\").unwrap(); (m.kind,m.size,m.readonly,m.symlink,m.modified_ms>0,fs::metadata(\"missing\").is_err())"
		}
		"fs::read_dir" => {
			"(fs::read_dir(\".\"),fs::read_dir(\"empty\"),fs::read_dir(\"missing\").is_err())"
		}
		"time::parts" => "(time::parts(0,\"UTC\"),time::parts(0,\"not-a-zone\").is_err())",
		"text::split_max" => {
			"(text::split_max(\"a b c\",2),text::split_max(\"\",0),text::split_max(\"a\",-1))"
		}
		"env::args" => "env::args()",
		"env::var" => {
			"(env::var(\"RNX_0247_VALUE\"),env::var(\"RNX_0247_EMPTY\"),env::var(\"RNX_0247_MISSING\"),env::var(\"=bad\"))"
		}
		"env::home_dir" => "env::home_dir()",
		"env::vars" => "let e=env::vars().unwrap();(e[\"RNX_0247_VALUE\"],e[\"RNX_0247_EMPTY\"])",
		_ => panic!("unknown witness"),
	};
	let row = super::parity::observe_host(
		&name,
		input,
		crate::routing_inventory::session_parity_context().unwrap(),
	)
	.unwrap();
	assert_eq!(row["class"], "Equal", "{row}");
	std::fs::write(
		std::env::var_os("RNX_0247_RECEIPT").unwrap(),
		serde_json::to_vec_pretty(&row).unwrap(),
	)
	.unwrap();
}

#[test]
fn nine_executed_witnesses() {
	for name in NAMES {
		isolated_witness(name).unwrap();
	}
}

#[test]
fn resolved_refusals_preserve_aliases_and_first_cause() {
	use super::domain_session::{DomainSession, transcripts::parity_new};
	for source in [
		"fs::write(\"missing\",\"data\")",
		"use fs::write as save; save(\"missing\",\"data\")",
		"use fs::write; write(\"missing\",\"data\")",
	] {
		let mut session = DomainSession::new(
			crate::routing_inventory::session_parity_context().unwrap(),
			super::DEFAULT_CEILING,
		)
		.unwrap();
		let result = parity_new(&mut session, source);
		let text = result.to_string();
		if source.starts_with("use ") {
			assert!(text.contains("imports"), "first cause lost: {result}");
		} else {
			assert!(text.contains("fs::write needs dynamic input"), "{result}");
			assert!(text.contains("without --engine=execution"), "{result}");
		}
		session.close().unwrap();
	}
}

struct Scratch(std::path::PathBuf);
impl Scratch {
	fn path(&self) -> &std::path::Path {
		&self.0
	}
}
impl Drop for Scratch {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.0);
	}
}

#[test]
fn compiler_reports_resolved_host_for_imported_and_aliased_calls() {
	for source in [
		"pub fn main(){fs::write(\"missing\",\"data\")}",
		"use fs::write as save; pub fn main(){save(\"missing\",\"data\")}",
		"use fs::write;pub fn main(){write(\"missing\",\"data\")}",
	] {
		let context = crate::routing_inventory::session_parity_context().unwrap();
		let mut sources = rune::Sources::new();
		sources
			.insert(rune::Source::memory(source).unwrap())
			.unwrap();
		let mut diagnostics = rune::Diagnostics::new();
		let error =
			rune::experimental_core::frontend::compile(&mut sources, &context, &mut diagnostics, 1)
				.err()
				.unwrap();
		assert_eq!(
			error.refusal,
			Some(rune::experimental_core::error::Refusal::Native)
		);
		let host = diagnostics
			.diagnostics()
			.iter()
			.find_map(|d| match d {
				rune::diagnostics::Diagnostic::Fatal(f) => {
					f.as_compile_error().and_then(|e| e.unenrolled_host())
				}
				_ => None,
			})
			.expect("resolved host diagnostic");
		assert_eq!(host.to_string().trim_start_matches("::"), "fs::write");
	}
}
