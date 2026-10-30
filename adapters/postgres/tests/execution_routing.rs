//! Routing is exercised through the real adapter command line.
#![cfg(unix)]
#[path = "../../../tests/support/adapter_routing.rs"]
mod routing;
#[test]
fn one_shot_routing_matches_legacy_and_never_retries_a_terminal_failure() {
	routing::verify(
		env!("CARGO_BIN_EXE_rnx-pg"),
		env!("CARGO_MANIFEST_DIR"),
		r#"pub async fn main(args){println!("effect-once");postgres::query("not a connection URL","SELECT 1",[],#{}).await.is_err()}"#,
	);
}
