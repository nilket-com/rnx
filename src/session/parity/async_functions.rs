//! ADR 0273 observations; the pin gates the measured classes, not presumed parity.
use super::classify;
use crate::session::{
	DEFAULT_CEILING, Session,
	domain_session::{DomainSession, transcripts::parity_observe},
};

fn context() -> rune::Context {
	let mut context = super::context();
	crate::time::install(&mut context).unwrap();
	context
}

// Step-2 ruling: a synchronous script target is refused before execution with
// a useful remove-await remedy. Only this fully observed pair is classified.
fn ordinary_await_pair() -> (serde_json::Value, serde_json::Value) {
	(
		serde_json::json!({"outcome":"runtime","message":"Missing protocol function `INTO_FUTURE` for `std::i64`","origin":{"input":1,"line":1,"column":18}}),
		serde_json::json!({"outcome":"refused","message":"refused: ordinary is not asynchronous; remove .await and call it as ordinary(...) (input 1, line 1, column 18)","origin":null}),
	)
}
pub(super) fn reviewed_ordinary_await(a: &serde_json::Value, b: &serde_json::Value) -> bool {
	let (old, new) = ordinary_await_pair();
	*a == old && *b == new
}

fn future_pairs() -> [(serde_json::Value, serde_json::Value); 2] {
	[
		(
			serde_json::json!({"outcome":"ok","value":"<::std::future::Future>"}),
			serde_json::json!({"outcome":"refused","message":"refused: f is asynchronous; await its call as f(...).await (input 1, line 1, column 17)","origin":null}),
		),
		(
			serde_json::json!({"outcome":"ok","value":"0"}),
			serde_json::json!({"outcome":"refused","message":"refused: f is asynchronous; futures as values are not available; await its call as f(...).await (input 1, line 1, column 23)","origin":null}),
		),
	]
}
pub(super) fn reviewed_future_refusal(a: &serde_json::Value, b: &serde_json::Value) -> bool {
	future_pairs()
		.into_iter()
		.any(|(old, new)| *a == old && *b == new)
}
#[test]
fn future_refusal_classes_match_the_host_policy_with_exact_pair_controls() {
	for (old, new) in future_pairs() {
		assert_eq!(
			classify(&old, &new),
			(
				"Intentional difference",
				Some("direct-script-await-required"),
				false
			)
		);
		for key in ["outcome", "value"] {
			let mut changed = old.clone();
			changed[key] = serde_json::Value::Null;
			assert!(!reviewed_future_refusal(&changed, &new));
			assert_ne!(
				classify(&changed, &new).1,
				Some("direct-script-await-required")
			);
		}
		for key in ["outcome", "message", "origin"] {
			let mut changed = new.clone();
			changed[key] = if key == "origin" {
				serde_json::json!({"input":1,"line":1,"column":17})
			} else {
				serde_json::Value::Null
			};
			assert!(!reviewed_future_refusal(&old, &changed));
			assert_ne!(
				classify(&old, &changed).1,
				Some("direct-script-await-required")
			);
		}
	}
}

#[test]
fn ordinary_await_classification_is_an_exact_observed_pair() {
	let (old, new) = ordinary_await_pair();
	assert_eq!(
		classify(&old, &new),
		(
			"Intentional difference",
			Some("script-await-requires-async"),
			false
		)
	);
	for key in ["outcome", "message", "origin"] {
		let mut changed = old.clone();
		changed[key] = serde_json::Value::Null;
		assert!(!reviewed_ordinary_await(&changed, &new));
		let mut changed = new.clone();
		changed[key] = if key == "origin" {
			serde_json::json!({"input":1,"line":1,"column":18})
		} else {
			serde_json::Value::Null
		};
		assert!(!reviewed_ordinary_await(&old, &changed));
	}
}

pub(super) fn report() -> serde_json::Value {
	let mut rows = Vec::new();
	for (name, source) in [
		("direct", "async fn f(x){x+1} f(6).await"),
		(
			"nested",
			"async fn f(x){time::sleep(0).await?;Ok(x+1)} async fn g(x){f(x).await} g(6).await",
		),
		("try", "async fn f(){Err(\"stop\")?;Ok(7)} f().await"),
		("early-return", "async fn f(){return 7;9} f().await"),
		("bare", "async fn f(){7} f()"),
		("function-value", "async fn f(){7} let x=f;0"),
		("ordinary-await", "fn ordinary(){7} ordinary().await"),
		(
			"moved-direct",
			"async fn f(x){x.len()} let x=[7];drop(x);f(x).await",
		),
	] {
		let mut old = Session::new(context()).unwrap();
		let mut new = DomainSession::for_frontend(
			context(),
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
fn direct_await_observations() {
	let rows = report();
	if let Some(path) = std::env::var_os("RNX_ASYNC_FUNCTION_REPORT") {
		std::fs::write(path, serde_json::to_vec_pretty(&rows).unwrap()).unwrap();
	}
	assert_eq!(rows.as_array().unwrap().len(), 8);
}
