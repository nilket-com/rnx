//! Recovery wording belongs to the front end that created the session.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum FrontEnd {
	Terminal,
	Notebook,
	#[default]
	Neutral,
}

impl FrontEnd {
	pub(crate) fn parse(value: &str) -> Result<Self, String> {
		match value {
			"terminal" => Ok(Self::Terminal),
			"notebook" => Ok(Self::Notebook),
			"neutral" => Ok(Self::Neutral),
			_ => Err("worker frontend must be terminal, notebook, or neutral".into()),
		}
	}
	pub(super) fn fallback(self, reason: &str) -> String {
		let remedy = match self {
			Self::Terminal => "start `rnx repl` without --engine=execution".to_owned(),
			Self::Notebook => format!(
				"select the `{}` kernel",
				crate::kernel_name::LEGACY_KERNEL_DISPLAY_NAME
			),
			Self::Neutral => "use the legacy session engine".to_owned(),
		};
		format!("{reason}; {remedy}")
	}
}

pub(super) fn await_call(host: &str, as_value: bool) -> String {
	let reason = if as_value {
		"; futures as values are not available"
	} else {
		""
	};
	format!("{host} is asynchronous{reason}; await its call as {host}(...).await")
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::session::{DEFAULT_CEILING, Failure, INPUT_CAP, domain_session::DomainSession};

	fn session(frontend: FrontEnd) -> DomainSession {
		DomainSession::for_frontend(
			crate::routing_inventory::session_parity_context().unwrap(),
			DEFAULT_CEILING,
			frontend,
		)
		.unwrap()
	}
	fn refused(session: &mut DomainSession, source: &str) -> String {
		let error = session
			.eval_rendered(source, &crate::format::Limits::default())
			.unwrap_err();
		assert!(matches!(error, Failure::Refused(_)), "{error}");
		error.to_string()
	}
	#[test]
	fn each_switch_path_uses_its_declared_frontend_and_keeps_the_origin() {
		let remedies = [
			(
				FrontEnd::Terminal,
				"start `rnx repl` without --engine=execution",
			),
			(FrontEnd::Notebook, "select the `Rune (rnx)` kernel"),
			(FrontEnd::Neutral, "use the legacy session engine"),
		];
		for (source, reason) in [
			("async {7}.await", "an async block is not available"),
			("http::get_bytes(\"\")", "http::get_bytes needs bytes"),
			(
				"const A=b\"ab\";A",
				"execution session does not support byte-string constants",
			),
		] {
			for (frontend, expected) in remedies {
				let mut s = session(frontend);
				let text = refused(&mut s, source);
				assert!(text.contains(reason), "{text}");
				assert!(text.contains(" (input 1, line 1, column "), "{text}");
				assert!(text.contains(expected), "{text}");
				for (other, remedy) in remedies {
					if other != frontend {
						assert!(!text.contains(remedy), "{text}");
					}
				}
				s.reset_fallible().unwrap();
				assert_eq!(refused(&mut s, source), text);
				s.close().unwrap();
			}
		}
	}
	#[test]
	fn local_remedies_are_identical_in_every_frontend() {
		for source in [
			"time::sleep(0)",
			"let f=time::sleep;0",
			"mod elsewhere;",
			&" ".repeat(INPUT_CAP + 1),
		] {
			let mut observed = Vec::new();
			for frontend in [FrontEnd::Terminal, FrontEnd::Notebook, FrontEnd::Neutral] {
				let mut s = session(frontend);
				observed.push(refused(&mut s, source));
				s.close().unwrap();
			}
			assert_eq!(observed[0], observed[1]);
			assert_eq!(observed[1], observed[2]);
		}
		let mut observed = Vec::new();
		for frontend in [FrontEnd::Terminal, FrontEnd::Notebook, FrontEnd::Neutral] {
			let mut s = session(frontend);
			s.eval_rendered("struct RemedyShape {x}", &crate::format::Limits::default())
				.unwrap();
			let error = refused(&mut s, "struct RemedyShape {y}");
			assert!(error.contains(":reset before redeclaring it"), "{error}");
			observed.push(error);
			s.close().unwrap();
		}
		assert_eq!(observed[0], observed[1]);
		assert_eq!(observed[1], observed[2]);
	}
	#[test]
	fn default_embedding_remedy_names_an_existing_engine_choice() {
		let mut s = DomainSession::new(
			crate::routing_inventory::session_parity_context().unwrap(),
			DEFAULT_CEILING,
		)
		.unwrap();
		let error = refused(&mut s, "async {7}.await");
		assert!(error.contains("use the legacy session engine"), "{error}");
		s.close().unwrap();
		// This is the same legacy branch selected by the worker's existing
		// --engine=legacy option, not a hypothetical embedding constructor.
		let mut legacy = crate::session::Session::new(
			crate::routing_inventory::session_parity_context().unwrap(),
		)
		.unwrap();
		assert_eq!(
			legacy
				.eval("async {7}.await")
				.unwrap()
				.as_integer::<i64>()
				.unwrap(),
			7
		);
		legacy.close().unwrap();
	}
}
