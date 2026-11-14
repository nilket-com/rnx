//! Direct script awaits, declaration transactions and ordinary-call parity.
use super::*;

fn context() -> Context {
	let mut c = Context::with_default_modules().unwrap();
	crate::install_core(&mut c).unwrap();
	crate::time::install(&mut c).unwrap();
	c
}
fn session() -> DomainSession {
	DomainSession::new(context(), DEFAULT_CEILING).unwrap()
}
fn text(s: &mut DomainSession, source: &str) -> std::result::Result<Option<String>, Failure> {
	s.eval_rendered(source, &crate::format::Limits::default())
}
#[test]
fn direct_await_arguments_body_try_and_early_return_preserve_order() {
	let mut s = session();
	for (source, value) in [
		("let v=[];0", Some("0")),
		(
			"async fn f(v,a,b){v.push(3);time::sleep(0).await?;v.push(4);Ok((a,b))}",
			None,
		),
		(
			"let r=f(v,{v.push(1);11},{v.push(2);22}).await;(v,r)",
			Some("([1, 2, 3, 4], Ok((11, 22)))"),
		),
		(
			"async fn inner(x){time::sleep(0).await?;if x<0{return Ok(9)}Ok(x+1)}",
			None,
		),
		("async fn outer(x){inner(x).await}", None),
		("outer(6).await", Some("Ok(7)")),
		("outer(-1).await", Some("Ok(9)")),
		(
			"async fn fail(v){v.push(5);Err(\"stop\")?;v.push(99);Ok(7)}",
			None,
		),
		("fail(v).await", Some("Err(\"stop\")")),
		("v", Some("[1, 2, 3, 4, 5]")),
	] {
		assert_eq!(text(&mut s, source).unwrap().as_deref(), value, "{source}");
	}
	s.close().unwrap();
}
#[test]
fn script_errors_and_moved_arguments_use_the_ordinary_call_boundary() {
	for asynchronous in [false, true] {
		let mut s = session();
		let prefix = if asynchronous { "async " } else { "" };
		let suffix = if asynchronous { ".await" } else { "" };
		text(&mut s, "let v=[];let x=[7];drop(x);0").unwrap();
		text(
			&mut s,
			&format!("{prefix}fn fail(v,x){{v.push(1);x.len()}}"),
		)
		.unwrap();
		let error = text(&mut s, &format!("fail(v,x){suffix}")).unwrap_err();
		let Failure::Runtime {
			message,
			origin: Some(origin),
		} = error
		else {
			panic!("{error:?}")
		};
		assert!(
			message.starts_with("Cannot read the value: it has been moved"),
			"{message}"
		);
		assert_eq!(origin.input, 2);
		assert_eq!(
			origin.column,
			prefix.len() + "fn fail(v,x){v.push(1);x".len()
		);
		assert_eq!(text(&mut s, "v").unwrap().as_deref(), Some("[1]"));
		assert_eq!(text(&mut s, "7").unwrap().as_deref(), Some("7"));
		s.close().unwrap();
	}
	let mut s = session();
	text(&mut s, "let v=[];0").unwrap();
	text(
		&mut s,
		"async fn fail(v){v.push(1);time::sleep(0).await?;v.push(2);1/0}",
	)
	.unwrap();
	assert!(matches!(
		text(&mut s, "fail(v).await"),
		Err(Failure::Runtime {
			origin: Some(Origin { input: 2, .. }),
			..
		})
	));
	assert_eq!(text(&mut s, "v").unwrap().as_deref(), Some("[1, 2]"));
	s.close().unwrap();
}
#[test]
fn all_future_value_forms_are_named_and_refused_before_effects() {
	for (source, as_value) in [
		("f()", false),
		("let future=f();0", false),
		("id(f())", false),
		("fn bad(){f()}0", false),
		("let value=f;0", true),
		("id(f)", true),
		("Some(f)", true),
		("fn bad(){f}0", true),
	] {
		let mut s = session();
		text(&mut s, "let v=[];0").unwrap();
		text(&mut s, "async fn f(){7} fn id(x){x}").unwrap();
		let error = text(&mut s, &format!("v.push(9);{source}")).unwrap_err();
		assert!(matches!(error, Failure::Refused(_)), "{source}: {error}");
		let value_reason = if as_value {
			"; futures as values are not available"
		} else {
			""
		};
		let caller = if source.starts_with("fn bad") {
			"bad: "
		} else {
			""
		};
		let offset = if as_value {
			source.rfind('f').unwrap()
		} else {
			source.find("f()").unwrap()
		};
		assert_eq!(
			error.to_string(),
			format!(
				"refused: {caller}f is asynchronous{value_reason}; await its call as f(...).await (input 3, line 1, column {})",
				"v.push(9);".len() + offset + 1
			),
			"{source}"
		);
		assert_eq!(text(&mut s, "v").unwrap().as_deref(), Some("[]"));
		s.close().unwrap();
	}
}
#[test]
fn ordinary_await_remedy_is_local_and_old_declarations_survive_both_redefinition_directions() {
	for (definition, caller, replacement, call, expected, remove_await) in [
		(
			"async fn f(){1}",
			"async fn g(){f().await}",
			"fn f(){3}",
			"g().await",
			"1",
			true,
		),
		(
			"fn f(){1}",
			"fn g(){f()}",
			"async fn f(){3}",
			"g()",
			"1",
			false,
		),
		(
			"fn f(){1}",
			"async fn g(){f()}",
			"async fn f(){3}",
			"g().await",
			"1",
			false,
		),
	] {
		let mut s = session();
		text(&mut s, "let v=[];let n=7;0").unwrap();
		text(&mut s, definition).unwrap();
		text(&mut s, caller).unwrap();
		let input = format!("v.push(9);n=8;{replacement}");
		let error = text(&mut s, &input).unwrap_err();
		assert!(matches!(error, Failure::Refused(_)), "{error}");
		assert_eq!(
			error.to_string(),
			format!(
				"refused: {} (input 4, line 1, column 15)",
				remedy::script_await("g", "f", false, remove_await)
			)
		);
		assert_eq!(s.declarations["f"].source, definition);
		assert_eq!(text(&mut s, "(v,n)").unwrap().as_deref(), Some("([], 7)"));
		assert_eq!(text(&mut s, call).unwrap().as_deref(), Some(expected));
		s.close().unwrap();
	}
	let mut s = session();
	text(&mut s, "fn ordinary(){7}").unwrap();
	assert_eq!(
		text(&mut s, "ordinary().await").unwrap_err().to_string(),
		"refused: ordinary is not asynchronous; remove .await and call it as ordinary(...) (input 2, line 1, column 1)"
	);
	assert_eq!(text(&mut s, "ordinary()").unwrap().as_deref(), Some("7"));
	s.close().unwrap();
}
#[test]
fn async_redefinition_and_recursion_use_existing_frames_and_terminal_work_limit() {
	let mut s = session();
	text(&mut s, "async fn f(){1} async fn g(){f().await}").unwrap();
	assert_eq!(text(&mut s, "g().await").unwrap().as_deref(), Some("1"));
	text(&mut s, "async fn f(){2}").unwrap();
	assert_eq!(text(&mut s, "g().await").unwrap().as_deref(), Some("2"));
	text(&mut s, "let v=[7];let n=7;0").unwrap();
	text(
		&mut s,
		"async fn down(n){if n==0{0}else{1+down(n-1).await}}",
	)
	.unwrap();
	assert_eq!(
		text(&mut s, "down(128).await").unwrap().as_deref(),
		Some("128")
	);
	s.budget = 256;
	assert!(matches!(
		text(&mut s, "n=8;down(10000).await"),
		Err(Failure::Budget(256))
	));
	s.budget = super::super::BUDGET;
	assert_eq!(text(&mut s, "(v,n)").unwrap().as_deref(), Some("([7], 7)"));
	s.close().unwrap();
}
#[test]
fn ordinary_functions_keep_the_legacy_await_diagnostic() {
	let mut old = Session::new(context()).unwrap();
	let mut s = session();
	for source in ["async fn f(){7}", "fn bad(){f().await}"] {
		let a = old.eval(source);
		let b = text(&mut s, source);
		if let Err(Failure::Compile {
			message: a,
			origin: oa,
		}) = a
		{
			let Err(Failure::Compile {
				message: b,
				origin: ob,
			}) = b
			else {
				panic!("{b:?}")
			};
			assert_eq!((a, oa), (b, ob));
		} else {
			assert!(b.is_ok(), "{b:?}");
		}
	}
	s.close().unwrap();
	old.close().unwrap();
}
