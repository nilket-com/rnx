//! ADR0247: isolated executed witnesses; no ambient environment enters receipts.
use serde_json::Value;
const NAMES: &[&str] = &[
	"process::exit",
	"fs::metadata",
	"fs::read_dir",
	"time::parts",
	"text::split_max",
	"env::args",
	"env::var",
	"env::home_dir",
	"env::vars",
	"fs::write",
	"fs::append",
	"fs::write_new",
	"web::parse_form",
	"json::stringify",
	"web::html",
	"web::text",
	"web::escape_html",
];
pub(crate) fn is_new(name: &str) -> bool {
	NAMES.contains(&name) || super::parity::external::NAMES.contains(&name)
}

pub(crate) fn isolated_witness(name: &str) -> crate::routing_inventory::Result<Value> {
	if super::parity::external::NAMES.contains(&name) {
		return Ok(super::parity::external::report(name));
	}
	static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
	let dir = Scratch(std::env::temp_dir().join(format!(
			"rnx-0247-{}-{}-{}",
			std::process::id(),
			std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)?
				.as_nanos(),
			NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
		)));
	std::fs::create_dir(dir.path())?;
	std::fs::write(dir.path().join("a.txt"), "abc")?;
	std::fs::create_dir(dir.path().join("empty"))?;
	let receipt = dir.path().join("receipt.json");
	let output = std::process::Command::new(std::env::current_exe()?)
		.args([
			"--exact",
			"session::host_enrollment::child_witness",
			"--nocapture",
		])
		.env_clear()
		.env("RNX_0247_HOST", name)
		.env("RNX_0247_RECEIPT", &receipt)
		.env("RNX_0247_VALUE", "fixture")
		.env("RNX_0247_EMPTY", "")
		.env("HOME", "/0247-fixture-home")
		.env("USERPROFILE", "/0247-fixture-home")
		.current_dir(dir.path())
		.output()?;
	if !output.status.success() {
		return Err(format!(
			"isolated host witness {name}: {}",
			String::from_utf8_lossy(&output.stderr)
		)
		.into());
	}
	let result: Value = serde_json::from_slice(&std::fs::read(receipt)?)?;
	assert_eq!(result["class"], "Equal", "different host result: {result}");
	Ok(result)
}

#[test]
fn child_witness() {
	let Ok(name) = std::env::var("RNX_0247_HOST") else {
		return;
	};
	let input = match name.as_str() {
		"web::escape_html" => r#"web::escape_html("<&é\u{0}>")"#,
		"process::exit" => "process::exit(0)",
		"fs::metadata" => {
			"let m=fs::metadata(\"a.txt\").unwrap(); (m.kind,m.size,m.readonly,m.symlink,m.modified_ms>0,fs::metadata(\"missing\").is_err())"
		}
		"fs::read_dir" => {
			"(fs::read_dir(\".\"),fs::read_dir(\"empty\"),fs::read_dir(\"missing\").is_err())"
		}
		"time::parts" => "(time::parts(0,\"UTC\"),time::parts(0,\"not-a-zone\").is_err())",
		"text::split_max" => {
			"(text::split_max(\"a b c\",2),text::split_max(\"\",0),text::split_max(\"a\",-1))"
		}
		"env::args" => "env::args()",
		"env::var" => {
			"(env::var(\"RNX_0247_VALUE\"),env::var(\"RNX_0247_EMPTY\"),env::var(\"RNX_0247_MISSING\"),env::var(\"=bad\"))"
		}
		"env::home_dir" => "env::home_dir()",
		"env::vars" => "let e=env::vars().unwrap();(e[\"RNX_0247_VALUE\"],e[\"RNX_0247_EMPTY\"])",

		"fs::write" => {
			r#"let r=fs::write("out","abc");let t=fs::read("out");fs::remove_file("out").unwrap();(r,t,fs::write("never",42),fs::exists("never"))"#
		}
		"fs::append" => {
			r#"fs::write("out","a").unwrap();let r=fs::append("out","bc");let t=fs::read("out");fs::remove_file("out").unwrap();(r,t,fs::append("never",[]),fs::exists("never"))"#
		}
		"fs::write_new" => {
			r#"let r=fs::write_new("out","abc");let second=fs::write_new("out","bad").is_err();let t=fs::read("out");fs::remove_file("out").unwrap();(r,second,t,fs::write_new("never",#{}),fs::exists("never"))"#
		}
		"web::parse_form" => {
			r#"(web::parse_form("a=one+two&a=ignored"),web::parse_form(42),web::parse_form("%FF=x"))"#
		}
		"json::stringify" => r#"(json::stringify(#{a:[1,2],b:Some("x")}),json::stringify(Ok(1)))"#,
		"web::html" => r#"(web::html(200,"<b>hi</b>"),web::html(199,42))"#,
		"web::text" => r#"(web::text(200,"hello"),web::text(199,42))"#,
		_ => panic!("unknown witness"),
	};
	let row = super::parity::observe_host(
		&name,
		input,
		crate::routing_inventory::session_parity_context().unwrap(),
	)
	.unwrap();
	assert_eq!(row["class"], "Equal", "{row}");
	std::fs::write(
		std::env::var_os("RNX_0247_RECEIPT").unwrap(),
		serde_json::to_vec_pretty(&row).unwrap(),
	)
	.unwrap();
}

#[test]
fn enrolled_executed_witnesses() {
	for name in NAMES {
		isolated_witness(name).unwrap();
	}
}

#[test]
fn resolved_refusals_preserve_aliases_and_first_cause() {
	use super::domain_session::{DomainSession, transcripts::parity_new};
	for source in [
		"process::run(\"\",[],#{})",
		"use process::run as save; save(\"\",[],#{})",
		"use process::run; run(\"\",[],#{})",
	] {
		let mut session = DomainSession::for_frontend(
			crate::routing_inventory::session_parity_context().unwrap(),
			super::DEFAULT_CEILING,
			crate::session::remedy::FrontEnd::Terminal,
		)
		.unwrap();
		let result = parity_new(&mut session, source);
		let text = result.to_string();
		if source.starts_with("use ") {
			assert!(text.contains("imports"), "first cause lost: {result}");
		} else {
			assert!(
				text.contains("process::run needs argument-vector and options-object input"),
				"{result}"
			);
			assert!(text.contains("without --engine=execution"), "{result}");
		}
		session.close().unwrap();
	}
}

struct Scratch(std::path::PathBuf);
impl Scratch {
	fn path(&self) -> &std::path::Path {
		&self.0
	}
}
impl Drop for Scratch {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.0);
	}
}

#[test]
fn compiler_reports_resolved_host_for_imported_and_aliased_calls() {
	for source in [
		"pub fn main(){process::run(\"\",[],#{})}",
		"use process::run as save; pub fn main(){save(\"\",[],#{})}",
		"use process::run;pub fn main(){run(\"\",[],#{})}",
	] {
		let context = crate::routing_inventory::session_parity_context().unwrap();
		let mut sources = rune::Sources::new();
		sources
			.insert(rune::Source::memory(source).unwrap())
			.unwrap();
		let mut diagnostics = rune::Diagnostics::new();
		let error =
			rune::experimental_core::frontend::compile(&mut sources, &context, &mut diagnostics, 1)
				.err()
				.unwrap();
		assert_eq!(
			error.refusal,
			Some(rune::experimental_core::error::Refusal::Native)
		);
		let host = diagnostics
			.diagnostics()
			.iter()
			.find_map(|d| match d {
				rune::diagnostics::Diagnostic::Fatal(f) => {
					f.as_compile_error().and_then(|e| e.unenrolled_host())
				}
				_ => None,
			})
			.expect("resolved host diagnostic");
		assert_eq!(host.to_string().trim_start_matches("::"), "process::run");
	}
}

#[test]
fn string_input_sessions_preserve_values_and_reject_before_effects() {
	use super::domain_session::{DomainSession, transcripts::parity_observe};
	let dir = Scratch(std::env::temp_dir().join(format!("rnx-0249-{}", std::process::id())));
	std::fs::create_dir(&dir.0).unwrap();
	let path = dir.0.join("keep");
	std::fs::write(&path, "sentinel").unwrap();
	let quoted = serde_json::to_string(path.to_str().unwrap()).unwrap();
	let mut old =
		super::Session::new(crate::routing_inventory::session_parity_context().unwrap()).unwrap();
	let mut new = DomainSession::for_frontend(
		crate::routing_inventory::session_parity_context().unwrap(),
		super::DEFAULT_CEILING,
		crate::session::remedy::FrontEnd::Terminal,
	)
	.unwrap();
	for input in [
		"let s=\"a=hello+world\";let parse=||web::parse_form(s);0".to_owned(),
		"parse()".to_owned(),
		"web::parse_form(if true {s} else {\"\"})".to_owned(),
		"s".to_owned(),
		format!("let write_later=|v|fs::write({quoted},v);0"),
	] {
		let (a, b) = parity_observe(&mut old, &mut new, &input);
		assert_eq!(a, b, "{input}");
	}
	for value in [
		"()", "false", "42", "1.5", "'x'", "[]", "#{}", "(1,2)", "None", "Some(1)", "Ok(1)",
		"Err(1)", "||42",
	] {
		for func in ["fs::write", "fs::append", "fs::write_new"] {
			let input = format!("{func}({quoted},{value})");
			let (a, b) = parity_observe(&mut old, &mut new, &input);
			assert_eq!(a, b, "{input}");
			assert_eq!(b["outcome"], "ok");
			assert_eq!(std::fs::read_to_string(&path).unwrap(), "sentinel");
		}
		let input = format!("web::parse_form({value})");
		let (a, b) = parity_observe(&mut old, &mut new, &input);
		assert_eq!(a, b, "{input}");
		assert_eq!(b["outcome"], "ok");
	}
	let (a, b) = parity_observe(&mut old, &mut new, "write_later(42)");
	assert_eq!(a, b);
	assert_eq!(std::fs::read_to_string(&path).unwrap(), "sentinel");
	// Earlier completed effects survive a later runtime error; declarations do not.
	let input = format!("fs::write({quoted},\"once\").unwrap();let uncommitted=1;1/0");
	assert!(new.eval(&input).is_err());
	assert_eq!(std::fs::read_to_string(&path).unwrap(), "once");
	assert!(!new.binding_names().contains(&"uncommitted".to_owned()));
	old.close().unwrap();
	new.close().unwrap();
}

#[test]
fn bytes_are_not_representable_in_execution_sessions() {
	use super::domain_session::{DomainSession, transcripts::parity_new};
	for input in [
		"b\"bytes\"",
		"\"bytes\".as_bytes()",
		"fs::read_bytes(\"unused\")",
	] {
		let mut new = DomainSession::for_frontend(
			crate::routing_inventory::session_parity_context().unwrap(),
			super::DEFAULT_CEILING,
			crate::session::remedy::FrontEnd::Terminal,
		)
		.unwrap();
		let row = parity_new(&mut new, input);
		assert_ne!(row["outcome"], "ok", "{input}: {row}");
		new.close().unwrap();
	}
}

#[test]
fn owned_json_differential() {
	let mut old =
		super::Session::new(crate::routing_inventory::session_parity_context().unwrap()).unwrap();
	let mut new = super::domain_session::DomainSession::for_frontend(
		crate::routing_inventory::session_parity_context().unwrap(),
		super::DEFAULT_CEILING,
		crate::session::remedy::FrontEnd::Terminal,
	)
	.unwrap();
	use super::domain_session::transcripts::{parity_new, parity_old};
	let cases = [
		r#"json::stringify([1,2,3])"#,
		r#"json::stringify(i64)"#,
		r#"json::stringify(1..4)"#,
		r#"json::stringify(#{z:1,a:2,m:3})"#,
		r#"json::stringify(#{a:[1,(2,3)], b:Some("hello"), c:None})"#,
		r#"let a=[1,2];json::stringify([a,a])"#,
		r#"let a=[];a.push(a);json::stringify(a)"#,
		r#"let a=#{x:()};a.x=a;json::stringify(a)"#,
		r#"let a=((),);a.0=a;json::stringify(a)"#,
		r#"json::stringify(#{a:Ok(1)})"#,
		r#"json::stringify(#{"a.b":[||1]})"#,
		r#"struct A{x} json::stringify(#{a:A{x:1}})"#,
		r#"struct B(i64); json::stringify([B(1)])"#,
		r#"struct C; json::stringify(C)"#,
		r#"json::stringify([-9223372036854775807-1,9223372036854775807,18446744073709551615u64])"#,
		r#"json::stringify([-0.0,0.0/0.0,1.0/0.0,-1.0/0.0])"#,
		r#"json::stringify(['𝄞',"\n\t\"\\\u{0}"])"#,
		r#"let s="local";json::stringify(if true {s}else{"no"})"#,
		r#"let prior=||#{s:"captured"};prior()"#,
		r#"json::stringify(prior())"#,
		r#"let v=[];for n in 0..100000 {v.push(n);}json::stringify(v).unwrap().len()"#,
		r#"let v=0;for n in 0..64 {v=Some(v);}json::stringify(v)"#,
		r#"let v=0;for n in 0..64 {v=[v];}json::stringify(v)"#,
	];
	for case in cases {
		let a = parity_old(&mut old, case);
		let b = parity_new(&mut new, case);
		assert_eq!(a["outcome"], "ok", "legacy did not execute {case}: {a}");
		assert_eq!(a, b, "{case}");
	}
	new.close().unwrap();
}

#[test]
fn string_responses_differential() {
	use super::domain_session::transcripts::{parity_new, parity_old};
	let mut old =
		super::Session::new(crate::routing_inventory::session_parity_context().unwrap()).unwrap();
	let mut new = super::domain_session::DomainSession::for_frontend(
		crate::routing_inventory::session_parity_context().unwrap(),
		super::DEFAULT_CEILING,
		crate::session::remedy::FrontEnd::Terminal,
	)
	.unwrap();
	for name in ["html", "text"] {
		for args in [
			r#"200,"<&é\u{0}>""#,
			r#"199,"x""#,
			r#"600,"x""#,
			r#"204,"""#,
			r#"304,"""#,
			r#"204,"x""#,
			r#"304,"x""#,
			"199,42",
			"600,[]",
			"200,#{}",
			"200,None",
		] {
			let case = format!("web::{name}({args})");
			let a = parity_old(&mut old, &case);
			let b = parity_new(&mut new, &case);
			assert_eq!(a["outcome"], "ok", "{case}: {a}");
			assert_eq!(a, b, "{case}");
		}
		for body in [
			r#"let s="local";let r=web::NAME(200,s).unwrap();s=s+"!";(s,r.body,r.status,r.headers)"#,
			r#"let s="branch";web::NAME(200,if true{s}else{"no"})"#,
			r#"let prior=||web::NAME(200,"prior");prior()"#,
			"prior()",
			r#"let s="x";for _ in 0..20{s=s+s;}let r=web::NAME(200,s).unwrap();(r.body.len(),s.len())"#,
			r#"let s="x";for _ in 0..20{s=s+s;}s=s+"x";web::NAME(200,s)"#,
		] {
			let case = body.replace("NAME", name);
			let a = parity_old(&mut old, &case);
			let b = parity_new(&mut new, &case);
			assert_eq!(a["outcome"], "ok", "{case}: {a}");
			assert_eq!(a, b, "{case}");
		}
	}
	old.close().unwrap();
	new.close().unwrap();
}

#[test]
fn string_responses_moved_and_bytes_contracts() {
	use super::domain_session::transcripts::{parity_new, parity_old};
	for name in ["html", "text"] {
		let mut old =
			super::Session::new(crate::routing_inventory::session_parity_context().unwrap())
				.unwrap();
		let mut new = super::domain_session::DomainSession::for_frontend(
			crate::routing_inventory::session_parity_context().unwrap(),
			super::DEFAULT_CEILING,
			crate::session::remedy::FrontEnd::Terminal,
		)
		.unwrap();
		let moved = format!(r#"let s=String::from("x");drop(s);web::{name}(200,s)"#);
		let a = parity_old(&mut old, &moved);
		let b = parity_new(&mut new, &moved);
		assert_eq!(a["value"], "Err(\"web: expected String or Bytes\")", "{a}");
		assert_ne!(b["outcome"], "ok", "{b}");
		assert!(b["message"].as_str().unwrap().contains("moved"), "{b}");
		let bytes = format!(r#"web::{name}(200,b"x").unwrap().body == b"x""#);
		let a = parity_old(&mut old, &bytes);
		let b = parity_new(&mut new, &bytes);
		assert_eq!(a["outcome"], "ok", "{a}");
		assert_eq!(a["value"], "true", "{a}");
		assert_ne!(b["outcome"], "ok", "{b}");
		old.close().unwrap();
		new.close().unwrap();
	}
}

#[test]
fn escape_html_differential_and_recovery() {
	use super::domain_session::transcripts::{parity_new, parity_old};
	let mut old =
		super::Session::new(crate::routing_inventory::session_parity_context().unwrap()).unwrap();
	let mut new = super::domain_session::DomainSession::for_frontend(
		crate::routing_inventory::session_parity_context().unwrap(),
		super::DEFAULT_CEILING,
		crate::session::remedy::FrontEnd::Terminal,
	)
	.unwrap();
	for source in [
		r#"web::escape_html("")"#,
		r#"web::escape_html("&<>\"'é\u{0}")"#,
		r#"web::escape_html("&amp;")"#,
		r#"let s="<local>";web::escape_html(s)"#,
		r#"web::escape_html(if true {s} else {"no"})"#,
		r#"let earlier=||web::escape_html(s);earlier()"#,
		"earlier()",
		r#"`markup {web::escape_html(s)}`"#,
		r#"let big="a";for _ in 0..20{big=big+big;}web::escape_html(big).len()"#,
		"let keep=7;keep",
	] {
		let a = parity_old(&mut old, source);
		let b = parity_new(&mut new, source);
		assert_eq!(a["outcome"], "ok", "{source}: {a}");
		assert_eq!(a, b, "{source}");
	}
	for source in [
		r#"let failed=1;web::escape_html(big+"a");keep=99"#,
		r#"let failed=1;let expanded="&";for _ in 0..18{expanded=expanded+expanded;}web::escape_html(expanded);keep=99"#,
	] {
		let a = parity_old(&mut old, source);
		let b = parity_new(&mut new, source);
		println!("CAP legacy={a} execution={b}");
		assert_ne!(a["outcome"], "ok");
		assert_ne!(b["outcome"], "ok");
		assert!(a["message"].as_str().unwrap().contains("escape_html"));
		assert_eq!(a["origin"], b["origin"]);
		let origin = &a["origin"];
		assert_eq!(
			b["message"],
			format!(
				"{}\n  at input {}, line {}, column {}",
				a["message"].as_str().unwrap(),
				origin["input"],
				origin["line"],
				origin["column"]
			)
		);
		let a = parity_old(&mut old, "keep");
		let b = parity_new(&mut new, "keep");
		assert_eq!(a, b);
		assert_eq!(b["value"], "7");
		assert_ne!(parity_old(&mut old, "failed")["outcome"], "ok");
		assert_ne!(parity_new(&mut new, "failed")["outcome"], "ok");
	}
	old.close().unwrap();
	new.close().unwrap();
}
