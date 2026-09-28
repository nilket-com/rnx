use crate::emit::callable::materialized_call;
use crate::emit::{Emitted, OracleInfo, doc_line, generics_map, rust_ident, unused_generic};
use crate::families::callbacks::{
	binding_route_reason, callback_arg, closure_signature, routed_binding,
};
use crate::model::Callable;
use crate::text::sanitize;
use crate::ty;
use crate::world::mapping::{Ret, Unsupported};
use crate::world::{HAND_FREE, World, rune_name, spell};
use std::fmt::Write as _;

pub(crate) fn emit_free(world: &World, out: &mut Emitted, c: &Callable) {
	world.tmp.set(0);
	let rust_name = c.name.clone();
	let name = rune_name(&rust_name);
	if c.is_async {
		out.unsupported(c, "async", &name);
		return;
	}
	if HAND_FREE.contains(&name.as_str()) {
		out.adapted(
			c,
			"hand-written binding of the same name",
			&format!("polars::{name}"),
		);
		return;
	}
	if let Some(g) = unused_generic(c) {
		out.unsupported(c, "generic parameter not inferable from arguments", &g);
		return;
	}
	if let Some(r) = world
		.release
		.refused
		.iter()
		.find(|r| r.path == c.canonical_path)
	{
		out.unsupported(c, "release policy", &format!("{} ({})", r.reason, r.cite));
		return;
	}
	let Some(spelled) = spell(
		&c.found_paths,
		&c.crate_paths,
		&name,
		&world.ambiguous_prelude,
	) else {
		out.unsupported(c, "no public path", c.canonical_path.clone().as_str());
		return;
	};
	let key = ("polars".to_string(), name.clone());
	if let Some(prev) = out.taken.get(&key) {
		out.unsupported(c, "name taken in polars:: by", prev.clone().as_str());
		return;
	}
	let generics = generics_map(c);
	let mut params = Vec::new();
	for p in &c.params {
		let mapped = match closure_signature(c, p) {
			Some(sig) => callback_arg(world, c, &sig, &sanitize(&p.name), None),
			None => world.arg(
				&ty::parse(&p.ty_canonical),
				&sanitize(&p.name),
				&generics,
				None,
				0,
			),
		};
		match mapped {
			Ok(a) => params.push((sanitize(&p.name), a)),
			Err(Unsupported(why, what)) => {
				out.unsupported(c, why, &format!("{} ({what})", p.name));
				return;
			}
		}
	}
	let ret = match c.ret_canonical.as_deref() {
		None => Ret {
			materialize: None,
			rust_ty: "()".into(),
			fallible: false,
			conv: "__r".into(),
			doc: "unit".into(),
		},
		Some(rc) => match world.ret(&ty::parse(rc), None, 0) {
			Ok(r) => r,
			Err(Unsupported(why, what)) => {
				out.unsupported(c, why, &format!("return ({what})"));
				return;
			}
		},
	};
	// Rune implements `Function` for free functions of at most five
	// parameters (rune 0.14.2 `function/macros.rs`, every reference
	// permutation); methods share it (record 0108, METHOD_ARITY).
	const FREE_ARITY: usize = 5;
	if params.len() > FREE_ARITY {
		out.unsupported(
			c,
			"arity",
			&format!(
				"free function with {} parameters; Rune binds at most {FREE_ARITY}",
				params.len()
			),
		);
		return;
	}
	let route = routed_binding(world, c, None);
	let fallible = ret.fallible
		|| params
			.iter()
			.any(|(_, a)| a.fallible || a.pre.iter().any(|p| p.contains('?')));
	let pre: String = params
		.iter()
		.filter(|(_, a)| a.shape.starts_with("callback:"))
		.chain(
			params
				.iter()
				.filter(|(_, a)| !a.shape.starts_with("callback:")),
		)
		.flat_map(|(_, a)| a.pre.iter())
		.map(|p| format!("{p} "))
		.collect();
	let idx = out.fn_index;
	out.fn_index += 1;
	let ident = rust_ident("g", &c.canonical_path, idx);
	let sig: Vec<String> = params
		.iter()
		.map(|(n, a)| format!("{n}: {}", a.rust_ty))
		.collect();
	let args: Vec<String> = params.iter().map(|(_, a)| a.conv.clone()).collect();
	let ret_ty = if fallible {
		format!("Result<{}, Error>", ret.rust_ty)
	} else {
		ret.rust_ty.clone()
	};
	let body_conv = if fallible {
		format!("Ok({})", ret.conv)
	} else {
		ret.conv.clone()
	}
	.replace("__OP__", &name);
	let doc = doc_line(c);
	let arg_docs: Vec<String> = params
		.iter()
		.map(|(n, a)| format!("{n}: {}", a.doc))
		.collect();
	let summary = format!(
		"{name}({}) -> {}{}",
		arg_docs.join(", "),
		ret.doc,
		if fallible { " (fallible)" } else { "" }
	);
	let (pre, call) = if let Some(m) = &ret.materialize {
		let mut pre = pre.clone();
		let mut hoisted = Vec::new();
		for (i, a) in args.iter().enumerate() {
			pre.push_str(&format!("let __arg{i} = {a}; "));
			hoisted.push(format!("__arg{i}"));
		}
		(
			pre,
			materialized_call(
				m,
				&format!("{spelled}({})", hoisted.join(", ")),
				&name,
				route,
			),
		)
	} else if route {
		let mut pre = pre.clone();
		let mut hoisted = Vec::new();
		for (i, a) in args.iter().enumerate() {
			pre.push_str(&format!("let __arg{i} = {a}; "));
			hoisted.push(format!("__arg{i}"));
		}
		let call = format!(
			"crate::engine::run(\"polars::{name}\", move || {spelled}({}))",
			hoisted.join(", ")
		);
		(
			pre,
			if fallible {
				format!("{call}.map_err(Error::engine)?")
			} else {
				format!("crate::engine::infallible({call}, \"polars::{name}\")")
			},
		)
	} else {
		(pre, format!("{spelled}({})", args.join(", ")))
	};
	let docline = if doc.is_empty() {
		String::new()
	} else {
		format!("/// {doc}\n")
	};
	writeln!(out.functions, "{docline}/// Polars: `{}`. {}\n#[rune::function(path = {name})]\nfn {ident}({}) -> {ret_ty} {{ {pre}let __r = {call}; {body_conv} }}", c.canonical_path, summary, sig.join(", ")).unwrap();
	out.registrations
		.push(format!("m.function_meta({ident})?;"));
	let rune = format!("polars::{name}");
	out.catalogue.push((
		rune.clone(),
		if doc.is_empty() {
			summary.clone()
		} else {
			format!("{summary}: {doc}")
		},
	));
	out.taken.insert(key, c.canonical_path.clone());
	let mut notes = Vec::new();
	if route {
		notes.push("routed through the engine thread".to_string());
	}
	if name != rust_name {
		notes.push(format!("renamed: `{rust_name}` is a Rune keyword"));
	}
	let info = OracleInfo {
		rune_owner: None,
		rune_name: name.clone(),
		receiver: "none".into(),
		owner: None,
		callee: spelled.clone(),
		params: c
			.params
			.iter()
			.zip(params.iter())
			.map(|(p, (_, a))| (a.shape.clone(), p.ty_canonical.clone()))
			.collect(),
		param_names: c.params.iter().map(|p| sanitize(&p.name)).collect(),
		ret_canonical: c.ret_canonical.clone(),
		ret_rust: ret.rust_ty.clone(),
		fallible,
		generics: generics.clone(),
		implementors: vec![],
		deref: false,
	};
	out.generated_with(
		c,
		&rune,
		if notes.is_empty() {
			None
		} else {
			Some(notes.join("; "))
		},
		info,
	);
	if let Some(binding) = out.entries.last_mut().unwrap().bindings.first_mut() {
		binding.route_reason = binding_route_reason(world, c, route);
		if route {
			binding.reentry = Some(if fallible { "error" } else { "unwind" }.into());
		}
	}
}
