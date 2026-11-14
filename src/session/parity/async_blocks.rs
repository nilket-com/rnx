//! Direct-block observations and exact future-value policy pairs.
use super::classify;
use crate::session::{
	DEFAULT_CEILING, Session,
	domain_session::{DomainSession, transcripts::parity_observe},
};

fn future_pairs() -> [(serde_json::Value, serde_json::Value); 2] {
	[
		(
			serde_json::json!({"outcome":"ok","value":"<::std::future::Future>"}),
			serde_json::json!({"outcome":"refused","message":"refused: async block futures as values are not available; await the block directly as async { ... }.await (input 1, line 1, column 1)","origin":null}),
		),
		(
			serde_json::json!({"outcome":"ok","value":"<::std::future::Future>"}),
			serde_json::json!({"outcome":"refused","message":"refused: async move block futures as values are not available; await the block directly as async move { ... }.await (input 1, line 1, column 1)","origin":null}),
		),
	]
}
pub(super) fn reviewed_future_refusal(a: &serde_json::Value, b: &serde_json::Value) -> bool {
	future_pairs()
		.into_iter()
		.any(|(old, new)| *a == old && *b == new)
}
#[test]
fn block_future_classification_accepts_only_the_exact_observed_pairs() {
	for (old, new) in future_pairs() {
		assert_eq!(
			classify(&old, &new),
			(
				"Intentional difference",
				Some("direct-block-await-required"),
				false
			)
		);
		for key in ["outcome", "value"] {
			let mut changed = old.clone();
			changed[key] = serde_json::Value::Null;
			assert!(!reviewed_future_refusal(&changed, &new));
			assert_ne!(
				classify(&changed, &new).1,
				Some("direct-block-await-required")
			);
		}
		for key in ["outcome", "message", "origin"] {
			let mut changed = new.clone();
			changed[key] = if key == "origin" {
				serde_json::json!({"input":1,"line":1,"column":1})
			} else {
				serde_json::Value::Null
			};
			assert!(!reviewed_future_refusal(&old, &changed));
			assert_ne!(
				classify(&old, &changed).1,
				Some("direct-block-await-required")
			);
		}
	}
}
pub(super) fn report() -> serde_json::Value {
	let mut rows = Vec::new();
	for (name, source) in [
		(
			"return-boundary",
			"async{let x=async{return 7;99}.await;x+1}.await",
		),
		(
			"capture-bindings",
			"let n=1;let v=[1];let w=v;let r=async{n=2;v.push(2);n}.await;(n,v,w,r)",
		),
		("future-value", "async {7}"),
		("move-future-value", "async move {7}"),
	] {
		let mut old = Session::new(super::context()).unwrap();
		let mut new = DomainSession::for_frontend(
			super::context(),
			DEFAULT_CEILING,
			crate::session::remedy::FrontEnd::Terminal,
		)
		.unwrap();
		let (legacy, domain) = parity_observe(&mut old, &mut new, source);
		let (class, rule, worse) = classify(&legacy, &domain);
		rows.push(serde_json::json!({"case":name,"input":source,"legacy":legacy,"domain":domain,"class":class,"wording_rule":rule,"domain_worse":worse}));
		old.close().unwrap();
		new.close().unwrap();
	}
	serde_json::json!(rows)
}
#[test]
fn block_observations() {
	let rows = report();
	if let Some(path) = std::env::var_os("RNX_ASYNC_BLOCK_REPORT") {
		std::fs::write(path, serde_json::to_vec_pretty(&rows).unwrap()).unwrap();
	}
	assert_eq!(rows.as_array().unwrap().len(), 4);
}
