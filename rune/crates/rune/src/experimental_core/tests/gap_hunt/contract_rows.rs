//! Compare independent rows without losing the rows after a disagreement.
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::string::String;
use std::vec::Vec;

#[derive(Default)]
pub(super) struct ContractRows {
	checked: usize,
	differences: Vec<serde_json::Value>,
}

#[derive(Default)]
pub(super) struct RowContext {
	evidence: Option<serde_json::Value>,
}

impl RowContext {
	pub(super) fn observations(
		&mut self,
		legacy: &serde_json::Value,
		execution: &serde_json::Value,
	) {
		self.evidence = Some(serde_json::json!({"legacy":legacy,"execution":execution}));
	}
}

impl ContractRows {
	pub(super) fn check(&mut self, label: impl AsRef<str>, check: impl FnOnce(&mut RowContext)) {
		self.checked += 1;
		let mut context = RowContext::default();
		if let Err(payload) = catch_unwind(AssertUnwindSafe(|| check(&mut context))) {
			let message = payload
				.downcast_ref::<String>()
				.map(String::as_str)
				.or_else(|| payload.downcast_ref::<&str>().copied())
				.unwrap_or("non-string comparison panic");
			self.differences.push(serde_json::json!({
				"row":label.as_ref(),"difference":message,"observations":context.evidence,
			}));
		}
	}

	fn report(&self) -> serde_json::Value {
		serde_json::json!({"rows_checked":self.checked,"differences":self.differences})
	}

	pub(super) fn finish(self) {
		let report = self.report();
		if let Some(path) = std::env::var_os("RNX_CONTRACT_DIFFS") {
			std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
		}
		assert!(
			self.differences.is_empty(),
			"contract row differences (assertion left/right retain their original meanings):\n{}",
			serde_json::to_string_pretty(&report).unwrap(),
		);
	}
}

#[test]
fn all_differing_rows_are_reported_after_the_last_row() {
	let mut rows = ContractRows::default();
	rows.check("first", |_| assert_eq!(1, 2));
	rows.check("second", |_| assert_eq!("actual", "expected"));
	let mut reached_last = false;
	rows.check("last", |_| reached_last = true);
	assert!(reached_last);
	let report = rows.report();
	assert_eq!(report["rows_checked"], 3);
	let differences = report["differences"].as_array().unwrap();
	assert_eq!(differences.len(), 2);
	assert_eq!(differences[0]["row"], "first");
	assert_eq!(differences[1]["row"], "second");
	assert!(differences[0]["difference"]
		.as_str()
		.unwrap()
		.contains("left: 1"));
	assert!(differences[0]["difference"]
		.as_str()
		.unwrap()
		.contains("right: 2"));
	let failure = catch_unwind(AssertUnwindSafe(|| rows.finish()));
	assert!(failure.is_err());
}
