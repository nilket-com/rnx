//! ADR 0284's golden fixtures (`tests/witness`). Each was generated once
//! from the published Python controller at `WITNESS`, by the method
//! recorded in it, with placeholders for paths; tests compare against
//! them, and they are never regenerated from this crate's own output. A
//! later record that extends one says so in its `successors`.
use serde_json::Value;
use std::path::Path;

pub const WITNESS: &str = "2b7323ae20325370e9d57d3862f5ebc1d9e51ad8";

/// The expected value of fixture `name`, each placeholder replaced by its
/// real text.
pub fn expected(name: &str, substitutions: &[(&str, &str)]) -> Value {
	let path = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("tests/witness")
		.join(format!("{name}.json"));
	let fixture: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
	assert_eq!(fixture["witness"], WITNESS, "{name}");
	assert!(
		fixture["method"].as_str().is_some_and(|m| !m.is_empty()),
		"{name}: no method"
	);
	// A reviewed later record may extend a fixture; each says what it added.
	for successor in fixture["successors"].as_array().into_iter().flatten() {
		assert!(
			successor["record"].as_str().is_some_and(|r| r.len() == 4)
				&& successor["change"].as_str().is_some_and(|c| c.len() > 40),
			"{name}: a successor names its record and its change"
		);
	}
	let mut text = fixture["expected"].to_string();
	for (mark, real) in substitutions {
		let escaped = serde_json::to_string(real).unwrap();
		text = text.replace(mark, &escaped[1..escaped.len() - 1]);
	}
	serde_json::from_str(&text).unwrap()
}
