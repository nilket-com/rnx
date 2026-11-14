//! Loop semantics and failed-input isolation, independent of measured row classes.
use super::context;
use crate::session::{
	DEFAULT_CEILING, Session,
	domain_session::{
		DomainSession,
		transcripts::{parity_new, parity_old},
	},
};

#[test]
fn loop_scope_and_failed_input() {
	let mut old = Session::new(context()).unwrap();
	let mut new = DomainSession::for_frontend(
		context(),
		DEFAULT_CEILING,
		crate::session::remedy::FrontEnd::Terminal,
	)
	.unwrap();
	for source in [
		"let kept=[7];",
		"const A={let x=9;while let Some(x)=Some(2){break;}x};A",
		"const B={let x=11;for x in [2,3]{if x==2{continue;}break;}x};B",
		"const C='out:{let x=7;loop{break 'out x}};C",
		"const D={let x=1;x+={x=10;2};x};D",
		"const E={let x=[1];let y=x;x=[2];y[0]};E",
		"const fn f(){loop{for x in [1,2,3]{if x==2{return x;}}}}",
		"const F=f();F",
		"const G={let x=1;x=2;x};G",
	] {
		let a = parity_old(&mut old, source);
		let b = parity_new(&mut new, source);
		assert_eq!(a["outcome"], "ok", "{source}: {a}");
		assert_eq!(a, b, "{source}");
	}
	for source in [
		"const BAD={let n=0;while n<3{n+=1;1/0;}};",
		"const BAD=for (a,b) in [(1,)]{};",
		"const BAD=for x in [1]{break 7};",
	] {
		assert_ne!(parity_old(&mut old, source)["outcome"], "ok");
		assert_ne!(parity_new(&mut new, source)["outcome"], "ok");
		assert_eq!(parity_old(&mut old, "kept"), parity_new(&mut new, "kept"));
		assert_ne!(parity_old(&mut old, "BAD")["outcome"], "ok");
		assert_ne!(parity_new(&mut new, "BAD")["outcome"], "ok");
	}
	for source in [
		r#"const BAD={let s="";for(k,v) in #{z:1,a:2,m:3,b:4}{s=k;break;}s};BAD"#,
		r#"const BAD={let s="";for(k,v) in #{z:1,a:2,m:3,b:4}{s+=k;}s};BAD"#,
	] {
		let b = parity_new(&mut new, source);
		assert_eq!(b["outcome"], "refused", "{b}");
		assert!(b.to_string().contains("constant object iteration"), "{b}");
		assert_eq!(parity_old(&mut old, "kept"), parity_new(&mut new, "kept"));
	}

	old.close().unwrap();
	new.close().unwrap();
}
