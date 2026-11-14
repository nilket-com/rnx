//! Differential controls; the measured report class remains unconstrained.
use super::context;
use crate::session::{
	DEFAULT_CEILING, Session,
	domain_session::{
		DomainSession,
		transcripts::{parity_new, parity_old},
	},
};
#[test]
fn patterns_preserve_sessions_and_scope() {
	let mut old = Session::new(context()).unwrap();
	let mut new = DomainSession::for_frontend(
		context(),
		DEFAULT_CEILING,
		crate::session::remedy::FrontEnd::Terminal,
	)
	.unwrap();
	for source in [
		"let kept=[7];",
		"const A={let (a,b)=(2,3);a+b};A",
		"const B={let #{a,..}=#{a:2,b:3};a};B",
		"const C={let [a,..]=[2,3];a};C",
		"const D=match Some(7){Some(x) if x<4=>1,Some(x)=>x,None=>0};D",
		"const E=match 7{1=>2};E",
		"const F=if let Some(x)=Some(7){x};F",
		"const fn pick((a,b)){a+b}",
		"const P=pick((2,3));P",
		"const fn option(){Some(7)}",
		"const fn unwrap(){match option(){Some(x)=>x,None=>0}}",
		"const U=unwrap();U",
		"const fn scope(){let x=11;match Some(2){Some(x) if false=>99,_=>x}}",
		"const S=scope();S",
	] {
		let a = parity_old(&mut old, source);
		let b = parity_new(&mut new, source);
		assert_eq!(a["outcome"], "ok", "{source}: {a}");
		assert_eq!(a, b, "{source}");
	}
	for source in [
		"const BAD={let (a,b)=(1,);a};",
		"const BAD=pick((1,));",
		"const BAD=match 1{_ if 3=>0};",
	] {
		let a = parity_old(&mut old, source);
		let b = parity_new(&mut new, source);
		assert_ne!(a["outcome"], "ok", "{a}");
		assert_ne!(b["outcome"], "ok", "{b}");
		assert_eq!(parity_old(&mut old, "kept"), parity_new(&mut new, "kept"));
	}
	// Unsupported reached pattern stays an execution refusal across source IDs.
	assert_eq!(
		parity_new(&mut new, "const fn unsupported(x){match x{Ok(v)=>v,_=>0}};")["outcome"],
		"ok"
	);
	let refused = parity_new(&mut new, "const BAD=unsupported(7);BAD");
	assert!(
		refused.to_string().contains("constant Result patterns"),
		"{refused}"
	);
	assert_eq!(refused["outcome"], "refused", "{refused}");
	assert_eq!(parity_old(&mut old, "kept"), parity_new(&mut new, "kept"));
	old.close().unwrap();
	new.close().unwrap();
}
