//! Direct blocks keep their own return/capture boundary without future values.
use super::*;

fn session() -> DomainSession {
	let mut context = Context::with_default_modules().unwrap();
	crate::install_core(&mut context).unwrap();
	crate::time::install(&mut context).unwrap();
	DomainSession::new(context, DEFAULT_CEILING).unwrap()
}
fn text(s: &mut DomainSession, source: &str) -> std::result::Result<Option<String>, Failure> {
	s.eval_rendered(source, &crate::format::Limits::default())
}
fn check_error(error: &Failure) -> String {
	let text = error.to_string();
	for generated in ["$", "::{async", "::{closure", "__rnx", "__session"] {
		assert!(!text.contains(generated), "generated identifier: {text}");
	}
	text
}
#[test]
fn returns_try_loops_and_nested_waits_stay_inside_the_block() {
	let mut s = session();
	for (source, value) in [
		("async {time::sleep(0).await;7}.await", "7"),
		("(async {7}).await", "7"),
		("async {let x=async{return 7;99}.await;x+1}.await", "8"),
		("async {let x=async{return}.await;(x,9)}.await", "((), 9)"),
		(
			"async {let x=async{Err(\"stop\")?;Ok(7)}.await;(x,9)}.await",
			"(Err(\"stop\"), 9)",
		),
		(
			"async {let x=async{None?;Some(7)}.await;(x,9)}.await",
			"(None, 9)",
		),
		(
			"async {async{Err(\"stop\")?;Ok(7)}.await?;Ok(9)}.await",
			"Err(\"stop\")",
		),
		(
			"async {let n=0;for x in 0..5 {if x==1{continue} if x==3{break} n+=1;}n}.await",
			"2",
		),
		("async {loop{break 7}}.await", "7"),
		("async {'inner:loop{break 'inner 2}}.await", "2"),
	] {
		assert_eq!(
			text(&mut s, source).unwrap().as_deref(),
			Some(value),
			"{source}"
		);
	}
	text(&mut s, "async fn f(){let x=async{return 7}.await;x+1}").unwrap();
	assert_eq!(text(&mut s, "f().await").unwrap().as_deref(), Some("8"));
	s.close().unwrap();
}
#[test]
fn captures_copy_bindings_share_payloads_and_move_only_free_variables() {
	let mut s = session();
	for (source, value) in [
		("let n=1;let r=async{n+=1;n}.await;(n,r)", "(1, 2)"),
		(
			"let v=[1];let w=v;let r=async{v.push(2);v.len()}.await;(v,w,r)",
			"([1, 2], [1, 2], 2)",
		),
		("let r=async{v=[9];v}.await;(v,r)", "([1, 2], [9])"),
		("let r=async move{7}.await;(n,r)", "(1, 7)"),
		("{let n=n;async move{n+1}.await}", "2"),
		("n", "1"),
		("async{n+1}.await", "2"),
	] {
		assert_eq!(
			text(&mut s, source).unwrap().as_deref(),
			Some(value),
			"{source}"
		);
	}
	for source in [
		"async fn scalar(){let n=1;async move{n+1}.await}",
		"async fn heap(){let v=[7];async move{v[0]+1}.await}",
		"async fn nested(){let v=[7];async move{async move{v[0]+1}.await}.await}",
	] {
		text(&mut s, source).unwrap();
	}
	for (source, value) in [
		("scalar().await", "2"),
		("heap().await", "8"),
		("nested().await", "8"),
	] {
		assert_eq!(text(&mut s, source).unwrap().as_deref(), Some(value));
	}
	s.close().unwrap();
}
#[test]
fn retained_move_is_located_named_before_effects_and_both_remedies_work() {
	let mut s = session();
	text(&mut s, "let n=1;let v=[];0").unwrap();
	let source = "v.push(9);async move{n+1}.await";
	let error = text(&mut s, source).unwrap_err();
	assert!(matches!(error, Failure::Refused(_)), "{error:?}");
	assert_eq!(
		check_error(&error),
		format!(
			"refused: {} (input 2, line 1, column 21)",
			remedy::retained_async_move("n")
		)
	);
	assert_eq!(text(&mut s, "(v,n)").unwrap().as_deref(), Some("([], 1)"));
	assert_eq!(
		text(&mut s, "async{n+1}.await").unwrap().as_deref(),
		Some("2")
	);
	assert_eq!(
		text(&mut s, "{let n=n;async move{n+1}.await}")
			.unwrap()
			.as_deref(),
		Some("2")
	);
	assert_eq!(text(&mut s, "n").unwrap().as_deref(), Some("1"));
	assert_eq!(
		text(&mut s, "let r=async move{n+1}.await;let n=7;(n,r)")
			.unwrap()
			.as_deref(),
		Some("(7, 2)")
	);
	text(&mut s, "let values=[7];0").unwrap();
	let error = text(&mut s, "async move{values[0]}.await").unwrap_err();
	assert!(
		check_error(&error).contains(&remedy::retained_async_move("values")),
		"{error}"
	);
	assert_eq!(
		text(&mut s, "async{values[0]}.await").unwrap().as_deref(),
		Some("7")
	);
	assert_eq!(
		text(&mut s, "{let values=values;async move{values[0]+1}.await}")
			.unwrap()
			.as_deref(),
		Some("8")
	);
	assert_eq!(
		text(&mut s, "values").unwrap().as_deref(),
		Some("<::std::vec::Vec>")
	);

	s.close().unwrap();
}
#[test]
fn future_value_forms_are_lazy_refusals_with_direct_block_remedies() {
	for do_move in [false, true] {
		let prefix = if do_move { "async move" } else { "async" };
		for source in [
			format!("{prefix}{{v.push(9);7}}"),
			format!("let future={prefix}{{v.push(9);7}};0"),
			format!("id({prefix}{{v.push(9);7}})"),
			format!("Some({prefix}{{v.push(9);7}})"),
			format!("fn bad(){{{prefix}{{7}}}}0"),
			format!("let future={prefix}{{v.push(9);7}};future.await"),
		] {
			let mut s = session();
			text(&mut s, "let v=[];fn id(x){x}0").unwrap();
			let error = text(&mut s, &source).unwrap_err();
			assert!(matches!(error, Failure::Refused(_)), "{source}: {error:?}");
			assert!(
				check_error(&error).contains(&remedy::async_block_value(do_move)),
				"{error}"
			);
			assert!(!error.to_string().contains("legacy"));
			assert_eq!(text(&mut s, "v").unwrap().as_deref(), Some("[]"));
			s.close().unwrap();
		}
	}
}
#[test]
fn loop_scope_and_named_diagnostics_do_not_expose_synthetic_owners() {
	for (source, reason) in [
		("loop{async{break}.await}", "Break outside of loop"),
		("loop{async{continue}.await}", "Continue outside of loop"),
		("'outer:loop{async{break 'outer}.await}", "Missing label"),
		(
			"fn f(){async{7}.await}",
			"outside of async function or block",
		),
		(
			"async fn f(){let n=1;async move{n}.await;n}",
			"Variable moved",
		),
	] {
		let mut s = session();
		let error = text(&mut s, source).unwrap_err();
		assert!(check_error(&error).contains(reason), "{error}");
		assert_eq!(text(&mut s, "7").unwrap().as_deref(), Some("7"));
		s.close().unwrap();
	}
	let mut s = session();
	text(&mut s, "async fn g(){7} fn ordinary(){7}").unwrap();
	for (source, owner) in [
		("async{g()}.await", ""),
		("async fn outer(){async{g()}.await}", "outer: "),
		(
			"async fn outer(){async{async{ordinary().await}.await}.await}",
			"outer: ",
		),
	] {
		let error = text(&mut s, source).unwrap_err();
		let message = check_error(&error);
		assert!(
			message.starts_with(&format!("refused: {owner}")),
			"{message}"
		);
		assert!(!message.contains("main:"), "{message}");
	}
	s.close().unwrap();
}
#[test]
fn errors_and_partial_capture_takes_keep_effects_and_next_input_usable() {
	let mut s = session();
	text(&mut s, "let v=[];0").unwrap();
	text(
		&mut s,
		"async fn fail(v){async{v.push(1);time::sleep(0).await;v.push(2);1/0}.await}",
	)
	.unwrap();
	let error = text(&mut s, "fail(v).await").unwrap_err();
	check_error(&error);
	assert!(
		matches!(
			error,
			Failure::Runtime {
				origin: Some(Origin { input: 2, .. }),
				..
			}
		),
		"{error:?}"
	);
	assert_eq!(text(&mut s, "v").unwrap().as_deref(), Some("[1, 2]"));
	text(
		&mut s,
		"async fn take(v){let w=v;async move{v.push(99);w.len()}.await}",
	)
	.unwrap();
	let error = text(&mut s, "take(v).await").unwrap_err();
	assert!(
		check_error(&error).contains("Cannot take the value: it has been moved"),
		"{error}"
	);
	assert_eq!(
		text(&mut s, "v").unwrap().as_deref(),
		Some("<::std::vec::Vec>")
	);
	assert_eq!(
		text(&mut s, "async{42}.await").unwrap().as_deref(),
		Some("42")
	);
	s.close().unwrap();
}
