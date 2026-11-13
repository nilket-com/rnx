//! ADR 0266 measured runtime loop-control rows; classifications are not presumed.
use super::context;
use crate::session::{
	DEFAULT_CEILING, Session,
	domain_session::{DomainSession, transcripts::parity_observe},
};
pub(super) fn report() -> serde_json::Value {
	let mut rows = Vec::new();
	for (name, input) in [
		("valued_for", "for x in [1]{break 7}"),
		("valued_while", "while true{break 7}"),
		("valued_while_let", "while let Some(x)=Some(1){break 7}"),
		(
			"while_break_plain",
			"{let n=0;loop{n+=1;while {break;false} {}if n==3{break;}}n}",
		),
		(
			"while_break_self",
			"{let n=0;loop{n+=1;'w: while {break 'w;false} {}if n==3{break;}}n}",
		),
		(
			"while_break_outer",
			"{let n=0;'outer: loop{n+=1;if n==3{break;}while {break 'outer;false} {}}n}",
		),
		(
			"while_continue_plain",
			"{let n=0;while {n+=1;if n<3{continue;}false} {}n}",
		),
		(
			"while_continue_self",
			"{let n=0;'w: while {n+=1;if n<3{continue 'w;}false} {}n}",
		),
		(
			"while_continue_outer",
			"{let n=0;'outer: loop{n+=1;if n==3{break;}while {n+=1;if n<3{continue 'outer;}false} {}}n}",
		),
		(
			"while_let_break_plain",
			"{let n=0;loop{n+=1;while let Some(x)={break;None} {}if n==3{break;}}n}",
		),
		(
			"while_let_break_self",
			"{let n=0;loop{n+=1;'w: while let Some(x)={break 'w;None} {}if n==3{break;}}n}",
		),
		(
			"while_let_break_outer",
			"{let n=0;'outer: loop{n+=1;if n==3{break;}while let Some(x)={break 'outer;None} {}}n}",
		),
		(
			"while_let_continue_plain",
			"{let n=0;while let Some(x)={n+=1;if n<3{continue;}None} {}n}",
		),
		(
			"while_let_continue_self",
			"{let n=0;'w: while let Some(x)={n+=1;if n<3{continue 'w;}None} {}n}",
		),
		(
			"while_let_continue_outer",
			"{let n=0;'outer: loop{n+=1;if n==3{break;}while let Some(x)={n+=1;if n<3{continue 'outer;}None} {}}n}",
		),
		(
			"for_iter_break_plain",
			"{let n=0;'outer: loop{n+=1;for x in {if n<3{break;}[1]} {}if n==3{break;}}n}",
		),
		(
			"for_iter_break_self",
			"{let n=0;'outer: loop{n+=1;'f: for x in {if n<3{break 'f;}[1]} {}if n==3{break;}}n}",
		),
		(
			"for_iter_break_outer",
			"{let n=0;'outer: loop{n+=1;for x in {if n<3{break 'outer;}[1]} {}if n==3{break;}}n}",
		),
		(
			"for_iter_continue_plain",
			"{let n=0;'outer: loop{n+=1;for x in {if n<3{continue;}[1]} {}if n==3{break;}}n}",
		),
		(
			"for_iter_continue_self",
			"{let n=0;'outer: loop{n+=1;'f: for x in {if n<3{continue 'f;}[1]} {}if n==3{break;}}n}",
		),
		(
			"for_iter_continue_outer",
			"{let n=0;'outer: loop{n+=1;for x in {if n<3{continue 'outer;}[1]} {}if n==3{break;}}n}",
		),
		(
			"while_inner_break",
			"{let n=0;while {let x=loop{n+=1;break true;};n<3} {} n}",
		),
		(
			"while_let_inner_break",
			"{let n=0;while let Some(x)={loop{n+=1;if n<3{break Some(n);}break None;}} {} n}",
		),
		(
			"for_inner_break",
			"{let n=0;for x in loop{break [1,2,3];}{n+=x;}n}",
		),
		("while_unknown_label", "while {break 'missing;false} {}"),
		(
			"while_let_unknown_label",
			"while let Some(x)={continue 'missing;None} {}",
		),
		("for_unknown_label", "for x in {break 'missing;[1]} {}"),
		("body_unknown_label", "loop{break 'missing}"),
		(
			"body_label_outer_valued_for",
			"'outer:loop{for x in [1]{break 'outer 7;}}",
		),
		("body_label_while_value", "'w:while true{break 'w 7}"),
	] {
		let mut old = Session::new(context()).unwrap();
		let mut new = DomainSession::new(context(), DEFAULT_CEILING).unwrap();
		let (legacy, domain) = parity_observe(&mut old, &mut new, input);
		let (class, rule, worse) = super::classify(&legacy, &domain);
		rows.push(serde_json::json!({"construct":name,"input":input,"legacy":legacy,"domain":domain,"class":class,"wording_rule":rule,"domain_worse":worse}));
		old.close().unwrap();
		new.close().unwrap();
	}
	serde_json::json!(rows)
}
#[test]
fn valued_for_is_a_source_error() {
	use crate::session::domain_session::transcripts::{parity_new, parity_old};
	let mut old = Session::new(context()).unwrap();
	let mut new = DomainSession::new(context(), DEFAULT_CEILING).unwrap();
	for input in ["for x in [1]{break 7}", "for x in [1]{break 7;}0"] {
		let a = parity_old(&mut old, input);
		let b = parity_new(&mut new, input);
		assert_eq!(b["outcome"], "compile", "{b}");
		assert!(b.to_string().contains("Can only break with a value"), "{b}");
		assert_eq!(a, b, "{input}");
	}
	old.close().unwrap();
	new.close().unwrap();
}
