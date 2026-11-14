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
	let mut new = DomainSession::for_frontend(
		context(),
		DEFAULT_CEILING,
		crate::session::remedy::FrontEnd::Terminal,
	)
	.unwrap();
	let a = parity_old(&mut old, source);
	let b = parity_new(&mut new, source);
	old.close().unwrap();
	new.close().unwrap();
	(a, b)
}
#[test]
fn values_and_transactionality() {
	for source in [
		"const A={let (x,y)=(1,2);x};A",
		"const A=if let Some(x)=Some(1){x};A",
		"const A=if false{1}else if let Some(x)=Some(2){x}else{3};A",
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
	let mut new = DomainSession::for_frontend(
		context(),
		DEFAULT_CEILING,
		crate::session::remedy::FrontEnd::Terminal,
	)
	.unwrap();
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
}

#[test]
fn option_and_field_values() {
	let (a, b) = pair("const A=Some(1);A");
	assert_eq!(a["outcome"], "ok");
	assert_eq!(a["value"], "Some(1)");
	assert_eq!(a, b);
	for source in [
		"const A=None;A",
		"const A=[3];const B=A.0;let a=[3];(B,a.0)",
		"const A=Some(3);const B=A.0;let a=Some(3);(B,a.0)",
		"const A=(3,);const B=A.0;let a=(3,);(B,a.0)",
		"const A=#{x:3};const B=A.x;let a=#{x:3};(B,a.x)",
		"const A=Some(Some(None));A",
		"const A=[Some(1),None];A",
		"const A=(Some(1),None);A",
		"const A=#{x:Some([1])};A",
		"const A=Some([1]);let a=A;let b=A;a.0.push(2);(a,b,A)",
		"const A=[Some([1])];const B=A[0];let b=B;b.0.push(2);(b,B,A)",
		"const A=(Some([1]),);const B=A.0;const C=B;let b=C;b.0.push(2);(b,B,C)",
		"const A=([Some(7)],);const B=A.0.0.0;B",
		"const A=#{x:[3]};const B=A.x;B",
		"const A=[1];const B=A.0;B",
		"const A=Some(3);const B=A.0;B",
		"const Some=7;const A=Some;A",
		"const None=8;const A=None;A",
	] {
		let (a, b) = pair(source);
		assert_eq!(a["outcome"], "ok", "{source}: {a}");
		assert_eq!(a, b, "{source}");
	}
}

#[test]
fn option_and_field_diagnostics() {
	for source in [
		"const A=None;const B=A.0;B",
		"const A=();const B=A.0;B",
		"const A=(1,2);const B=A.9;B",
		"const A=[1];const B=A.9;B",
		"const A=Some(3);const B=A.1;B",
		"const A=#{x:1};const B=A.y;B",
		"const A=1;const B=A.0;B",
		"const A=1;const B=A.x;B",
		"const A=#{x:1};const B=A.0;B",
		"const A=(1,2);const B=A.x;B",
		"const A=((1,),);const B=A.0.9;B",
		"const A=Some();A",
		"const A=Some(1,2);A",
		"const A=None(1);A",
	] {
		let (a, b) = pair(source);
		assert_ne!(a["outcome"], "ok", "{source}: {a}");
		assert_eq!(a, b, "{source}");
	}
	for source in ["const A=Ok(1);A", "const A=Err(1);A"] {
		let (a, b) = pair(source);
		assert_ne!(a["outcome"], "ok");
		assert_ne!(b["outcome"], "ok");
		assert!(b.to_string().contains("Result constants"), "{b}");
	}
	let (_, b) = pair("fn Some(x){x} const A=Some(1);A");
	assert_ne!(b["outcome"], "ok");
	let mut old = Session::new(context()).unwrap();
	let mut new = DomainSession::for_frontend(
		context(),
		DEFAULT_CEILING,
		crate::session::remedy::FrontEnd::Terminal,
	)
	.unwrap();
	assert_eq!(
		parity_old(&mut old, "let kept=[7];"),
		parity_new(&mut new, "let kept=[7];")
	);
	for source in ["const BAD=Some([1]).0.9;", "const BAD=#{x:Some(1)}.y;"] {
		assert_ne!(parity_old(&mut old, source)["outcome"], "ok");
		assert_ne!(parity_new(&mut new, source)["outcome"], "ok");
		assert_eq!(parity_old(&mut old, "kept"), parity_new(&mut new, "kept"));
	}
	old.close().unwrap();
	new.close().unwrap();
}
