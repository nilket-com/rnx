//! ADR0256: pin the value of no-else forms, with separate effect/error controls.
use super::{classify, context};
use crate::session::{
	DEFAULT_CEILING, Session,
	domain_session::{
		DomainSession,
		transcripts::{parity_new, parity_old},
	},
};
use serde_json::{Value, json};
const CASES: &[(&str, &str)] = &[
	("if true", "let a=if true {7}; a"),
	("if false", "let a=if false {7}; a"),
	("else-if first", "let a=if true {7}else if true {8}; a"),
	("else-if second", "let a=if false {7}else if true {8}; a"),
	("else-if none", "let a=if false {7}else if false {8}; a"),
	("if-let match", "let a=if let Some(x)=Some(7){x}; a"),
	("if-let miss", "let a=if let Some(x)=None{x}; a"),
];
fn pair(source: &str) -> (Value, Value) {
	let mut old = Session::new(context()).unwrap();
	let mut new = DomainSession::new(context(), DEFAULT_CEILING).unwrap();
	let a = parity_old(&mut old, source);
	let b = parity_new(&mut new, source);
	old.close().unwrap();
	new.close().unwrap();
	(a, b)
}
pub(super) fn report() -> Value {
	let rows: Vec<_> = CASES
		.iter()
		.map(|(construct, input)| {
			let (a, b) = pair(input);
			let (class, rule, worse) = classify(&a, &b);
			json!({
				"construct": construct,
				"input": input,
				"legacy": a,
				"domain": b,
				"class": class,
				"wording_rule": rule,
				"domain_worse": worse,
			})
		})
		.collect();
	json!(rows)
}
#[test]
fn no_else_session_controls() {
	for (_, source) in CASES {
		let (a, b) = pair(source);
		assert_eq!(a, json!({"outcome":"ok","value":null}), "legacy {source}");
		assert_eq!(a, b, "{source}");
	}
	for (source, value) in [
		("let n=0;let a=if true {n+=1;7};(a,n)", "((), 1)"),
		(
			"let n=0;let a=if let Some(x)=Some(7) {n+=1;x};(a,n)",
			"((), 1)",
		),
		("let a=if true {7}else{8};a", "7"),
		("let a=if false {7}else{8};a", "8"),
		("let a=if false {7}else if true {8}else{9};a", "8"),
		("let a=if let Some(x)=Some(7) {x}else{8};a", "7"),
		("let a=if let Some(x)=None {x}else{8};a", "8"),
	] {
		let (a, b) = pair(source);
		assert_eq!(a, json!({"outcome":"ok","value":value}));
		assert_eq!(a, b, "{source}");
	}
	for expression in [
		"if true {seen.push(1);panic!(\"noelse\");7}",
		"if let Some(x)=Some(7) {seen.push(1);panic!(\"noelse\");x}",
	] {
		let mut old = Session::new(context()).unwrap();
		let mut new = DomainSession::new(context(), DEFAULT_CEILING).unwrap();
		let init = "let seen=[];";
		assert_eq!(parity_old(&mut old, init), parity_new(&mut new, init));
		let source = format!("let a={expression};a");
		let a = parity_old(&mut old, &source);
		let b = parity_new(&mut new, &source);
		assert_eq!(a["outcome"], "runtime");
		assert_eq!(b["outcome"], "runtime");
		assert!(a["message"].as_str().unwrap().contains("noelse"));
		assert!(b["message"].as_str().unwrap().contains("noelse"));
		let a = parity_old(&mut old, "seen");
		let b = parity_new(&mut new, "seen");
		assert_eq!(a, json!({"outcome":"ok","value":"[1]"}));
		assert_eq!(a, b);
		old.close().unwrap();
		new.close().unwrap();
	}
}
