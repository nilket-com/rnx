//! Explicit differential transcript: errors compare category, origin and message.
use super::*;

#[derive(Debug, PartialEq, Eq)]
struct Diagnostic {
	category: &'static str,
	origin: Option<Origin>,
	message: String,
}
fn diagnostic(f: Failure) -> Diagnostic {
	let (category, origin) = match &f {
		Failure::Compile { origin, .. } => ("compile", origin.clone()),
		Failure::Runtime { origin, .. } => ("runtime", origin.clone()),
		Failure::Refused(_) => ("refused", None),
		Failure::Budget(_) => ("budget", None),
		Failure::Interrupted => ("interrupted", None),
		Failure::OverCeiling { .. } => ("ceiling", None),
	};
	let message = match &f {
		Failure::Compile { message, .. } | Failure::Runtime { message, .. } => message.clone(),
		_ => f.to_string(),
	};
	Diagnostic {
		category,
		origin,
		message,
	}
}
enum Class {
	Equal,
	Wording {
		legacy: &'static str,
		execution: &'static str,
	},
	Policy {
		execution: &'static str,
	},
}
struct Row {
	input: &'static str,
	class: Class,
}
const ROWS: &[Row] = &[
	Row {
		input: "let n=7;let c=||n;let shared=[1];0",
		class: Class::Equal,
	},
	Row {
		input: "shared.push(2);c()",
		class: Class::Equal,
	},
	Row {
		input: "shared.len()",
		class: Class::Equal,
	},
	Row {
		input: "fn twice(n){n*2} const C=3; struct P{x} enum E{V(n)} let v=[1]; let alias=v; 0",
		class: Class::Equal,
	},
	Row {
		input: "alias.push(2); (v,twice(C),P{x:7},E::V(8))",
		class: Class::Equal,
	},
	Row {
		input: "fn twice(n){n*3} twice(2)",
		class: Class::Equal,
	},
	Row {
		input: "let stash=[]; let keep=19;0",
		class: Class::Equal,
	},
	Row {
		input: "stash.push(||42);panic!(\"failed\")",
		class: Class::Wording {
			legacy: "Panicked: failed",
			execution: "Panicked: failed\n  at input 8, line 1, column 18",
		},
	},
	Row {
		input: "stash[0]()",
		class: Class::Equal,
	},
	Row {
		input: "let new_binding=1;keep",
		class: Class::Equal,
	},
	Row {
		input: "let rejected=2;panic!(\"failed\")",
		class: Class::Wording {
			legacy: "Panicked: failed",
			execution: "Panicked: failed\n  at input 11, line 1, column 16",
		},
	},
	Row {
		input: "keep",
		class: Class::Equal,
	},
	Row {
		input: "let bad=||1/0;0",
		class: Class::Equal,
	},
	Row {
		input: "bad()",
		class: Class::Wording {
			legacy: "Division by zero in `i64` division (`/`)",
			execution: "Division by zero in `i64` division (`/`)\n  at input 13, line 1, column 11\n  at input 14, line 1, column 1",
		},
	},
	Row {
		input: ":renumber",
		class: Class::Equal,
	},
	Row {
		input: "bad()",
		class: Class::Wording {
			legacy: "Division by zero in `i64` division (`/`)",
			execution: "Division by zero in `i64` division (`/`)\n  at input 13 of numbering 1, line 1, column 11\n  at input 1, line 1, column 1",
		},
	},
	Row {
		input: ":budget",
		class: Class::Equal,
	},
	Row {
		input: "loop {}",
		class: Class::Equal,
	},
	Row {
		input: ":reset",
		class: Class::Equal,
	},
	Row {
		input: "7",
		class: Class::Equal,
	},
	Row {
		input: "let a=[1];let f=|n|n+1;0",
		class: Class::Equal,
	},
	Row {
		input: "a.push(2); async {1}.await",
		class: Class::Policy {
			execution: "refused: async input is not available in an execution session; start `rnx repl` without --engine=execution (input 3, line 1, column 21)",
		},
	},
	Row {
		input: ":reset",
		class: Class::Equal,
	},
	Row {
		input: "let a=[1];let f=|n|n+1;0",
		class: Class::Equal,
	},
	Row {
		input: "a.push(2); [1].iter().map(f).collect::<Vec>()",
		class: Class::Policy {
			execution: "refused: this method call is not available in an execution session; start `rnx repl` without --engine=execution (input 2, line 1, column 12)",
		},
	},
];
fn context() -> Context {
	let mut c = Context::with_default_modules().unwrap();
	crate::install_core(&mut c).unwrap();
	c
}
fn legacy(s: &mut Session, input: &str) -> std::result::Result<Option<String>, Diagnostic> {
	s.eval(input)
		.map(|v| {
			(!crate::runner::is_unit(&v)).then(|| {
				crate::format::render(&v, Some(&s.fields()), &crate::format::Limits::default())
			})
		})
		.map_err(diagnostic)
}
#[test]
fn differential_transcript() {
	let mut old = Session::new(context()).unwrap();
	let mut fresh = DomainSession::new(context(), DEFAULT_CEILING).unwrap();
	for row in ROWS {
		match row.input {
			":budget" => {
				old.budget = 1024;
				fresh.budget = 1024;
				continue;
			}
			":renumber" => {
				old.renumber();
				fresh.renumber();
				continue;
			}
			":reset" => {
				old.reset();
				fresh.reset();
				continue;
			}
			_ => {}
		}
		let a = legacy(&mut old, row.input);
		let b = fresh
			.eval_rendered(row.input, &crate::format::Limits::default())
			.map_err(diagnostic);
		for result in [&a, &b] {
			if let Err(d) = result {
				assert!(!d.message.contains("<memory>"), "{d:?}");
			}
		}
		match row.class {
			Class::Equal => assert_eq!(a, b, "{}", row.input),
			Class::Wording { legacy, execution } => {
				let a = a.expect_err(row.input);
				let b = b.expect_err(row.input);
				assert_eq!(
					(&a.category, &a.origin),
					(&b.category, &b.origin),
					"{}",
					row.input
				);
				assert_eq!(a.message, legacy);
				assert_eq!(b.message, execution);
			}
			Class::Policy { execution } => {
				assert!(a.is_ok(), "{a:?}");
				let b = b.expect_err(row.input);
				assert_eq!(b.category, "refused");
				assert_eq!(b.message, execution);
				assert_eq!(
					fresh
						.eval_rendered("a.len()", &crate::format::Limits::default())
						.unwrap(),
					Some("1".into())
				);
				assert_eq!(fresh.binding_names(), vec!["a", "f"]);
			}
		}
	}
}

#[test]
fn mapped_frames_cross_inputs_redefinition_renumber_and_reset() {
	let mut s = DomainSession::new(context(), DEFAULT_CEILING).unwrap();
	s.eval("fn fail(){1/0} let a=||fail();0").unwrap();
	s.eval("let b=||a();0").unwrap();
	let d = diagnostic(s.eval("b()").unwrap_err());
	assert_eq!(
		d.message,
		"Division by zero in `i64` division (`/`)\n  at input 1, line 1, column 11\n  at input 1, line 1, column 24\n  at input 2, line 1, column 9\n  at input 3, line 1, column 1"
	);
	s.eval("fn fail(){7} 0").unwrap();
	s.renumber();
	let d = diagnostic(s.eval("b()").unwrap_err());
	assert_eq!(
		d.message,
		"Division by zero in `i64` division (`/`)\n  at input 1 of numbering 1, line 1, column 11\n  at input 1 of numbering 1, line 1, column 24\n  at input 2 of numbering 1, line 1, column 9\n  at input 1, line 1, column 1"
	);
	s.reset();
	let d = diagnostic(s.eval("1/0").unwrap_err());
	assert_eq!(
		d.message,
		"Division by zero in `i64` division (`/`)\n  at input 1, line 1, column 1"
	);
}

#[test]
fn mapped_frames_cut_long_chain() {
	let mut s = DomainSession::new(context(), DEFAULT_CEILING).unwrap();
	let d = diagnostic(
		s.eval("fn f(n){if n==0 {1/0} else {f(n-1)}} f(40)")
			.unwrap_err(),
	);
	assert_eq!(d.message.matches("\n  at input ").count(), 32);
	assert!(d.message.ends_with("\n  [diagnostic truncated]"));
	assert!(!d.message.contains("<memory>"));
}

// Shared observations for ADR0246; the original transcript remains pinned above.
fn encode(r: std::result::Result<Option<String>, Diagnostic>) -> serde_json::Value {
	match r {
		Ok(value) => serde_json::json!({"outcome":"ok","value":value}),
		Err(d) => {
			serde_json::json!({"outcome":d.category,"message":d.message,"origin":d.origin.map(|o|serde_json::json!({"input":o.input,"line":o.line,"column":o.column}))})
		}
	}
}
pub(in crate::session) fn parity_old(old: &mut Session, input: &str) -> serde_json::Value {
	encode(legacy(old, input))
}
pub(in crate::session) fn parity_new(new: &mut DomainSession, input: &str) -> serde_json::Value {
	encode(
		new.eval_rendered(input, &crate::format::Limits::default())
			.map_err(diagnostic),
	)
}
pub(in crate::session) fn parity_observe(
	old: &mut Session,
	new: &mut DomainSession,
	input: &str,
) -> (serde_json::Value, serde_json::Value) {
	(parity_old(old, input), parity_new(new, input))
}

pub(in crate::session) fn parity_ledger() -> serde_json::Value {
	let rows:Vec<_>=ROWS.iter().map(|r|serde_json::json!({"input":r.input,"existing_class":match r.class {Class::Equal=>"Equal",Class::Wording{..}=>"Wording",Class::Policy{..}=>"Policy (historical fixture label, not a classification decision)"}})).collect();
	serde_json::json!({"source":"src/session/domain_session/transcripts.rs::ROWS","rows":rows})
}

// Test-only access for exact source-linked replay budgets.
pub(in crate::session) fn parity_budget(session: &mut DomainSession, budget: usize) {
	session.budget = budget;
}
