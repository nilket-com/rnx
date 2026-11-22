//! Exercise the executable the guardian starts, including the public Cargo
//! message boundary between compilation and the program's own stdout.
use rnx_final::cache::{Lane, Route, claim};
use rnx_final::receipt::invocation_id;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

struct Temp(PathBuf);
impl Drop for Temp {
	fn drop(&mut self) {
		let _ = fs::remove_dir_all(&self.0);
	}
}

#[test]
fn cached_cargo_keeps_program_output_and_binds_the_actual_target_binary() {
	let root = std::env::temp_dir().join(format!("rnx-cargo-route-{}", invocation_id()));
	fs::create_dir(&root).unwrap();
	let _temp = Temp(root.clone());
	fs::create_dir(root.join("src")).unwrap();
	fs::write(
		root.join("Cargo.toml"),
		"[package]\nname='product'\nversion='0.1.0'\nedition='2024'\n",
	)
	.unwrap();
	fs::write(
		root.join("src/main.rs"),
		r#"fn main(){println!("{{\"reason\":\"build-finished\",\"success\":false}}");}
#[test] fn from_actual_cargo(){}"#,
	)
	.unwrap();
	let target = claim(&root.join("lane")).unwrap();
	let _lease = Lane::open(&target).unwrap();
	let receipts = root.join("cache-recipes");
	fs::create_dir(&receipts).unwrap();
	let mark = receipts.join("fixture.marks");
	let route = json!({"recipe":"fixture","target":target,"mark":mark});
	let tool = Path::new(env!("CARGO_BIN_EXE_rnx-final"));
	let command = |args: &[&str]| {
		let result = Command::new(tool)
			.current_dir(&root)
			.args(["cached-cargo", &route.to_string(), "cargo", "+1.98.1"])
			.args(args)
			.env("CARGO_TARGET_DIR", root.join("foreign-target"))
			.env("CARGO_BUILD_BUILD_DIR", root.join("foreign-build"))
			.output()
			.unwrap();
		assert!(
			result.status.success(),
			"{}",
			String::from_utf8_lossy(&result.stderr)
		);
		String::from_utf8(result.stdout).unwrap()
	};
	let version = Command::new("rustc")
		.args(["+1.98.1", "-vV"])
		.output()
		.unwrap();
	assert!(version.status.success());
	let host = std::str::from_utf8(&version.stdout)
		.unwrap()
		.lines()
		.find_map(|l| l.strip_prefix("host: "))
		.unwrap();
	assert_eq!(
		command(&["run", "--offline", "--target", host]).trim(),
		r#"{"reason":"build-finished","success":false}"#
	);
	let binding: Route = serde_json::from_value(route.clone()).unwrap();
	assert_eq!(
		binding.sole_binary().unwrap(),
		binding.binary("product").unwrap()
	);
	assert_eq!(
		binding.binary("product").unwrap(),
		target.join(host).join("debug/product")
	);
	assert!(command(&["test", "--offline", "--", "--list"]).contains("from_actual_cargo: test"));
	let marks: serde_json::Value = serde_json::from_slice(&fs::read(&mark).unwrap()).unwrap();
	assert!(
		marks
			.as_object()
			.unwrap()
			.keys()
			.any(|p| p.starts_with("debug/deps/product-"))
	);
	fs::write(
		root.join("src/main.rs"),
		"fn main() {} #[test] fn failed() { panic!() }",
	)
	.unwrap();
	let failed = Command::new(tool)
		.current_dir(&root)
		.args([
			"cached-cargo",
			&route.to_string(),
			"cargo",
			"+1.98.1",
			"test",
			"--offline",
		])
		.output()
		.unwrap();
	assert!(!failed.status.success());
	assert!(!mark.exists());
	assert!(target.join(".rnx-observed/fixture").is_file());
	assert!(!root.join("foreign-target").exists() && !root.join("foreign-build").exists());
}
