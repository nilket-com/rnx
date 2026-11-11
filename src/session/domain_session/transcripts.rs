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
			execution: "Panicked: failed\n  at <memory>:152..168",
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
			execution: "Panicked: failed\n  at <memory>:116..132",
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
			execution: "Division by zero in `i64` division (`/`)\n  at <memory>:90..93\n  at <memory>:130..135",
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
			execution: "Division by zero in `i64` division (`/`)\n  at <memory>:90..93\n  at <memory>:130..135",
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
