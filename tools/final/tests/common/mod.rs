//! Complete receipts for the controls' fake stages and selections, so the
//! fakes pass the same receipt types real stages do.
#![allow(dead_code)]
use serde_json::{Value, json};

/// A selection as select() writes one, with the given scope and stages.
pub fn selection(scope: &str, candidate: Value, stages: Value) -> Value {
	json!({
		"schema": 1, "scope": scope, "publication_eligible": scope == "required" || scope == "complete",
		"record": {"candidate": candidate, "baseline": "b", "plan": "p", "record": "9999", "phase": "impl", "paths": [], "required": []},
		"policy": "p", "source": "s", "root": "/fixture", "out": "/fixture/final", "stages": stages,
	})
}

/// A passing finalization, as finalize() writes one.
pub fn finalization() -> Value {
	json!({"schema": 1, "status": "PASS", "candidate": "c", "scope": "required", "publication_eligible": true, "source": "s", "selection": "x", "evidence": {}, "durations": {}})
}

/// A stage child's receipt, as child::run writes one.
pub fn stage_receipt(passed: bool, exit: i32, keys: Value, artifacts: Value) -> Value {
	json!({
		"schema": 1, "status": if passed { "PASS" } else { "FAIL" }, "command": ["command"], "process_exit": exit,
		"context": {"schema": 1, "invocation": "i", "root": "/fixture", "cwd": "/fixture", "command": ["command"], "candidate": "c", "source": "s", "environment": {"keys": keys, "sha256": "x"}, "kind": "leaf"},
		"seconds": 0.0, "cleanup_seconds": 0, "cleanup": [], "artifacts": artifacts,
	})
}

/// A passing finalization that binds the stages' evidence, as finalize() does.
pub fn finalization_of(evidence: &serde_json::Map<String, Value>) -> Value {
	let mut value = finalization();
	value["evidence"] = Value::Object(evidence.clone());
	value
}
