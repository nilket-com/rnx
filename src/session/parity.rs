//! ADR0246: source-derived session coverage, confined to tests.
pub(crate) mod clocks;
mod conditionals;
mod constant_loops;
mod constant_patterns;
mod constant_structure;
pub(crate) mod external;
mod witnesses;
use quote::ToTokens;
use serde::Serialize;
use syn::{
	Expr, Item, Lit, Pat, Token,
	parse::Parser,
	punctuated::Punctuated,
	visit::{self, Visit},
};

#[derive(Debug, Serialize)]
struct Event {
	session: String,
	action: String,
	input: String,
}
#[derive(Debug, Default, Serialize)]
struct Sequence {
	name: String,
	sites: usize,
	classified_sites: usize,
	excluded: Vec<String>,
	events: Vec<Event>,
}
struct Extract {
	row: Sequence,
	reason: Option<&'static str>,
	array: Option<(String, Vec<String>)>,
}
fn literal(e: &Expr) -> Option<String> {
	if let Expr::Lit(e) = e
		&& let Lit::Str(s) = &e.lit
	{
		return Some(s.value());
	}
	None
}
fn variable(e: &Expr) -> Option<String> {
	match e {
		Expr::Path(p) => p.path.get_ident().map(ToString::to_string),
		Expr::Reference(r) => variable(&r.expr),
		_ => None,
	}
}
impl Extract {
	fn input(&mut self, session: &Expr, input: &Expr) {
		self.row.sites += 1;
		let values = literal(input).map(|s| vec![s]).or_else(|| {
			let (name, values) = self.array.as_ref()?;
			(variable(input).as_ref() == Some(name)).then(|| values.clone())
		});
		if let Some(reason) = self.reason {
			self.row.excluded.push(reason.into());
			self.row.events.push(Event {
				session: variable(session).unwrap_or_default(),
				action: "unclassified".into(),
				input: reason.into(),
			});
		} else if let (Some(session), Some(values)) = (variable(session), values) {
			self.row.classified_sites += 1;
			self.row
				.events
				.extend(values.into_iter().map(|input| Event {
					session: session.clone(),
					action: "eval".into(),
					input,
				}));
		} else {
			self.row.excluded.push("computed".into());
			self.row.events.push(Event {
				session: variable(session).unwrap_or_default(),
				action: "unclassified".into(),
				input: "computed".into(),
			});
		}
	}
}
impl<'a> Visit<'a> for Extract {
	fn visit_expr_method_call(&mut self, e: &'a syn::ExprMethodCall) {
		if e.method == "eval" && e.args.len() == 1 {
			self.input(&e.receiver, &e.args[0]);
			return;
		}
		if (e.method == "reset" || e.method == "renumber")
			&& e.args.is_empty()
			&& self.reason.is_none()
			&& let Some(session) = variable(&e.receiver)
		{
			self.row.events.push(Event {
				session,
				action: e.method.to_string(),
				input: String::new(),
			});
		}
		visit::visit_expr_method_call(self, e);
	}
	fn visit_expr_call(&mut self, e: &'a syn::ExprCall) {
		if variable(&e.func).as_deref() == Some("shown") && e.args.len() == 2 {
			self.input(&e.args[0], &e.args[1]);
			return;
		}
		visit::visit_expr_call(self, e);
	}
	fn visit_expr_for_loop(&mut self, e: &'a syn::ExprForLoop) {
		let old_reason = self.reason;
		let old_array = self.array.take();
		let binding = match (&*e.pat, &*e.expr) {
			(Pat::Ident(p), Expr::Array(a)) => a
				.elems
				.iter()
				.map(literal)
				.collect::<Option<Vec<_>>>()
				.map(|v| (p.ident.to_string(), v)),
			(Pat::Tuple(p), Expr::Array(a)) => match p.elems.first() {
				Some(Pat::Ident(p)) => a
					.elems
					.iter()
					.map(|e| match e {
						Expr::Tuple(t) => t.elems.first().and_then(literal),
						_ => None,
					})
					.collect::<Option<Vec<_>>>()
					.map(|v| (p.ident.to_string(), v)),
				_ => None,
			},
			_ => None,
		};
		if binding.is_none() {
			self.reason = Some(old_reason.unwrap_or("loop"));
		}
		self.array = binding;
		self.visit_block(&e.body);
		self.reason = old_reason;
		self.array = old_array;
	}
	fn visit_macro(&mut self, m: &'a syn::Macro) {
		// Assertions wrap many eval calls. Parse their expressions, not string text.
		if matches!(
			m.path.get_ident().map(ToString::to_string).as_deref(),
			Some("assert" | "assert_eq" | "assert_ne" | "matches")
		) {
			if let Ok(args) =
				Punctuated::<Expr, Token![,]>::parse_terminated.parse2(m.tokens.clone())
			{
				for e in &args {
					self.visit_expr(e);
				}
			} else {
				// matches! has a pattern after its first expression.
				let first = |input: syn::parse::ParseStream| input.parse::<Expr>();
				// The general token walker below retains the call expression before the comma.
				let tokens = m
					.tokens
					.clone()
					.into_iter()
					.take_while(
						|t| !matches!(t,proc_macro2::TokenTree::Punct(p) if p.as_char()==','),
					)
					.collect();
				if let Ok(e) = first.parse2(tokens) {
					self.visit_expr(&e);
				}
			}
		}
	}
}
fn sequences() -> Vec<Sequence> {
	fn walk(items: &[Item], prefix: &str, out: &mut Vec<Sequence>) {
		for item in items {
			match item {
				Item::Mod(m) => {
					if let Some((_, items)) = &m.content {
						walk(items, &format!("{prefix}{}::", m.ident), out)
					}
				}
				Item::Fn(f)
					if f.attrs.iter().any(|a| a.path().is_ident("test"))
						|| f.sig.ident == "checks" =>
				{
					let text = f.block.to_token_stream().to_string();
					let reason =
						if text.contains(". units . clear") || text.contains(". over_ceiling =") {
							Some("private")
						} else if text.contains("with_ceiling") {
							Some("ceiling")
						} else if text.contains("context . install") {
							Some("context")
						} else {
							None
						};
					let mut x = Extract {
						row: Sequence {
							name: format!("{prefix}{}", f.sig.ident),
							..Default::default()
						},
						reason,
						array: None,
					};
					x.visit_block(&f.block);
					out.push(x.row);
				}
				_ => {}
			}
		}
	}
	let file = syn::parse_file(include_str!("../session.rs")).unwrap();
	let mut out = Vec::new();
	walk(&file.items, "", &mut out);
	out
}
#[test]
fn source_coverage() {
	let rows = sequences();
	let sites: usize = rows.iter().map(|r| r.sites).sum();
	let classified: usize = rows.iter().map(|r| r.classified_sites).sum();
	let report = serde_json::json!({"sites":sites,"classified_sites":classified,"sequences":rows});
	if let Some(path) = std::env::var_os("RNX_PARITY_REPORT") {
		std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
	}
	assert!(
		classified * 3 >= sites * 2,
		"coverage STOP: {classified}/{sites}"
	);
}

fn context() -> rune::Context {
	let mut c = rune::Context::with_default_modules().unwrap();
	crate::install_core(&mut c).unwrap();
	crate::fs::install(&mut c).unwrap();
	c
}
fn literal_report() -> serde_json::Value {
	use super::{
		Session,
		domain_session::{DomainSession, transcripts::parity_observe},
	};
	use std::collections::BTreeMap;
	let witnessed = witnesses::report();
	let mut used_witnesses = 0;
	let mut rows = Vec::new();
	let mut sequence_summary = Vec::new();
	let mut defects = Vec::new();
	for sequence in sequences() {
		let start = rows.len();
		let mut sessions = BTreeMap::new();
		for (ordinal, event) in sequence.events.iter().enumerate() {
			let pair = sessions.entry(event.session.clone()).or_insert_with(|| {
				(
					Session::new(context()).unwrap(),
					DomainSession::new(context(), super::DEFAULT_CEILING).unwrap(),
					None::<String>,
				)
			});
			if event.action == "unclassified" {
				pair.2 = Some(format!("unclassified site {ordinal}"));
				continue;
			}
			if event.action == "reset" {
				pair.0.reset();
				pair.1.reset();
				pair.2 = None;
				continue;
			}
			if event.action == "renumber" {
				pair.0.renumber();
				pair.1.renumber();
				continue;
			}
			if let Some(blocker) = &pair.2 {
				let row = witnesses::blocked_row(
					&witnessed,
					&sequence.name,
					ordinal,
					&event.input,
					blocker,
				);
				if row.get("witness").is_some() {
					used_witnesses += 1;
				}
				rows.push(row);
				continue;
			}
			let (a, b) = parity_observe(&mut pair.0, &mut pair.1, &event.input);
			let (class, rule, worse) = classify(&a, &b);
			let row = serde_json::json!({"sequence":sequence.name,"ordinal":ordinal,"input":event.input,"legacy":a,"domain":b,"class":class,"wording_rule":rule,"domain_worse":worse});
			if matches!(
				class,
				"Different result" | "Domain-only failure" | "Unreviewed wording"
			) {
				defects.push(row.clone());
			}
			rows.push(row);
			if class == "Refused at admission" {
				pair.2 = Some(format!("row {ordinal}"));
			}
			if !defects.is_empty() {
				break;
			}
		}
		let selected = &rows[start..];
		let until = selected
			.iter()
			.take_while(|r| r["class"] != "Refused at admission" && r["class"] != "Not classified")
			.filter(|r| r["domain"]["outcome"] == "ok")
			.count();
		if !sequence.events.is_empty() {
			sequence_summary.push(serde_json::json!({"sequence":sequence.name,"successful_inputs_before_first_gap":until,"clean_end_to_end":selected.iter().all(|r|matches!(r["class"].as_str(),Some("Equal"|"Wording")))&&sequence.excluded.is_empty()}));
		}
		for (_, (old, mut fresh, _)) in sessions {
			let _ = old.close();
			let _ = fresh.close();
		}
		if !defects.is_empty() {
			break;
		}
	}
	assert_eq!(used_witnesses, witnessed.len(), "unused witness target");
	let wording = rows
		.iter()
		.filter(|r| matches!(r["class"].as_str(), Some("Wording" | "Wording candidate")))
		.count();
	let report = serde_json::json!({"rows":rows,"sequences":sequence_summary,"defects":defects,"wording_count":wording,"status":if defects.is_empty() {"PASS"} else {"STOP"}});
	if let Some(path) = std::env::var_os("RNX_PARITY_REPORT") {
		std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
	}
	assert!(defects.is_empty(), "parity STOP: {defects:#?}");
	report
}

fn constant_report() -> serde_json::Value {
	use super::{
		Session,
		domain_session::{DomainSession, transcripts::parity_observe},
	};
	let mut rows = Vec::new();
	for (construct, input) in [
		("string constant", "const S = \"x\"; S"),
		("vector constant", "const T = [1, 2]; T"),
		("tuple constant", "const A=(1, \"x\"); A"),
		("object constant", "const A=#{a:1}; A"),
		("nested constant", "const A=[#{a:(1,[2,3])}]; A"),
		("constant reference", "const A=[1]; const B=[A,A]; B"),
		("constant indexing", "const A=[1,2]; const B=A[0]; B"),
		("constant blocks", "const A={let x=2;x+1}; A"),
		("constant if", "const A=if true {1}else{2}; A"),
		("constant functions", "const fn f(x){x+1} const A=f(2); A"),
		(
			"constant function recursion",
			"const fn f(n){if n==0{0}else{1+f(n-1)}} const A=f(8); A",
		),
		(
			"constant function early return",
			"const fn g(x){x} const fn f(){g(return 7)} const A=f(); A",
		),
		(
			"constant function wildcard",
			"const fn f(_,x){x+1} const A=f(99,2); A",
		),
		("constant option construction", "const A=Some(1); A"),
		("constant None", "const A=None; A"),
		("constant tuple field", "const A=(1,[2]);const B=A.1; B"),
		("constant object field", "const A=#{x:[3]};const B=A.x; B"),
		("constant tuple pattern", "const A={let (a,b)=(2,3);a+b};A"),
		(
			"constant object pattern",
			"const A={let #{a,b}=#{a:2,b:3};a+b};A",
		),
		(
			"constant vector rest pattern",
			"const A={let [a,..]=[2,3];a};A",
		),
		(
			"constant if let Some",
			"const A=if let Some(x)=Some(7){x}else{0};A",
		),
		(
			"constant if let None",
			"const A=if let Some(x)=None{x}else{0};A",
		),
		(
			"constant guarded match",
			"const A=match Some(7){Some(x) if x<4=>1,Some(x)=>x,None=>0};A",
		),
		("constant unmatched match", "const A=match 7{1=>2};A"),
		(
			"constant pattern parameter",
			"const fn f((a,b)){a+b} const A=f((2,3));A",
		),
		(
			"constant Option call match",
			"const fn f(){Some(7)}const fn g(){match f(){Some(x)=>x,None=>0}}const A=g();A",
		),
		(
			"constant byte scalar pattern",
			"const A=match b'a'{b'a'=>7,_=>0};A",
		),
		(
			"constant counting loop",
			"const A={let n=0;loop{n+=1;if n==3{break n}}};A",
		),
		(
			"constant while assignment",
			"const A={let n=0;while n<3{n+=1;}n};A",
		),
		(
			"constant for range",
			"const A={let n=0;for x in 0..4{n+=x;}n};A",
		),
		(
			"constant for vector",
			"const A={let n=0;for x in [1,2,3]{n+=x;}n};A",
		),
		("constant valued break", "const A=loop{break 7};A"),
		(
			"constant continue",
			"const A={let n=0;let s=0;while n<5{n+=1;if n==3{continue;}s+=n;}s};A",
		),
		(
			"constant loop return",
			"const fn f(){let n=0;loop{while n<5{n+=1;if n==3{return n;}}}}const A=f();A",
		),
		(
			"constant 10000 iterations",
			"const fn f(){let n=0;while n<10000{n+=1;}n}const A=f();A",
		),
		("constant bytes", "const A=b\"ab\"; A"),
	] {
		let mut old = Session::new(context()).unwrap();
		let mut new = DomainSession::new(context(), super::DEFAULT_CEILING).unwrap();
		let (legacy, domain) = parity_observe(&mut old, &mut new, input);
		rows.push(
			serde_json::json!({"construct":construct,"input":input,"legacy":legacy,"domain":domain}),
		);
		old.close().unwrap();
		new.close().unwrap();
	}
	if let Some(path) = std::env::var_os("RNX_PARITY_REPORT") {
		std::fs::write(path, serde_json::to_vec_pretty(&rows).unwrap()).unwrap();
	}
	serde_json::json!(rows)
}

// ADR0250: exact reviewed diagnostic pairs, never a general error allowance.
const MOVED_JSON_CASES: &[(&str, &str, usize)] = &[
	(
		"let moved=[1];drop(moved);json::stringify(moved)",
		"Err(\"cannot serialize ::std::vec::Vec (an external reference) at the value itself\")",
		27,
	),
	(
		"let moved=[1];let outer=[moved];drop(moved);json::stringify(outer)",
		"Err(\"cannot serialize ::std::vec::Vec (an external reference) at [0]\")",
		45,
	),
	(
		"let moved=String::from(\"x\");drop(moved);json::stringify(moved)",
		"Err(\"cannot serialize ::std::string::String (an external reference) at the value itself\")",
		41,
	),
];
fn moved_json_pair(value: &str, column: usize) -> (serde_json::Value, serde_json::Value) {
	(
		serde_json::json!({"outcome":"ok", "value":value}),
		serde_json::json!({"outcome":"runtime",
		"message":format!("Cannot read the value: it has been moved\n  at input 1, line 1, column {column}"),
		"origin":{"input":1,"line":1,"column":column}}),
	)
}
fn moved_json_report() -> serde_json::Value {
	let rows: Vec<_> = MOVED_JSON_CASES.iter().map(|(input, value, column)| {
		let mut row = observe_host("json::stringify", input,
			crate::routing_inventory::session_parity_context().unwrap()).unwrap();
		let (a,b) = moved_json_pair(value, *column);
		assert_eq!(row["legacy"], a);
		assert_eq!(row["domain"], b);
		assert_eq!(row["class"], "Intentional difference");
		row["reason"] = "Use after move raises the truthful access error before the host body instead of an external-reference serialization refusal.".into();
		row
	}).collect();
	serde_json::json!(rows)
}
#[test]
fn moved_json_rule_is_exact() {
	for (_, value, column) in MOVED_JSON_CASES {
		let (a, b) = moved_json_pair(value, *column);
		assert_eq!(
			classify(&a, &b),
			("Intentional difference", Some("use-after-move"), false)
		);
		for key in ["outcome", "message", "origin"] {
			let mut bad = b.clone();
			bad[key] = serde_json::Value::Null;
			assert_eq!(classify(&a, &bad).0, "Domain-only failure");
		}
		let mut bad = a.clone();
		bad["value"] = "Err(\"unreviewed refusal\")".into();
		assert_eq!(classify(&bad, &b).0, "Domain-only failure");
	}
}
#[test]
fn moved_json_diagnostics() {
	let rows = moved_json_report();
	assert_eq!(rows.as_array().unwrap().len(), MOVED_JSON_CASES.len());
}

fn classify(
	a: &serde_json::Value,
	b: &serde_json::Value,
) -> (&'static str, Option<&'static str>, bool) {
	if a == b {
		return ("Equal", None, false);
	}
	if b["outcome"] == "refused" && a["outcome"] == "ok" {
		return ("Refused at admission", None, false);
	}
	if a["outcome"] == "ok" && b["outcome"] == "ok" {
		return ("Different result", None, false);
	}
	if a["outcome"] == "ok" {
		if MOVED_JSON_CASES.iter().any(|(_, value, column)| {
			let (old, new) = moved_json_pair(value, *column);
			*a == old && *b == new
		}) {
			return ("Intentional difference", Some("use-after-move"), false);
		}
		return ("Domain-only failure", None, false);
	}
	if b["outcome"] == "ok" {
		return ("Different result", None, false);
	}
	let old = a["message"].as_str().unwrap_or("");
	let new = b["message"].as_str().unwrap_or("");
	if a["outcome"] == b["outcome"] && a["origin"] == b["origin"] {
		if new.strip_prefix(old).is_some_and(|r| {
			r.starts_with("\n  at ") && r.lines().skip(1).all(|l| l.starts_with("  at "))
		}) {
			return ("Wording", Some("appended-frames"), false);
		}
		if old == "no method `missing_method` on `std::i64`"
			&& new.starts_with("Type `std::i64` has no method named `missing_method`\n  at ")
		{
			return ("Wording", Some("missing-method"), false);
		}
	}
	("Unreviewed wording", None, false)
}
#[test]
fn notebook_probe() {
	use super::{
		Session,
		domain_session::{
			DomainSession,
			transcripts::{parity_new, parity_old},
		},
	};
	let Some(name) = std::env::var_os("RNX_PARITY_NOTEBOOK") else {
		return;
	};
	let notebook: serde_json::Value =
		serde_json::from_slice(&std::fs::read(name).unwrap()).unwrap();
	let mut old = Session::new(context()).unwrap();
	let mut fresh = DomainSession::new(context(), super::DEFAULT_CEILING).unwrap();
	let mut rows = Vec::new();
	let mut blocked = None;
	let mut stop = false;
	for (ordinal, cell) in notebook["cells"].as_array().unwrap().iter().enumerate() {
		if cell["cell_type"] != "code" {
			continue;
		}
		let input = cell["source"]
			.as_array()
			.unwrap()
			.iter()
			.map(|s| s.as_str().unwrap())
			.collect::<String>();
		if let Some(by) = blocked {
			rows.push(
				serde_json::json!({"ordinal":ordinal,"input":input,"class":"Not classified","blocked_by":by}),
			);
			continue;
		}
		println!("__PARITY_OLD_{ordinal}__");
		let a = parity_old(&mut old, &input);
		println!("__PARITY_NEW_{ordinal}__");
		let b = parity_new(&mut fresh, &input);
		println!("__PARITY_END_{ordinal}__");
		let (class, rule, worse) = classify(&a, &b);
		rows.push(serde_json::json!({"ordinal":ordinal,"input":input,"legacy":a,"domain":b,"class":class,"wording_rule":rule,"domain_worse":worse}));
		if class == "Refused at admission" {
			blocked = Some(ordinal)
		}
		if matches!(
			class,
			"Different result" | "Domain-only failure" | "Unreviewed wording"
		) {
			stop = true;
			break;
		}
	}
	old.close().unwrap();
	fresh.close().unwrap();
	let report = serde_json::json!({"status":if stop {"STOP"} else {"PASS"},"rows":rows});
	std::fs::write(
		std::env::var_os("RNX_PARITY_REPORT").unwrap(),
		serde_json::to_vec_pretty(&report).unwrap(),
	)
	.unwrap();
	assert!(!stop, "notebook classification STOP: {report}");
}

pub(crate) fn observe_host(
	function: &str,
	input: &str,
	c: rune::Context,
) -> crate::routing_inventory::Result<serde_json::Value> {
	use super::{
		Session,
		domain_session::{DomainSession, transcripts::parity_observe},
	};
	let mut old = Session::new(c)?;
	let mut fresh = DomainSession::new(
		crate::routing_inventory::session_parity_context()?,
		super::DEFAULT_CEILING,
	)?;
	let (a, b) = parity_observe(&mut old, &mut fresh, input);
	let (class, rule, worse) = classify(&a, &b);
	old.close()?;
	fresh.close()?;
	Ok(
		serde_json::json!({"function":function,"input":input,"legacy":a,"domain":b,"class":class,"wording_rule":rule,"domain_worse":worse}),
	)
}
fn host_report() -> serde_json::Value {
	let rows = crate::routing_inventory::session_parity_hosts(false).unwrap();
	let stop = rows.as_array().unwrap().iter().any(|r| {
		matches!(
			r["class"].as_str(),
			Some(
				"Different result"
					| "Domain-only failure"
					| "Unreviewed wording"
					| "Property failure"
			)
		)
	});
	let report = serde_json::json!({"status":if stop {"STOP"} else {"PASS"},"rows":rows});
	if let Some(path) = std::env::var_os("RNX_PARITY_REPORT") {
		std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
	}
	assert!(!stop, "host classification STOP: {report}");
	report
}

fn notebook_reports() -> serde_json::Value {
	let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
	let temporary = std::env::temp_dir().join(format!(
		"rnx-parity-{}-{}",
		std::process::id(),
		std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.unwrap()
			.as_nanos()
	));
	std::fs::create_dir(&temporary).unwrap();
	let mut reports = Vec::new();
	for name in ["01_mortgage", "02_orders", "03_report"] {
		let receipt = temporary.join(format!("{name}.json"));
		let output = std::process::Command::new(std::env::current_exe().unwrap())
			.args(["--exact", "session::parity::notebook_probe", "--nocapture"])
			.current_dir(root.join("demos/notebooks"))
			.env("RNX_PARITY_NOTEBOOK", format!("{name}.ipynb"))
			.env("RNX_PARITY_REPORT", &receipt)
			.output()
			.unwrap();
		let mut report: serde_json::Value =
			serde_json::from_slice(&std::fs::read(&receipt).unwrap()).unwrap();
		let stdout = String::from_utf8(output.stdout.clone()).unwrap();
		for row in report["rows"].as_array_mut().unwrap() {
			if row["class"] == "Not classified" {
				continue;
			}
			let n = row["ordinal"].as_u64().unwrap();
			let body = stdout
				.split_once(&format!("__PARITY_OLD_{n}__\n"))
				.unwrap()
				.1;
			let (old, rest) = body.split_once(&format!("__PARITY_NEW_{n}__\n")).unwrap();
			let new = rest.split_once(&format!("__PARITY_END_{n}__\n")).unwrap().0;
			row["legacy"]["stdout"] = old.into();
			row["domain"]["stdout"] = new.into();
			assert_eq!(
				old, new,
				"Different result: notebook {name}, cell {n}, stdout"
			);
		}
		assert!(output.status.success(), "notebook STOP {name}: {report}");
		reports.push(serde_json::json!({"source":format!("demos/notebooks/{name}.ipynb"),"report":report,"observation":"values, diagnostics and per-cell stdout"}));
	}
	std::fs::remove_dir_all(temporary).unwrap();
	serde_json::json!(reports)
}
fn ledger() -> serde_json::Value {
	let sources = sequences();
	let sites: usize = sources.iter().map(|s| s.sites).sum();
	let classified: usize = sources.iter().map(|s| s.classified_sites).sum();
	let mut notebooks = Vec::new();
	for name in [
		"01_mortgage",
		"02_orders",
		"03_report",
		"04_polars_sales",
		"05_candle_model",
		"06_candle_search",
	] {
		let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
			.join(format!("demos/notebooks/{name}.ipynb"));
		let book: serde_json::Value =
			serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
		let count = book["cells"]
			.as_array()
			.unwrap()
			.iter()
			.filter(|c| c["cell_type"] == "code")
			.count();
		notebooks.push(serde_json::json!({"source":name,"inputs":count,"coverage":if name<"04" {"classified"} else {"not classified"},"reason":if name.starts_with("04") {"Polars kernel"} else if name.starts_with("05") {"Candle kernel"} else if name.starts_with("06") {"Candle kernel and external model"} else {"stock"}}));
	}
	serde_json::json!({"legacy_sources":{"source":"src/session.rs","sites":sites,"classified_sites":classified,"functions":sources},"notebooks":notebooks,"referenced_fixtures":super::domain_session::transcripts::parity_ledger(),"worker_fixture":worker_ledger(),"not_classified":[{"source":"tests/repl.rs","tests":26,"reason":"PTY"},{"source":"tests/repl_execution.rs","tests":3,"reason":"terminal/options/signals"},{"source":"README.md :dep example","events":2,"reason":"adapter restart"}]})
}
fn summary(rows: &[serde_json::Value]) -> serde_json::Value {
	use std::collections::BTreeMap;
	let mut classes = BTreeMap::<String, usize>::new();
	let mut groups = BTreeMap::<String, BTreeMap<String, usize>>::new();
	for row in rows {
		*classes
			.entry(row["class"].as_str().unwrap_or("unknown").into())
			.or_default() += 1;
		let mut entries = Vec::new();
		if row["domain_worse"] == true {
			entries.push((
				"diagnostics to improve",
				row["wording_rule"].as_str().unwrap_or("diagnostic"),
			));
		}
		if row.get("diagnostic_issue").is_some() {
			entries.push((
				"diagnostics to improve",
				row["diagnostic_issue"].as_str().unwrap(),
			));
		}
		if row["class"] == "Refused for a host function" {
			entries.push((
				row["work_group"].as_str().unwrap_or("host enrollment"),
				row["construct"].as_str().unwrap_or("host"),
			));
		}
		if row["class"] == "Refused at admission" {
			entries.push((
				"source forms",
				row["construct"].as_str().unwrap_or("source"),
			));
		}
		for (group, construct) in entries {
			*groups
				.entry(group.into())
				.or_default()
				.entry(construct.into())
				.or_default() += 1;
		}
	}
	let groups: BTreeMap<_, _> = groups
		.into_iter()
		.map(|(name, constructs)| {
			let count: usize = constructs.values().sum();
			let mut ranked: Vec<_> = constructs.into_iter().collect();
			ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
			ranked.truncate(3);
			(
				name,
				serde_json::json!({"count":count,"top_constructs":ranked}),
			)
		})
		.collect();
	serde_json::json!({"classes":classes,"work_groups":groups})
}

// Keep every classed observation on its own line, including notebook rows.
// The summary is one line so a reclassification changes only those two lines.
fn serialize_table(value: &serde_json::Value) -> String {
	fn render(value: &serde_json::Value, depth: usize, compact: bool) -> String {
		if compact || value.get("class").is_some() {
			return serde_json::to_string(value).unwrap();
		}
		let pad = "\t".repeat(depth);
		let child_pad = "\t".repeat(depth + 1);
		match value {
			serde_json::Value::Object(fields) if !fields.is_empty() => {
				let lines = fields
					.iter()
					.map(|(key, value)| {
						format!(
							"{child_pad}{}: {}",
							serde_json::to_string(key).unwrap(),
							render(value, depth + 1, key == "summary")
						)
					})
					.collect::<Vec<_>>();
				format!("{{\n{}\n{pad}}}", lines.join(",\n"))
			}
			serde_json::Value::Array(items) if !items.is_empty() => {
				let lines = items
					.iter()
					.map(|item| format!("{child_pad}{}", render(item, depth + 1, false)))
					.collect::<Vec<_>>();
				format!("[\n{}\n{pad}]", lines.join(",\n"))
			}
			_ => serde_json::to_string(value).unwrap(),
		}
	}
	format!("{}\n", render(value, 0, false))
}

#[test]
fn serializer_preserves_row_diffs() {
	let pin: serde_json::Value =
		serde_json::from_str(include_str!("../../tests/fixtures/session_parity.json")).unwrap();
	let before = serialize_table(&pin);
	assert_eq!(
		serde_json::from_str::<serde_json::Value>(&before).unwrap(),
		pin
	);
	assert!(before.len() <= 163840);
	let mut changed = pin.clone();
	assert_eq!(changed["hosts"]["rows"][0]["class"], "Equal");
	changed["hosts"]["rows"][0]["class"] = "Different result".into();
	let classes = &mut changed["summary"]["classes"];
	classes["Equal"] = (classes["Equal"].as_u64().unwrap() - 1).into();
	classes["Different result"] = 1.into();
	let after = serialize_table(&changed);
	let a: Vec<_> = before.lines().collect();
	let b: Vec<_> = after.lines().collect();
	assert_eq!(a.len(), b.len());
	let delta: Vec<_> = a.iter().zip(&b).filter(|(a, b)| a != b).collect();
	assert_eq!(delta.len(), 2);
	assert!(
		delta
			.iter()
			.any(|(a, _)| a.contains("\"function\":\"json::parse\""))
	);
	assert!(
		delta
			.iter()
			.any(|(a, _)| a.trim_start().starts_with("\"summary\":"))
	);
}

#[test]
fn pinned_classification() {
	let literals = literal_report();
	let hosts = host_report();
	let notebooks = notebook_reports();
	let constants = constant_report();
	let moved_json = moved_json_report();
	let conditionals = conditionals::report();
	let mut probes = Vec::new();
	for row in constants.as_array().unwrap() {
		let (class, rule, worse) = classify(&row["legacy"], &row["domain"]);
		let mut row = row.clone();
		row["class"] = class.into();
		row["wording_rule"] = serde_json::json!(rule);
		row["domain_worse"] = worse.into();

		probes.push(row);
	}
	let mut all = literals["rows"].as_array().unwrap().clone();
	all.extend(hosts["rows"].as_array().unwrap().iter().cloned());
	all.extend(probes.iter().cloned());
	all.extend(moved_json.as_array().unwrap().iter().cloned());
	all.extend(conditionals.as_array().unwrap().iter().cloned());
	for book in notebooks.as_array().unwrap() {
		all.extend(book["report"]["rows"].as_array().unwrap().iter().cloned());
	}
	let report = serde_json::json!({"ledger":ledger(),"literal_sessions":literals,"hosts":hosts,"notebooks":notebooks,"constant_probes":probes,"conditional_probes":conditionals,"moved_json":moved_json,"summary":summary(&all)});
	if let Some(path) = std::env::var_os("RNX_PARITY_REPORT") {
		std::fs::write(path, serialize_table(&report).into_bytes()).unwrap();
	}
	let pin =
		std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/session_parity.json");
	let expected: serde_json::Value =
		serde_json::from_slice(&std::fs::read(&pin).expect("classification not pinned yet"))
			.unwrap();
	assert_eq!(report, expected, "session parity pin changed");
	assert_eq!(
		std::fs::read_to_string(pin).unwrap(),
		serialize_table(&report),
		"pin serialization differs from generator"
	);
}

#[test]
fn removing_host_enrollment_changes_only_its_witness() {
	let baseline = crate::routing_inventory::session_parity_hosts(false).unwrap();
	let changed = crate::routing_inventory::session_parity_hosts(true).unwrap();
	let a = baseline.as_array().unwrap();
	let b = changed.as_array().unwrap();
	assert_eq!(a.len(), b.len());
	let deltas: Vec<_> = a.iter().zip(b).filter(|(a, b)| a != b).collect();
	assert_eq!(deltas.len(), 1);
	assert_eq!(deltas[0].0["function"], "text::group_digits");
	assert_eq!(deltas[0].0["class"], "Equal");
	assert_eq!(deltas[0].1["class"], "Refused for a host function");
	let pinned: serde_json::Value =
		serde_json::from_str(include_str!("../../tests/fixtures/session_parity.json")).unwrap();
	assert_ne!(pinned["hosts"]["rows"], changed);
	let mut moved = pinned.clone();
	moved["hosts"]["rows"][0]["class"] = "Different result".into();
	assert_ne!(pinned, moved);
}
#[test]
fn extractor_records_computed_sites_and_literal_order() {
	let block:syn::Block=syn::parse_str(r#"{s.eval("one");for input in ["two","three"] {s.eval(input);} s.reset();s.eval(&format!("{}",4));}"#).unwrap();
	let mut x = Extract {
		row: Sequence::default(),
		reason: None,
		array: None,
	};
	x.visit_block(&block);
	assert_eq!((x.row.sites, x.row.classified_sites), (3, 2));
	assert_eq!(
		x.row
			.events
			.iter()
			.map(|e| e.action.as_str())
			.collect::<Vec<_>>(),
		["eval", "eval", "eval", "reset", "unclassified"]
	);
}

fn worker_ledger() -> serde_json::Value {
	let script = "import ast,json,sys; t=ast.parse(open(sys.argv[1]).read()); print(json.dumps(next(ast.literal_eval(n.value) for n in t.body if isinstance(n,ast.Assign) and any(isinstance(x,ast.Name) and x.id=='TABLE' for x in n.targets))))";
	let out = std::process::Command::new("python3")
		.args(["-c", script])
		.arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/worker_execution.py"))
		.output()
		.unwrap();
	assert!(out.status.success(), "worker fixture AST extraction failed");
	serde_json::json!({"source":"tests/worker_execution.py::TABLE","existing_rows":serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap(),"coverage":"referenced existing differential fixture; historical Policy labels are not new policy decisions"})
}
