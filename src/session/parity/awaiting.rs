//! Observations of the explicit direct-await boundary; no external requests.
use super::*;
use crate::session::{
	Session,
	domain_session::{DomainSession, transcripts::parity_observe},
};
fn contexts() -> crate::routing_inventory::Result<(DomainSession, Session)> {
	fn make() -> crate::Result<(
		rune::Context,
		crate::http::State,
		crate::lifecycle::Lifecycle,
	)> {
		let mut c = rune::Context::with_default_modules()?;
		crate::install_core(&mut c)?;
		crate::time::install(&mut c)?;
		let http = crate::http::State::default();
		let life = crate::lifecycle::Lifecycle::new(true)?;
		crate::http::install(&mut c, &http, life.scope("http"))?;
		Ok((c, http, life))
	}
	let (c, h, l) = make()?;
	let fresh = DomainSession::for_frontend(
		c,
		crate::session::DEFAULT_CEILING,
		crate::session::remedy::FrontEnd::Terminal,
	)?
	.with_http(h)
	.with_lifecycle(l);
	let (c, h, l) = make()?;
	let old = Session::new(c)?.with_http(h).with_lifecycle(l);
	Ok((fresh, old))
}
pub(crate) fn host(
	function: &str,
	args: &str,
	bytes: bool,
) -> crate::routing_inventory::Result<Vec<serde_json::Value>> {
	let (mut fresh, mut old) = contexts()?;
	let mut rows = Vec::new();
	for awaited in [false, true] {
		let input = format!("{function}({args}){}", if awaited { ".await" } else { "" });
		let (legacy, domain) = parity_observe(&mut old, &mut fresh, &input);
		let (class, wording_rule, domain_worse) = if bytes {
			if domain["outcome"] == "refused"
				&& domain["message"]
					.as_str()
					.is_some_and(|s| s.contains(&format!("{function} needs bytes")))
			{
				("Refused for a host function", None, false)
			} else {
				classify(&legacy, &domain)
			}
		} else {
			classify(&legacy, &domain)
		};
		rows.push(serde_json::json!({"function":function,"input":input,"legacy":legacy,"domain":domain,"class":class,"wording_rule":wording_rule,"domain_worse":domain_worse,"construct":function,"work_group":if bytes{"bytes"}else{"direct host await"}}));
	}
	old.close()?;
	fresh.close()?;
	Ok(rows)
}

// Both skipped and read moved fields are observed independently of classification.
pub(super) const MOVED_OPTIONS: &[(&str, &str)] = &[
	(
		"unknown-bad-url",
		"let moved=[1];let options=#{unknown:moved};drop(moved);http::request(\"GET\",\"not a url\",options).await",
	),
	(
		"unknown-good-url",
		"let moved=[1];let options=#{unknown:moved};drop(moved);http::request(\"GET\",\"http://127.0.0.1/\",options).await",
	),
	(
		"read-headers",
		"let moved=#{};let options=#{headers:moved};drop(moved);http::request(\"GET\",\"http://127.0.0.1/\",options).await",
	),
	(
		"read-body",
		"let moved=String::from(\"body\");let options=#{body:moved};drop(moved);http::request(\"GET\",\"http://127.0.0.1/\",options).await",
	),
];
pub(super) fn options_report() -> serde_json::Value {
	let mut rows = Vec::new();
	for (construct, input) in MOVED_OPTIONS.iter().copied().chain([
		(
			"unknown-live-bad-url",
			"http::request(\"GET\",\"not a url\",#{unknown:[1]}).await",
		),
		(
			"unknown-live-good-url",
			"http::request(\"GET\",\"http://127.0.0.1/\",#{unknown:[1]}).await",
		),
	]) {
		let (mut fresh, mut old) = contexts().unwrap();
		let (legacy, domain) = parity_observe(&mut old, &mut fresh, input);
		let (class, wording_rule, domain_worse) = classify(&legacy, &domain);
		rows.push(serde_json::json!({"function":"http::request","construct":construct,"input":input,"legacy":legacy,"domain":domain,"class":class,"wording_rule":wording_rule,"domain_worse":domain_worse,"reason":"A by-snapshot host argument is read in full at the call; a moved descendant is an access error before the handler and any effects. Live skipped fields retain parser error selection."}));
		fresh.close().unwrap();
		old.close().unwrap();
	}
	serde_json::json!(rows)
}
#[test]
fn options_observations() {
	let rows = options_report();
	if let Some(path) = std::env::var_os("RNX_OPTIONS_REPORT") {
		std::fs::write(path, serde_json::to_vec_pretty(&rows).unwrap()).unwrap();
	}
	assert_eq!(rows.as_array().unwrap().len(), 6);
}

// Exact diagnostic pairs approved by the snapshot access rule (seq31173).
const MOVED_PAIRS: &[(&str, &str, usize)] = &[
	(
		"let moved=[1];let options=#{unknown:moved};drop(moved);http::request(\"GET\",\"not a url\",options).await",
		"Err(\"cannot get not a url: invalid URL: relative URL without a base\")",
		56,
	),
	(
		"let moved=[1];let options=#{unknown:moved};drop(moved);http::request(\"GET\",\"http://127.0.0.1/\",options).await",
		"Err(\"cannot get http://127.0.0.1/: unknown option \\\"unknown\\\"\")",
		56,
	),
	(
		"let moved=#{};let options=#{headers:moved};drop(moved);http::request(\"GET\",\"http://127.0.0.1/\",options).await",
		"Err(\"cannot get http://127.0.0.1/: headers must be an object\")",
		56,
	),
	(
		"let moved=String::from(\"body\");let options=#{body:moved};drop(moved);http::request(\"GET\",\"http://127.0.0.1/\",options).await",
		"Err(\"cannot get http://127.0.0.1/: body must be String or Bytes\")",
		70,
	),
];
fn moved_pair(value: &str, column: usize) -> (serde_json::Value, serde_json::Value) {
	(
		serde_json::json!({"outcome":"ok","value":value}),
		serde_json::json!({"outcome":"runtime","message":format!("Cannot read the value: it has been moved\n  at input 1, line 1, column {column}"),"origin":{"input":1,"line":1,"column":column}}),
	)
}
pub(super) fn reviewed_moved_pair(a: &serde_json::Value, b: &serde_json::Value) -> bool {
	MOVED_PAIRS.iter().any(|(_, value, column)| {
		let (old, new) = moved_pair(value, *column);
		*a == old && *b == new
	})
}
#[test]
fn moved_option_rule_is_exact() {
	for (_, value, column) in MOVED_PAIRS {
		let (a, b) = moved_pair(value, *column);
		assert_eq!(
			classify(&a, &b),
			(
				"Intentional difference",
				Some("snapshot-moved-argument"),
				false
			)
		);
		for key in ["outcome", "message", "origin"] {
			let mut changed = b.clone();
			changed[key] = serde_json::Value::Null;
			assert_ne!(classify(&a, &changed).1, Some("snapshot-moved-argument"));
		}
		let mut changed = a.clone();
		changed["value"] = "Err(\"unreviewed\")".into();
		assert_eq!(classify(&changed, &b).0, "Domain-only failure");
	}
}
