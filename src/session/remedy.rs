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

pub(super) fn script_await(
	caller: &str,
	function: &str,
	as_value: bool,
	remove_await: bool,
) -> String {
	let reason = if remove_await {
		format!("{function} is not asynchronous; remove .await and call it as {function}(...)")
	} else {
		await_call(function, as_value)
	};
	if caller == "main" {
		reason
	} else {
		format!("{caller}: {reason}")
	}
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

	#[test]
	fn engine_and_session_script_await_wording_cannot_drift() {
		for (source, as_value, remove_await) in [
			("async fn f(){7} fn g(){f()}", false, false),
			("async fn f(){7} fn g(){f}", true, false),
			("fn f(){7} async fn g(){f().await}", false, true),
		] {
			let context = crate::routing_inventory::session_parity_context().unwrap();
			let mut domain = rune::experimental_core::domain::ProgramDomain::new(
				Default::default(),
				Default::default(),
				context.execution_runtime().unwrap(),
			)
			.unwrap();
			let mut sources = rune::Sources::new();
			sources
				.insert(
					rune::Source::memory(format!("{source} pub fn main(s){{let r=7;(#{{}},r)}}"))
						.unwrap(),
				)
				.unwrap();
			let mut diagnostics = rune::Diagnostics::new();
			assert!(
				domain
					.compile_session(&context, &mut sources, &mut diagnostics, 1, &[], &[])
					.is_err()
			);
			let errors: Vec<_> = diagnostics
				.diagnostics()
				.iter()
				.filter_map(|d| match d {
					rune::diagnostics::Diagnostic::Fatal(f) => {
						let error = f.as_compile_error()?;
						let (item, caller, value, remove) = error.script_await_refusal()?;
						Some((error, item, caller, value, remove))
					}
					_ => None,
				})
				.collect();
			assert_eq!(errors.len(), 1, "{diagnostics:?}");
			let (error, item, caller, value, remove) = errors[0];
			assert_eq!((value, remove), (as_value, remove_await));
			assert_eq!(item.to_string(), "f");
			assert_eq!(caller.to_string(), "g");
			assert_eq!(error.to_string(), script_await("g", "f", value, remove));
			domain.close().unwrap();
		}
	}
}
