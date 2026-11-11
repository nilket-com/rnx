//! Engine-preserving dependency handovers use actual PTY input and exec arguments.
#![cfg(all(unix, feature = "test-support"))]
#[test]
fn dependency_handover_preserves_engine_and_precommit_state() {
	let output = std::process::Command::new("timeout")
		.args(["--kill-after=2s", "180s", "python3"])
		.arg(concat!(
			env!("CARGO_MANIFEST_DIR"),
			"/tests/fixtures/dep_engine.py"
		))
		.arg(env!("CARGO_BIN_EXE_rnx"))
		.arg(if cfg!(feature = "stock-management") {
			"project"
		} else {
			"scratch"
		})
		.output()
		.unwrap();
	assert!(output.status.success(), "{output:?}");
}
