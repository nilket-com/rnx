//! ADR0257: differential admission controls, separate from measured parity rows.
use super::context;
use crate::session::{
	DEFAULT_CEILING, Session,
	domain_session::{
		DomainSession,
		transcripts::{parity_new, parity_old},
	},
};
use serde_json::Value;
fn pair(source: &str) -> (Value, Value) {
	let mut old = Session::new(context()).unwrap();
	let mut new = DomainSession::new(context(), DEFAULT_CEILING).unwrap();
	let a = parity_old(&mut old, source);
	let b = parity_new(&mut new, source);
	old.close().unwrap();
	new.close().unwrap();
	(a, b)
}
#[test]
fn values_and_transactionality() {
	for source in [
		"const A=[1,2];const B=A[1u64];B",
		"const A=(1,2);const B=A[1];B",
		"const A=#{x:([2],3)};const B=A[\"x\"][0][0];B",
		"const A={let x=2;let y={let x=x+3;x};x+y};A",
		"const A={let _=2;};A",
		"const A=if true {1}else{1/0};A",
		"const A=if false {Some(1)}else{2};A",
		"const A=if true {1}else if let Some(x)=Some(2){x}else{3};A",
		"const A=if true {1}else if let Some(x)=Some(2){x};A",
		"const A=if false {if true {1/0}else{Some(1)}}else{3};A",
		"const A=if true {7};(A,if true {7})",
		"const A=if false {7};(A,if false {7})",
		"const A=({let x=[1,2];x})[1];A",
		"const A=(if true {[3]}else{[4]})[0];A",
		"let a=const {let x=3;x+1};a",
	] {
		let (a, b) = pair(source);
		assert_eq!(a["outcome"], "ok", "{source}: {a}");
		assert_eq!(a, b, "{source}");
	}
	let mut old = Session::new(context()).unwrap();
	let mut new = DomainSession::new(context(), DEFAULT_CEILING).unwrap();
	assert_eq!(
		parity_old(&mut old, "let kept=[7];"),
		parity_new(&mut new, "let kept=[7];")
	);
	for source in [
		"const BAD=[1][2];",
		"const BAD={let x=1;1/0};",
		"const BAD=if true {1/0}else{1};",
	] {
		let a = parity_old(&mut old, source);
		let b = parity_new(&mut new, source);
		assert_ne!(a["outcome"], "ok");
		assert_ne!(b["outcome"], "ok");
		assert_eq!(parity_old(&mut old, "kept"), parity_new(&mut new, "kept"));
	}
	old.close().unwrap();
	new.close().unwrap();
}
#[test]
fn diagnostics_and_cycles() {
	for (source, family) in [
		("const A=[1][2];A", "Index out of bounds"),
		("const A=(1,2)[9];A", "Index out of bounds"),
		("const A=[1][-1];A", "Failed to convert value"),
		("const A=#{x:1}[\"y\"];A", "Missing field"),
		("const A=[1][true];A", "Expected number type"),
		("const A=[1][1.0];A", "Expected number type"),
		("const A=[1]['a'];A", "Expected number type"),
		("const A=[1][\"x\"];A", "Expected number type"),
		("const A=#{x:1}[0];A", "index get operation"),
		("const A=1[0];A", "index get operation"),
		("const A=\"s\"[0];A", "index get operation"),
		("const A=if 1 {2}else{3};A", "Expected type"),
	] {
		let (a, b) = pair(source);
		assert_eq!(a, b, "diagnostic parity: {source}");
		assert_ne!(a["outcome"], "ok");
		assert_ne!(b["outcome"], "ok");
		assert!(a.to_string().contains(family), "{source}: {a}");
		assert!(b.to_string().contains(family), "{source}: {b}");
	}
	for source in [
		"const A=[A][0];A",
		"const A={A};A",
		"const A=if true {A}else{1};A",
	] {
		let (a, b) = pair(source);
		assert_ne!(a["outcome"], "ok", "{source}");
		assert_ne!(b["outcome"], "ok", "{source}");
		assert_eq!(a, b, "cycle parity: {source}");
	}
	for (source, reason) in [
		("const A={let (x,y)=(1,2);x};A", "constant binding patterns"),
		("const A={let x=1;x=2;x};A", "constant assignment"),
		(
			"const A=if let Some(x)=Some(1){x};A",
			"constant if pattern conditions",
		),
		(
			"const A=if false {1}else if let Some(x)=Some(2){x}else{3};A",
			"constant if pattern conditions",
		),
		("const A={loop{break 1}};A", "constant loops"),
	] {
		let (_, b) = pair(source);
		assert_ne!(b["outcome"], "ok");
		assert!(b.to_string().contains(reason), "{source}: {b}");
	}
}
