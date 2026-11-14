//! ADR0275: observations and exact reviewed moved-snapshot pairs.
use super::super::domain_session::transcripts::parity_observe;
use serde_json::{Value, json};
pub(crate) const MOVED: &[(&str, &str, &str)] = &[
	(
		"time-ignored",
		"let v=[1];let p=#{year:1970,month:1,day:1,ignored:v};drop(v);time::from_parts(p,\"UTC\")",
		"Ok(0)",
	),
	(
		"time-read",
		"let v=[1];let p=#{year:v,month:1,day:1};drop(v);time::from_parts(p,\"UTC\")",
		"time field \"year\" must be an integer: Expected number type, but found `std::vec::Vec`",
	),
	(
		"process-empty",
		"let v=[1];let p=#{unknown:v};drop(v);process::run(\"\",[],p)",
		"cannot run : program is empty",
	),
	(
		"process-ignored",
		"let v=[1];let p=#{unknown:v};drop(v);process::run(\"/0275-missing\",[],p)",
		"cannot run /0275-missing: unknown option \"unknown\"",
	),
	(
		"process-args",
		"let v=String::from(\"value\");let a=[v];drop(v);process::run(\"/0275-missing\",a,#{})",
		"cannot run /0275-missing: argument 1 must be a string",
	),
	(
		"process-env",
		"let v=String::from(\"value\");let p=#{env:#{K:v}};drop(v);process::run(\"/0275-missing\",[],p)",
		"cannot run /0275-missing: environment variable \"K\": value must be a string or None",
	),
	(
		"response-earlier-error",
		"let v=[\"a\"];let h=#{\"x\":v};drop(v);web::response(199,\"body\",\"text/plain\",h)",
		"response status wants 200 to 599",
	),
	(
		"response-header",
		"let v=[\"a\"];let h=#{\"x\":v};drop(v);web::response(200,\"body\",\"text/plain\",h)",
		"web::response header \"x\" value 0: web: header values want String, Bytes or a vector",
	),
];
fn pair(input: &str, message: &str) -> (Value, Value) {
	let rendered = if message == "Ok(0)" {
		message.to_owned()
	} else {
		let value = rune::to_value(Err::<(), _>(message.to_owned())).unwrap();
		crate::format::render(&value, None, &crate::format::Limits::default())
	};
	let host = if input.contains("time::from_parts") {
		"time::from_parts"
	} else if input.contains("web::response") {
		"web::response"
	} else {
		"process::run"
	};
	let column = input.find(host).unwrap() + 1;
	(
		json!({"outcome":"ok","value":rendered}),
		json!({"outcome":"runtime","message":format!("Cannot read the value: it has been moved\n  at input 1, line 1, column {column}"),"origin":{"input":1,"line":1,"column":column}}),
	)
}
pub(super) fn reviewed_moved_pair(a: &Value, b: &Value) -> bool {
	MOVED.iter().any(|(_, input, message)| {
		let (old, new) = pair(input, message);
		*a == old && *b == new
	})
}
pub(crate) const LEAVES: &[(&str, &str)] = &[
	(
		"http-leaf",
		"let v=[1];let options=#{unknown:Ok(v)};drop(v);http::request(\"GET\",\"not a url\",options).await",
	),
	(
		"time-leaf-ignored",
		"let v=[1];let p=#{year:1970,month:1,day:1,ignored:Ok(v)};drop(v);time::from_parts(p,\"UTC\")",
	),
	(
		"time-leaf-read",
		"let v=[1];let p=#{year:Ok(v),month:1,day:1};drop(v);time::from_parts(p,\"UTC\")",
	),
	(
		"process-leaf-ignored",
		"let v=[1];let p=#{unknown:Ok(v)};drop(v);process::run(\"/0275-missing\",[],p)",
	),
	(
		"process-leaf-read",
		"let v=[1];let p=#{input:Ok(v)};drop(v);process::run(\"/0275-missing\",[],p)",
	),
	(
		"response-leaf-skipped",
		"let v=[1];let h=#{\"x\":Ok(v)};drop(v);web::response(199,\"body\",\"text/plain\",h)",
	),
	(
		"response-leaf-read",
		"let v=[1];let h=#{\"x\":Ok(v)};drop(v);web::response(200,\"body\",\"text/plain\",h)",
	),
];

pub(super) fn report() -> Value {
	let mut rows = Vec::new();
	for (name, input) in MOVED
		.iter()
		.map(|(name, input, _)| (*name, *input))
		.chain(LEAVES.iter().copied())
		.chain([
			("time-root", "time::from_parts([],\"UTC\")"),
			(
				"time-live-ignored",
				"time::from_parts(#{year:1970,month:1,day:1,ignored:||1},\"UTC\")",
			),
			(
				"time-wrong-field",
				"time::from_parts(#{year:[1],month:1,day:1},\"UTC\")",
			),
			(
				"time-roundtrip",
				"time::from_parts(time::parts(0,\"UTC\").unwrap(),\"UTC\")",
			),
			(
				"response-owned",
				"web::response(200,\"body\",\"text/plain\",#{\"x\":[\"a\",\"b\"]})",
			),
			(
				"response-bytes",
				"web::response(200,b\"body\",\"text/plain\",#{})",
			),
			(
				"process-live-unknown",
				"process::run(\"/0275-missing\",[],#{unknown:||1})",
			),
		]) {
		let (mut new, mut old) = super::awaiting::contexts().unwrap();
		let (a, b) = parity_observe(&mut old, &mut new, input);
		let (class, rule, worse) = super::classify(&a, &b);
		rows.push(json!({"construct":name,"input":input,"legacy":a,"domain":b,"class":class,"wording_rule":rule,"domain_worse":worse}));
		old.close().unwrap();
		new.close().unwrap();
	}
	json!(rows)
}
#[test]
fn observations() {
	let report = report();
	if let Some(path) = std::env::var_os("RNX_PARITY_REPORT") {
		std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap()
	}
	let http = report
		.as_array()
		.unwrap()
		.iter()
		.find(|r| r["construct"] == "http-leaf")
		.unwrap();
	assert!(
		http["legacy"]["value"]
			.as_str()
			.unwrap()
			.contains("invalid URL: relative URL without a base")
	);
	for row in report.as_array().unwrap() {
		assert!(
			!matches!(
				row["class"].as_str(),
				Some("Different result" | "Domain-only failure" | "Unreviewed wording")
			),
			"{row}"
		)
	}
}
#[test]
fn moved_rule_rejects_changed_message_or_origin() {
	for (_, input, message) in MOVED {
		let (a, b) = pair(input, message);
		assert!(reviewed_moved_pair(&a, &b));
		let mut changed = b.clone();
		changed["message"] = format!("{} extra", b["message"].as_str().unwrap()).into();
		assert!(!reviewed_moved_pair(&a, &changed));
		let mut changed = b;
		changed["origin"]["column"] = 0.into();
		assert!(!reviewed_moved_pair(&a, &changed));
	}
}
