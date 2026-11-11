#![cfg(unix)]
#[test]
fn worker_engine_comparison_table() {
	let result = std::process::Command::new("timeout")
		.args(["--kill-after=2s", "120s", "python3", "-B"])
		.arg(concat!(
			env!("CARGO_MANIFEST_DIR"),
			"/tests/worker_execution.py"
		))
		.arg(env!("CARGO_BIN_EXE_rnx"))
		.args(if cfg!(feature = "test-support") {
			vec!["--test-support"]
		} else {
			vec![]
		})
		.env_remove("RNX_MEMORY_CEILING")
		.env_remove("RNX_CONFIG")
		.output()
		.unwrap();
	assert!(
		result.status.success(),
		"{}\n{}",
		String::from_utf8_lossy(&result.stdout),
		String::from_utf8_lossy(&result.stderr)
	);
}
