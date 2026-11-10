//! Cheap standalone preflight: no Rune or rnx dependency or runtime build.
use quote::ToTokens;
use syn::{
	visit::{self, Visit},
	visit_mut::{self, VisitMut},
};
fn canonical(stream: proc_macro2::TokenStream) -> String {
	stream
		.into_iter()
		.map(|t| match t {
			proc_macro2::TokenTree::Group(g) => {
				let (a, b) = match g.delimiter() {
					proc_macro2::Delimiter::Brace => ("{", "}"),
					proc_macro2::Delimiter::Bracket => ("[", "]"),
					proc_macro2::Delimiter::Parenthesis => ("(", ")"),
					proc_macro2::Delimiter::None => ("<none>", "</none>"),
				};
				format!("{a} {} {b}", canonical(g.stream()))
			}
			t => t.to_string(),
		})
		.collect::<Vec<_>>()
		.join(" ")
}
fn tokens(t: &impl ToTokens) -> String {
	canonical(t.to_token_stream())
}
fn parsed(s: &str) -> String {
	canonical(s.parse().unwrap())
}
fn require(ok: bool, why: &str) -> Result<(), String> {
	if ok {
		Ok(())
	} else {
		Err(why.into())
	}
}
// Diagnostic token positions are zero-based within the named region. Literal
// tokens stay whole, including embedded spaces; punctuation is not discarded.
fn diagnostic_tokens(text: &str) -> Vec<String> {
	fn flatten(stream: proc_macro2::TokenStream, out: &mut Vec<String>) {
		for token in stream {
			if let proc_macro2::TokenTree::Group(g) = token {
				out.push(format!("open:{:?}", g.delimiter()));
				flatten(g.stream(), out);
				out.push(format!("close:{:?}", g.delimiter()));
			} else {
				out.push(token.to_string());
			}
		}
	}
	let mut out = Vec::new();
	flatten(text.parse().expect("canonical Rust tokens"), &mut out);
	out
}
fn equal(region: &str, complete: &str, sliced: &str) -> Result<(), String> {
	if complete == sliced {
		return Ok(());
	}
	let a = diagnostic_tokens(complete);
	let b = diagnostic_tokens(sliced);
	let pos = a
		.iter()
		.zip(&b)
		.position(|(a, b)| a != b)
		.unwrap_or(a.len().min(b.len()));
	let context =
		|v: &[String]| v[pos.saturating_sub(5).min(v.len())..(pos + 6).min(v.len())].join(" ");
	Err(format!(
		"{region}: first differing token {pos}; complete [{}]; sliced [{}]",
		context(&a),
		context(&b)
	))
}
#[derive(Default)]
struct Methods {
	methods: Vec<syn::ImplItemFn>,
}
impl<'a> Visit<'a> for Methods {
	fn visit_impl_item_fn(&mut self, f: &'a syn::ImplItemFn) {
		self.methods.push(f.clone());
	}
}
fn method(file: &syn::File, name: &str) -> Result<syn::ImplItemFn, String> {
	let mut v = Methods::default();
	v.visit_file(file);
	let mut found = v.methods.into_iter().filter(|f| f.sig.ident == name);
	let first = found.next().ok_or(format!("missing {name}"))?;
	require(found.next().is_none(), &format!("duplicate {name}"))?;
	Ok(first)
}
#[derive(Default)]
struct Regions {
	op: Vec<Vec<syn::Stmt>>,
	enrich: Vec<syn::Block>,
	cleanup: Vec<syn::Block>,
	charged: Vec<syn::Block>,
}
impl<'a> Visit<'a> for Regions {
	fn visit_expr_if(&mut self, e: &'a syn::ExprIf) {
		if tokens(&e.cond) == parsed("instruction.charged") {
			self.charged.push(e.then_branch.clone());
		}
		visit::visit_expr_if(self, e);
	}
	fn visit_block(&mut self, b: &'a syn::Block) {
		for (i, s) in b.stmts.iter().enumerate() {
			if tokens(s) == parsed("pc += 1;") {
				self.op.push(b.stmts[i..].to_vec());
			}
		}
		visit::visit_block(self, b);
	}
	fn visit_arm(&mut self, a: &'a syn::Arm) {
		if let syn::Expr::Block(b) = a.body.as_ref() {
			if tokens(&a.pat) == parsed("Err(e)") && tokens(&b.block).contains("enrich_error") {
				self.enrich.push(b.block.clone());
			}
			if tokens(&a.pat) == parsed("Err(mut failure)") {
				self.cleanup.push(b.block.clone());
			}
		}
		visit::visit_arm(self, a);
	}
}
// Only these relative paths occur in the copied opcode region. All other tokens
// remain exact. The checker refuses any other parent qualification in the region.
struct CorePaths {
	error: Option<String>,
}
impl VisitMut for CorePaths {
	fn visit_macro_mut(&mut self, m: &mut syn::Macro) {
		match macro_paths(m.tokens.clone()) {
			Ok(t) => m.tokens = t,
			Err(e) => self.error = Some(e),
		}
	}

	fn visit_path_mut(&mut self, p: &mut syn::Path) {
		let names: Vec<_> = p.segments.iter().map(|s| s.ident.to_string()).collect();
		if names.starts_with(&["super".into(), "super".into()]) {
			let allowed = (names.len() >= 4 && names[2..4] == ["value", "Type"])
				|| names[2..] == ["region", "recursion", "observe_return"];
			if !allowed {
				self.error = Some(format!("unreviewed child path: {}", tokens(p)));
				return;
			}
			p.segments = p.segments.iter().skip(1).cloned().collect();
		}
		visit_mut::visit_path_mut(self, p)
	}
}
fn macro_paths(stream: proc_macro2::TokenStream) -> Result<proc_macro2::TokenStream, String> {
	use proc_macro2::{Group, TokenStream, TokenTree};
	let input: Vec<_> = stream.into_iter().collect();
	let mut out = TokenStream::new();
	let mut i = 0;
	while i < input.len() {
		if let TokenTree::Group(g) = &input[i] {
			out.extend([TokenTree::Group(Group::new(
				g.delimiter(),
				macro_paths(g.stream())?,
			))]);
			i += 1;
			continue;
		}
		let prefix: TokenStream = input[i..input.len().min(i + 6)].iter().cloned().collect();
		if canonical(prefix) == parsed("super::super::") {
			let tail: TokenStream = input[i..input.len().min(i + 10)].iter().cloned().collect();
			require(
				canonical(tail) == parsed("super::super::value::Type"),
				"unreviewed path in macro tokens",
			)?;
			i += 3;
		}
		out.extend([input[i].clone()]);
		i += 1;
	}
	Ok(out)
}
struct HostAdapter {
	count: usize,
	error: Option<String>,
}
impl VisitMut for HostAdapter {
	fn visit_expr_mut(&mut self, e: &mut syn::Expr) {
		if let syn::Expr::MethodCall(c) = e {
			if c.method == "budgeted_sliced_intrinsic" {
				self.count += 1;
				let expected: syn::Expr=syn::parse_str("self.budgeted_sliced_intrinsic(code, store, function, frame.base, out, callee, args_start, args_len, &mut fuel, cross,)").unwrap();
				if tokens(e) != tokens(&expected) {
					self.error = Some("host adapter hook changed".into());
					return;
				}
				*e = syn::parse_str(
					"self.intrinsic(code, store, function, frame.base, out, callee, args_start, args_len,)",
				)
				.unwrap();
				return;
			}
		}
		visit_mut::visit_expr_mut(self, e)
	}
}

// Narrow ADR0238 hooks. Their complete-side semantics and sliced call sites
// are both pinned as syntax; all non-hook operation tokens remain compared.
struct CrossHooks {
	call_value: usize,
	leave: usize,
	prepare: usize,
	publish: usize,
	error: Option<String>,
}
impl VisitMut for CrossHooks {
	fn visit_arm_mut(&mut self, a: &mut syn::Arm) {
		if tokens(&a.pat) == parsed("Op::CallValue {out,value,args_start,args_len,}") {
			self.call_value += 1;
			if tokens(a.body.as_ref()) != parsed(CROSS_CALL) {
				self.error = Some("cross CallValue hook drift".into());
				return;
			}
			a.body = Box::new(syn::parse_str(COMPLETE_CALL).unwrap());
			return;
		}
		visit_mut::visit_arm_mut(self, a);
	}
	fn visit_block_mut(&mut self, b: &mut syn::Block) {
		b.stmts.retain(|s| {
			let t = tokens(s);
			if t == parsed("Self::cross_prepare_owner(cross,code.id())?;") {
				self.prepare += 1;
				false
			} else if t == parsed("Self::cross_publish_owner(cross,code.id());") {
				self.publish += 1;
				false
			} else {
				true
			}
		});
		visit_mut::visit_block_mut(self, b);
	}
	fn visit_expr_mut(&mut self, e: &mut syn::Expr) {
		if let syn::Expr::MethodCall(c) = e {
			if c.method == "sliced_leave" {
				self.leave += 1;
				if tokens(e) != parsed("self.sliced_leave(store,cross)") {
					self.error = Some("cross leave hook drift".into());
					return;
				}
				*e = syn::parse_str("self.leave(store)").unwrap();
				return;
			}
		}
		visit_mut::visit_expr_mut(self, e);
	}
}
const COMPLETE_CALL: &str = r###"{
								let called = get(value)?;
								match called {
									Value::Function {
										code: owner,
										id: callee,
									} => {
										// Keep the plain-function validation and dispatch path unchanged.
										if owner != code.id() {
											return Err(error(ErrorKind::Type));
										}
										let target = code
											.function(callee)
											.ok_or_else(|| error(ErrorKind::InvalidCode))?;
										if target.parameters != args_len {
											return Err(error(ErrorKind::Type));
										}
										call = Some((out, callee, args_start, args_len));
									}
									Value::Heap(_) => {
										self.call_closure(
											code,
											store,
											function,
											frame.base,
											frame_index,
											pc,
											source_pc,
											out,
											called,
											args_start,
											args_len,
										)
										.map_err(|failure| {
											setup_cleanup_error = failure.cleanup_error;
											failure.error
										})?;
										transferred = true;
									}
									_ => return Err(error(ErrorKind::Type)),
								}
							}"###;
const CROSS_CALL: &str = r###"{self.sliced_call_value(code,store,function,frame.base,frame_index,pc,source_pc,out,get(value)?,args_start,args_len,cross).map_err(|failure| {setup_cleanup_error=failure.cleanup_error;failure.error})?;transferred=true;}"###;

fn stmts(s: &[syn::Stmt]) -> String {
	s.iter().map(tokens).collect::<Vec<_>>().join(" ")
}
fn guard(fixed: &str, sliced: &str) -> Result<(), String> {
	let a = syn::parse_file(fixed).map_err(|e| e.to_string())?;
	let b = syn::parse_file(sliced).map_err(|e| e.to_string())?;
	let complete = method(&a, "invoke_args")?;
	let resume = method(&b, "resume_sliced")?;
	let mut ar = Regions::default();
	ar.visit_block(&complete.block);
	let mut br = Regions::default();
	br.visit_block(&resume.block);
	require(
		ar.op.len() == 1 && br.op.len() == 1,
		"opcode region missing or duplicated",
	)?;
	require(
		ar.enrich.len() == 1 && br.enrich.len() == 1,
		"enrichment missing or duplicated",
	)?;
	require(
		ar.cleanup.len() == 1 && br.cleanup.len() == 1,
		"cleanup missing or duplicated",
	)?;
	require(
		ar.charged.len() == 1 && br.charged.len() == 1,
		"charge block missing or duplicated",
	)?;
	let mut charge = br.charged[0].clone();
	require(
		charge
			.stmts
			.last()
			.is_some_and(|s| tokens(s) == parsed("soft -= 1;")),
		"soft debit position",
	)?;
	charge.stmts.pop();
	require(
		tokens(&charge) == tokens(&ar.charged[0]),
		"charge/cancellation hook drift",
	)?;
	// Validate the fixed hook before substituting the sliced hook.
	require(
		stmts(&ar.op[0]).contains(&parsed(COMPLETE_CALL)),
		"complete CallValue hook drift",
	)?;
	let mut cross = CrossHooks {
		call_value: 0,
		leave: 0,
		prepare: 0,
		publish: 0,
		error: None,
	};
	for stmt in &mut br.op[0] {
		cross.visit_stmt_mut(stmt);
	}
	if let Some(error) = cross.error {
		return Err(error);
	}
	require(
		(cross.call_value, cross.leave, cross.prepare, cross.publish) == (1, 2, 1, 1),
		&format!(
			"cross hook counts: {:?}",
			(cross.call_value, cross.leave, cross.prepare, cross.publish)
		),
	)?;
	let mut adapter = HostAdapter {
		count: 0,
		error: None,
	};
	for stmt in &mut br.op[0] {
		adapter.visit_stmt_mut(stmt);
	}
	require(adapter.count == 1, "host adapter count")?;
	if let Some(e) = adapter.error {
		return Err(e);
	}
	let mut paths = CorePaths { error: None };
	for stmt in &mut br.op[0] {
		paths.visit_stmt_mut(stmt);
	}
	if let Some(e) = paths.error {
		return Err(e);
	}

	equal(
		"opcode/post-call transfer",
		&stmts(&ar.op[0]),
		&stmts(&br.op[0]),
	)?;
	equal(
		"error enrichment",
		&tokens(&ar.enrich[0]),
		&tokens(&br.enrich[0]),
	)?;
	// The terminal wrapper differs; the cleanup expression itself must be exact.
	let expected = parsed(
		"{ let result: core::result::Result<Root, Failure> = CLEANUP; match result { Err(failure) => SliceOutcome::Failed(failure), Ok(_) => unreachable!(), } }",
	);
	let mut cb = br.cleanup[0].clone();
	let first = cb.stmts.first_mut().ok_or("missing terminal cleanup")?;
	let syn::Stmt::Local(local) = first else {
		return Err("terminal cleanup is not bound".into());
	};
	let init = local.init.as_mut().ok_or("missing cleanup initializer")?;
	let syn::Expr::Block(cleanup) = init.expr.as_ref() else {
		return Err("cleanup block missing".into());
	};
	equal(
		"terminal cleanup",
		&tokens(&ar.cleanup[0]),
		&tokens(&cleanup.block),
	)?;
	init.expr = Box::new(syn::parse_str("CLEANUP").unwrap());
	require(tokens(&cb) == expected, "terminal conversion hook changed")?;
	let fail = method(&b, "sliced_failure")?;
	let mut blocks = FindCleanup::default();
	blocks.visit_block(&fail.block);
	require(
		blocks.found.len() == 1 && tokens(&blocks.found[0]) == tokens(&ar.cleanup[0]),
		"cancel/drop cleanup drift",
	)?;
	// Hooks outside opcode body are exact syntax, with unique occurrences.
	let rt = tokens(&resume.block);

	for (name, hook) in [
		(
			"checkpoint",
			"if instruction.charged && soft == 0 { self.commit_pc(frame_index, pc)?; return Ok(None); } let mut transferred = false; let result =",
		),
		("soft debit", "soft -= 1;"),
		(
			"terminal complete",
			"Ok(None) => SliceOutcome::Limited, Ok(Some(value)) => SliceOutcome::Complete(store.export(value)),",
		),
	] {
		let h = parsed(hook);
		require(
			rt.matches(&h).count() == 1,
			&format!("{name} hook changed or duplicated"),
		)?;
	}
	let hard = parsed(
		"fuel = fuel.checked_sub(1).ok_or_else(|| error(ErrorKind::FuelExhausted))?; soft -= 1;",
	);
	require(rt.contains(&hard), "soft debit moved before hard debit")?;
	Ok(())
}
#[derive(Default)]
struct FindCleanup {
	found: Vec<syn::Block>,
}
impl<'a> Visit<'a> for FindCleanup {
	fn visit_block(&mut self, b: &'a syn::Block) {
		if b.stmts
			.first()
			.is_some_and(|s| tokens(s).starts_with("let numeric_context ="))
		{
			self.found.push(b.clone());
		}
		visit::visit_block(self, b)
	}
}
fn mutation_controls(a: &str, b: &str) -> Result<(), String> {
	let message = equal(
		"diagnostic control",
		&parsed("value + 1"),
		&parsed("value - 1"),
	)
	.unwrap_err();
	require(
		message.contains("first differing token 1")
			&& message.contains("complete [value + 1]")
			&& message.contains("sliced [value - 1]"),
		"token difference diagnostic control",
	)?;
	for (from, to) in [
		("pc += 1;", "pc += 2;"),
		(
			"Self::cross_prepare_owner(cross,code.id())?;",
			"Self::cross_prepare_owner(cross,0)?;",
		),
		(
			"Self::cross_publish_owner(cross,code.id());",
			"Self::cross_publish_owner(cross,0);",
		),
		(
			"self.sliced_leave(store,cross)",
			"self.sliced_leave(store,None)",
		),
		(
			"self.frames.clear();",
			"self.frames.clear(); self.frames.clear();",
		),
		("soft -= 1;", "soft -= 2;"),
		(
			"instruction.charged && soft == 0",
			"instruction.charged && soft == 1",
		),
		(
			"Ok(None) => SliceOutcome::Limited",
			"Ok(None) => SliceOutcome::Failed(failure)",
		),
		("&mut fuel,", "&mut soft,"),
	] {
		require(b.contains(from), &format!("mutation anchor absent: {from}"))?;
		require(
			guard(a, &b.replacen(from, to, 1)).is_err(),
			&format!("accepted mutation {from}"),
		)?;
	}
	for (from, to) in [
		("pc += 1;", "pc += 2;"),
		(
			"self.frames.clear();",
			"self.frames.clear(); self.frames.clear();",
		),
	] {
		require(a.contains(from), "fixed mutation missing")?;
		require(
			guard(&a.replacen(from, to, 1), b).is_err(),
			"accepted fixed mutation",
		)?;
	}
	// AST mutations prove extra/reordered dispatch arms are rejected.
	for duplicate in [false, true] {
		let mut file = syn::parse_file(b).map_err(|e| e.to_string())?;
		let mut mutation = ArmMutation {
			duplicate,
			count: 0,
		};
		mutation.visit_file_mut(&mut file);
		require(mutation.count == 1, "dispatch mutation count")?;
		require(
			guard(a, &file.to_token_stream().to_string()).is_err(),
			"accepted extra/reordered arm",
		)?;
	}
	Ok(())
}
struct ArmMutation {
	duplicate: bool,
	count: usize,
}
impl VisitMut for ArmMutation {
	fn visit_expr_match_mut(&mut self, m: &mut syn::ExprMatch) {
		if tokens(&m.expr) == parsed("instruction.op") {
			self.count += 1;
			if self.duplicate {
				m.arms.insert(0, m.arms[0].clone());
			} else {
				m.arms.swap(0, 1);
			}
		}
		visit_mut::visit_expr_match_mut(self, m)
	}
}
// A qualified selector is deliberately finite: never fall back to a name-only
// match or recursively pick an unrelated nested item.
fn inventory_item(source: &str, kind: &str, receiver: &str, name: &str) -> Result<String, String> {
	let file = syn::parse_file(source).map_err(|e| e.to_string())?;
	let mut found = Vec::new();
	for item in &file.items {
		match item {
			syn::Item::Struct(v) if kind == "struct" && v.ident == name => found.push(tokens(v)),
			syn::Item::Fn(v) if kind == "fn" && v.sig.ident == name => found.push(tokens(v)),
			syn::Item::Impl(v) if kind == "method" => {
				let syn::Type::Path(ty) = &*v.self_ty else {
					continue;
				};
				if ty.path.segments.last().is_none_or(|s| s.ident != receiver) {
					continue;
				}
				for member in &v.items {
					if let syn::ImplItem::Fn(f) = member {
						if f.sig.ident == name {
							// Include the impl's generics/constraints and attributes too.
							let mut selected = v.clone();
							selected.items = vec![syn::ImplItem::Fn(f.clone())];
							found.push(tokens(&selected));
						}
					}
				}
			}
			_ => (),
		}
	}
	require(
		found.len() == 1,
		&format!(
			"{kind} {receiver}::{name}: expected one item, found {}",
			found.len()
		),
	)?;
	Ok(found.remove(0))
}
fn main() {
	let args: Vec<_> = std::env::args().collect();
	if args.get(1).is_some_and(|s| s == "--item") {
		assert_eq!(args.len(), 6, "--item FILE KIND RECEIVER NAME");
		let source = std::fs::read_to_string(&args[2]).unwrap();
		match inventory_item(&source, &args[3], &args[4], &args[5]) {
			Ok(value) => println!("{value}"),
			Err(e) => {
				eprintln!("FAIL: {e}");
				std::process::exit(1);
			}
		}
		return;
	}
	let root = std::env::args().nth(1).expect("repository root required");
	let base = std::path::Path::new(&root).join("rune/crates/rune/src/experimental_core");
	let a = std::fs::read_to_string(base.join("execute.rs")).unwrap();
	let b = std::fs::read_to_string(base.join("execute/sliced.rs")).unwrap();
	match guard(&a, &b).and_then(|_| mutation_controls(&a, &b)) {
		Ok(()) => println!("PASS: executor token drift guard and mutation controls"),
		Err(e) => {
			eprintln!("FAIL: {e}");
			std::process::exit(1)
		}
	}
}

#[cfg(test)]
mod inventory_controls {
	use super::*;
	#[test]
	fn qualified_nonvacuous_and_sensitive() {
		let a = "struct Frame { pc: usize } impl Executor { fn run(&self) { one(); } }";
		let baseline = inventory_item(a, "method", "Executor", "run").unwrap();
		let changed = a.replace("one", "two");
		assert_ne!(
			baseline,
			inventory_item(&changed, "method", "Executor", "run").unwrap()
		);
		// Matching changes to a second copy cannot change the baseline fingerprint.
		assert_eq!(
			inventory_item(&changed, "method", "Executor", "run").unwrap(),
			inventory_item(&changed, "method", "Executor", "run").unwrap()
		);
		assert!(inventory_item(a, "method", "Other", "run").is_err());
		assert!(inventory_item(&(a.to_owned() + a), "struct", "", "Frame").is_err());
		assert_ne!(
			inventory_item(a, "struct", "", "Frame").unwrap(),
			inventory_item(&a.replace("usize", "u32"), "struct", "", "Frame").unwrap()
		);
		assert_eq!(
			baseline,
			inventory_item(
				&a.replace("one();", "/* comment */ one ( ) ;"),
				"method",
				"Executor",
				"run"
			)
			.unwrap()
		);
	}
}
