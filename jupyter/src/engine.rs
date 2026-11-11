//! Process-sticky kernel selection; never selected by a cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Engine {
	#[default]
	Legacy,
	Execution,
}
impl Engine {
	pub fn parse(s: &str) -> Result<Self, String> {
		match s {
			"legacy" => Ok(Self::Legacy),
			"execution" => Ok(Self::Execution),
			_ => Err("engine must be legacy or execution".into()),
		}
	}
	pub fn kernel_name(self) -> &'static str {
		match self {
			Self::Legacy => "rnx",
			Self::Execution => "rnx-execution",
		}
	}
	pub fn display_name(self) -> &'static str {
		match self {
			Self::Legacy => "Rune (rnx)",
			Self::Execution => "Rune (rnx execution; sync only)",
		}
	}
	pub(crate) fn accepts(self, ready: &serde_json::Value) -> bool {
		match self {
			Self::Legacy => ready.get("engine").is_none() || ready["engine"] == "legacy",
			Self::Execution => {
				ready["engine"] == "execution"
					&& ready["capabilities"] == serde_json::json!({"html":false,"async":false})
			}
		}
	}
}

#[cfg(test)]
mod tests {
	#[test]
	fn readiness_engine_and_capabilities_fail_closed() {
		use super::Engine;
		use serde_json::json;
		assert!(Engine::Legacy.accepts(&json!({})));
		assert!(!Engine::Execution.accepts(&json!({})));
		let ready = json!({"engine":"execution","capabilities":{"html":false,"async":false}});
		assert!(Engine::Execution.accepts(&ready));
		assert!(!Engine::Legacy.accepts(&ready));
		for field in ["html", "async"] {
			let mut wrong = ready.clone();
			wrong["capabilities"][field] = json!(true);
			assert!(!Engine::Execution.accepts(&wrong));
		}
		for unknown in ["", "auto", "Execution", "other"] {
			assert!(Engine::parse(unknown).is_err());
			assert!(!Engine::Legacy.accepts(&json!({"engine":unknown})));
		}
	}
}
