//! The same bounded parent drives an actual worker, not a mocked Session.
#[test]
fn worker_protocol_and_lifecycle() {
	let python = if cfg!(windows) { "python" } else { "python3" };
	let result = std::process::Command::new(python)
		.arg(concat!(
			env!("CARGO_MANIFEST_DIR"),
			"/tests/worker_checks.py"
		))
		.arg(env!("CARGO_BIN_EXE_rnx"))
		.args(if cfg!(feature = "test-support") {
			vec!["--test-support"]
		} else {
			vec![]
		})
		.env_remove("RNX_MEMORY_CEILING")
		.env_remove("RNX_CONFIG")
		.env_remove("RNX_TEST_WORKER_PAUSE")
		.env("PYTHONDONTWRITEBYTECODE", "1")
		.output()
		.expect("worker gates require Python 3 for the inherited-pipe parent fixture");
	assert!(
		result.status.success(),
		"{}\n{}",
		String::from_utf8_lossy(&result.stdout),
		String::from_utf8_lossy(&result.stderr)
	);
}
