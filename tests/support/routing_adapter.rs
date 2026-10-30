//! Entry adaptation changes neither original bindings nor its returned value.
//! Original text is retained inside a fresh module; recursion still names the
//! original function. Absolute crate paths are redirected into that module.
use rune::{
	SourceId, ast,
	ast::Spanned,
	parse::{Parser, parse_all},
};
use std::collections::BTreeSet;

pub struct Adapted {
	pub source: String,
	pub callable: bool,
	pub reason: &'static str,
}
fn function<'a>(file: &'a ast::File, path: &[&str], text: &str) -> Option<&'a ast::ItemFn> {
	for (item, _) in &file.items {
		match item {
			ast::Item::Fn(function)
				if path.len() == 1 && &text[function.name.span().range()] == path[0] =>
			{
				return Some(function);
			}
			ast::Item::Mod(module)
				if path.len() > 1 && &text[module.name.span().range()] == path[0] =>
			{
				if let ast::ItemModBody::InlineBody(body) = &module.body {
					if let Some(function) = function(&body.file, &path[1..], text) {
						return Some(function);
					}
				}
			}
			_ => {}
		}
	}
	None
}
pub fn adapt(text: &str, mode: &str, entry: &str) -> Adapted {
	let mut parser = Parser::new(text, SourceId::EMPTY, true);
	let mut tokens = Vec::new();
	while let Ok(token) = parser.parse::<ast::Token>() {
		tokens.push(token);
	}
	let identifiers: BTreeSet<_> = tokens
		.iter()
		.filter(|token| matches!(token.kind, ast::Kind::Ident(..)))
		.map(|token| &text[token.span.range()])
		.collect();
	let mut nonce = 0usize;
	let (module, dispatch, args) = loop {
		let module = format!("__rnx_route_module_{nonce}");
		let dispatch = format!("__rnx_route_entry_{nonce}");
		let args = format!("__rnx_route_args_{nonce}");
		if ![&module, &dispatch, &args]
			.iter()
			.any(|name| identifiers.contains(name.as_str()))
		{
			break (module, dispatch, args);
		}
		nonce += 1;
	};
	let shebang_end = if text.starts_with("#!") && !text.starts_with("#![") {
		text.find('\n').map_or(text.len(), |n| n + 1)
	} else {
		0
	};
	if mode == "script" {
		let (shebang, body) = text.split_at(shebang_end);
		return Adapted {
			source: format!("{shebang}pub fn main({args}){{\n{body}\n}}"),
			callable: true,
			reason: "script-body",
		};
	}
	let Ok(file) = parse_all::<ast::File>(text, SourceId::EMPTY, true) else {
		return Adapted {
			source: text.to_owned(),
			callable: false,
			reason: "parse-control",
		};
	};
	let path: Vec<_> = entry.split("::").collect();
	let Some(function) = function(&file, &path, text) else {
		return Adapted {
			source: text.to_owned(),
			callable: false,
			reason: "no-selected-entry",
		};
	};
	let argc = function.args.len();
	if argc > 1 {
		return Adapted {
			source: text.to_owned(),
			callable: false,
			reason: "entry-needs-non-cli-arguments",
		};
	}
	if entry == "main" && argc == 1 {
		return Adapted {
			source: text.to_owned(),
			callable: true,
			reason: "original-cli-entry",
		};
	}
	if tokens.windows(3).any(|tokens| {
		tokens[0].kind == ast::Kind::Mod
			&& matches!(tokens[1].kind, ast::Kind::Ident(..))
			&& tokens[2].kind == ast::Kind::SemiColon
	}) {
		return Adapted {
			source: text.to_owned(),
			callable: false,
			reason: "external-loader-entry-needs-argument-adapter",
		};
	}
	let is_async = function.async_token.is_some();
	let arguments = if argc == 0 {
		String::new()
	} else {
		args.clone()
	};
	let (async_, await_) = if is_async {
		("async ", ".await")
	} else {
		("", "")
	};
	// Insert a fresh bridge in each enclosing scope. A root dispatcher must
	// not turn a private nested test entry into an apparent compile control.
	// Original visibility, names, recursion and local shadowing stay intact.
	let mut edits = Vec::new();
	let mut scope = &file;
	for depth in 0..path.len() - 1 {
		let body = scope
			.items
			.iter()
			.find_map(|(item, _)| {
				let ast::Item::Mod(item) = item else {
					return None;
				};
				if &text[item.name.span().range()] != path[depth] {
					return None;
				}
				let ast::ItemModBody::InlineBody(body) = &item.body else {
					return None;
				};
				Some(body)
			})
			.expect("selected entry has inline enclosing scopes");
		let call = if depth + 2 == path.len() {
			format!("{}({arguments}){await_}", path[depth + 1])
		} else {
			format!("{}::{dispatch}({args}){await_}", path[depth + 1])
		};
		edits.push((
			body.close.span().range().start,
			format!("\npub {async_}fn {dispatch}({args}){{{call}}}\n"),
		));
		scope = &body.file;
	}
	for pair in tokens.windows(2) {
		if pair[0].kind == ast::Kind::Crate && pair[1].kind == ast::Kind::ColonColon {
			edits.push((pair[1].span.range().end, format!("{module}::")));
		}
	}
	edits.sort_by_key(|(position, _)| std::cmp::Reverse(*position));
	let mut redirected = text.to_owned();
	for (position, insertion) in edits {
		redirected.insert_str(position, &insertion);
	}
	let (shebang, redirected) = redirected.split_at(shebang_end);
	let call = if path.len() == 1 {
		format!("{entry}({arguments}){await_}")
	} else {
		format!("{}::{dispatch}({args}){await_}", path[0])
	};
	Adapted {
		source: format!(
			"{shebang}mod {module}{{\n{redirected}\npub {async_}fn {dispatch}({args}){{{call}}}\n}}\npub {async_}fn main({args}){{{module}::{dispatch}({args}){await_}}}\n"
		),
		callable: true,
		reason: "hygienic-entry",
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn adaptation_preserves_recursion_shadowing_crate_paths_and_return_values() {
		for (source, entry) in [
			(
				"fn __rnx_route_module_0(){0}fn recurse(n){if n==0{3}else{recurse(n-1)}}fn main(){let main=||4;crate::recurse(2)+main()}",
				"main",
			),
			(
				"fn __rnx_route_module_0(){0}fn value(){7}mod private{mod nested{#[test]fn target(){crate::value()}}}",
				"private::nested::target",
			),
		] {
			let adapted = adapt(source, "test", entry);
			assert!(adapted.callable);
			assert!(adapted.source.contains("mod __rnx_route_module_1"));
			let context = rune::Context::with_default_modules().unwrap();
			let mut sources = rune::Sources::new();
			sources
				.insert(rune::Source::memory(&adapted.source).unwrap())
				.unwrap();
			let mut vm = rune::prepare(&mut sources)
				.with_context(&context)
				.build_vm()
				.unwrap();
			assert_eq!(
				vm.call(["main"], (rune::to_value(Vec::<String>::new()).unwrap(),))
					.unwrap()
					.as_integer::<i64>()
					.unwrap(),
				7
			);
		}
	}
}
