#![cfg(unix)]
#[test]
fn frontend_options_are_validated_before_control_endpoints_open() {
	for options in [
		vec!["--frontend=unknown"],
		vec!["--frontend=notebook", "--frontend=neutral"],
		vec!["--engine=execution", "--engine=legacy"],
	] {
		let result = std::process::Command::new(env!("CARGO_BIN_EXE_rnx"))
			.args(["worker", "--control-read", "0", "--control-write", "1"])
			.args(options)
			.output()
			.unwrap();
		assert_eq!(result.status.code(), Some(2), "{result:?}");
		assert!(!String::from_utf8_lossy(&result.stderr).contains("cannot open worker control"));
	}
}
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
