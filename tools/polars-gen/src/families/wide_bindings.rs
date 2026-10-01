//! Record 0127: a listed method whose arity exceeds Rune's typed limit.
//! Rune 0.14.2 implements its typed `Function` (and so `InstanceFunction`)
//! only up to arity 5 (`function/macros.rs`, `permute!`). A listed method is
//! generated as the same Rust function as always, without the
//! `#[rune::function]` attribute, and registered through a raw shim
//! (`Module::raw_function(..).build_associated::<Receiver>()`, an instance
//! function). The shim takes the exact argument slice and converts each slot
//! as Rune's typed path converts that parameter type, calls the function,
//! and stores the result with `ToValue`, as the typed path does.
use crate::model::Callable;

/// Rune's typed arity limit, receiver included.
pub(crate) const TYPED_ARITY: usize = 5;
/// The most arguments a raw shim takes here (Rune's own instance limit).
pub(crate) const RAW_ARITY: usize = 16;

#[derive(serde::Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct WideBinding {
	/// The callable's canonical path.
	pub(crate) path: String,
	/// Its arity, receiver included, as the binding sees it.
	pub(crate) arity: usize,
	pub(crate) cite: String,
}

/// The table, fail closed: each row cited and unique; its callable present
/// exactly once, a method (a receiver), with exactly the listed arity, above
/// the typed limit and within the raw one.
pub(crate) fn validate(rows: &[WideBinding], callables: &[Callable]) -> Result<(), String> {
	let mut seen = std::collections::BTreeSet::new();
	for r in rows {
		if r.cite.trim().is_empty() {
			return Err(format!("{}: no citation", r.path));
		}
		if !seen.insert(r.path.clone()) {
			return Err(format!("{}: listed twice", r.path));
		}
		let found: Vec<&Callable> = callables
			.iter()
			.filter(|c| c.canonical_path == r.path)
			.collect();
		if found.len() != 1 {
			return Err(format!(
				"{}: {} callables with this path, want exactly one",
				r.path,
				found.len()
			));
		}
		let c = found[0];
		if c.receiver == "none" {
			return Err(format!(
				"{}: not a method; this record binds methods only",
				r.path
			));
		}
		let arity = 1 + c.params.len();
		if arity != r.arity {
			return Err(format!("{}: arity {arity}, listed {}", r.path, r.arity));
		}
		if arity <= TYPED_ARITY || arity > RAW_ARITY {
			return Err(format!(
				"{}: arity {arity} is not above {TYPED_ARITY} and at most {RAW_ARITY}",
				r.path
			));
		}
	}
	Ok(())
}

/// Every listed row was generated: an unused row is refused.
pub(crate) fn check_used(rows: &[WideBinding], generated: &[&str]) -> Result<(), String> {
	for r in rows {
		if !generated.contains(&r.path.as_str()) {
			return Err(format!(
				"{}: listed, but the callable was not generated",
				r.path
			));
		}
	}
	Ok(())
}

pub(crate) fn listed<'a>(rows: &'a [WideBinding], path: &str) -> Option<&'a WideBinding> {
	rows.iter().find(|r| r.path == path)
}

/// One slot's conversion, as Rune's typed path converts that parameter type
/// (on the value taken from the slot): the statement binding its guard or
/// value, and the expression passed to the function. `None` for a type with
/// no conversion here (the callable is then refused by name).
pub(crate) fn slot(i: usize, rust_ty: &str) -> Option<(String, String)> {
	let v = format!("__v{i}");
	let g = format!("__g{i}");
	let ty = rust_ty.trim();
	if ty == "&str" {
		return Some((
			format!("let {g} = rune::vm_try!({v}.borrow_string_ref());"),
			format!("&*{g}"),
		));
	}
	if let Some(inner) = ty.strip_prefix("&mut ") {
		return Some((
			format!("let mut {g} = rune::vm_try!({v}.borrow_mut::<{inner}>());"),
			format!("&mut *{g}"),
		));
	}
	if let Some(inner) = ty.strip_prefix('&') {
		if inner.starts_with('[') || inner.starts_with("dyn ") {
			return None;
		}
		return Some((
			format!("let {g} = rune::vm_try!({v}.borrow_ref::<{inner}>());"),
			format!("&*{g}"),
		));
	}
	if ty == "rune::Value" {
		return Some((String::new(), v));
	}
	Some((
		format!("let {g}: {ty} = rune::vm_try!(rune::from_value({v}));"),
		g,
	))
}

/// The raw shim for a generated function `ident` with these parameter types
/// (receiver first), following rune 0.14.2's typed `fn_call`
/// (`function/mod.rs` `access_memory!`, `impl_function_traits!`): the count
/// first; every slot taken (replaced with `Value::empty()`); each converted
/// in order; the guards alive through the call and dropped before
/// `ToReturn::to_return`; then the store. (The typed path also tags a
/// conversion failure with `BadArgument { arg }`, through a `pub(crate)`
/// API; the shim reports the conversion's own error.)
pub(crate) fn shim(ident: &str, types: &[String]) -> Option<String> {
	shim_named(&format!("s_{ident}"), ident, types)
}

/// The shim under its own name, calling `callee`.
pub(crate) fn shim_named(name: &str, callee: &str, types: &[String]) -> Option<String> {
	let n = types.len();
	let mut convs = String::new();
	let mut pass = Vec::new();
	for (i, t) in types.iter().enumerate() {
		let (stmt, expr) = slot(i, t)?;
		convs.push_str(&stmt);
		convs.push(' ');
		pass.push(expr);
	}
	let slots: Vec<String> = (0..n).map(|i| format!("__s{i}")).collect();
	let takes: String = (0..n)
		.map(|i| format!("let __v{i} = std::mem::replace(__s{i}, rune::Value::empty()); "))
		.collect();
	Some(format!(
		"fn {name}(stack: &mut dyn rune::runtime::Memory, addr: rune::runtime::InstAddress, len: usize, out: rune::runtime::Output) -> rune::runtime::VmResult<()> {{ \
		 if len != {n} {{ return rune::runtime::VmResult::err(rune::runtime::RuntimeError::bad_argument_count(len, {n})); }} \
		 let [{slots}] = rune::vm_try!(stack.slice_at_mut(addr, len)) else {{ unreachable!() }}; \
		 {takes}\
		 let __r = {{ {convs}{callee}({pass}) }}; \
		 let __value = rune::vm_try!(rune::runtime::ToReturn::to_return(__r)); \
		 rune::vm_try!(out.store(stack, __value)); rune::runtime::VmResult::Ok(()) }}",
		slots = slots.join(", "),
		pass = pass.join(", ")
	))
}

/// Record 0127 (review of the plan): the typed-versus-raw control. Two
/// hand-written functions (`support::wide_control`, `Result`, and
/// `support::wide_control_vm`, `VmResult`) are registered typed by the
/// adapter and through shims this same emitter produces, under
/// `test-support`, so one script compares both conventions directly.
pub(crate) fn control() -> (String, Vec<String>) {
	let types: Vec<String> = ["&DataFrame", "&str", "i64", "DataFrame"]
		.iter()
		.map(|s| s.to_string())
		.collect();
	let mut text =
		String::from("\n// record 0127: the typed-versus-raw control (test-support only)\n");
	let mut regs = Vec::new();
	for (f, rune) in [
		("wide_control", "wide_control_raw"),
		("wide_control_vm", "wide_control_vm_raw"),
	] {
		let name = format!("s_{f}");
		text.push_str("#[cfg(feature = \"test-support\")]\n");
		text.push_str(
			&shim_named(&name, &format!("support::{f}"), &types).expect("control types convert"),
		);
		text.push('\n');
		regs.push(format!("#[cfg(feature = \"test-support\")] m.raw_function(\"{rune}\", {name}).build_associated::<DataFrame>()?;"));
	}
	(text, regs)
}

/// Record 0127: the table refuses what it must, and the shim follows the
/// typed convention's order (count, take, convert, call, `ToReturn`, store).
pub(crate) fn wide_bindings_self_test() {
	let callable = |path: &str, receiver: &str, n: usize| -> Callable {
		let params: Vec<serde_json::Value> = (0..n)
			.map(
				|i| serde_json::json!({"name": format!("p{i}"), "ty": "i64", "ty_canonical": "i64"}),
			)
			.collect();
		serde_json::from_value(serde_json::json!({
			"key": path, "kind": "inherent", "krate": "polars_lazy", "owner": "polars_lazy::frame::LazyFrame", "name": "m",
			"canonical_path": path, "found_paths": [], "crate_paths": [], "receiver": receiver,
			"params": params, "ret": null, "ret_canonical": null, "generics_canonical": [], "impl_for": null,
			"impl_bounds": [], "impl_head": null, "impl_where": [], "impl_assoc": [], "docs_first": null,
			"owner_generic": false, "is_unsafe": false, "is_async": false, "deprecated": false,
			"hidden": false, "implementors": [], "trait_reachable": false, "derived": false,
			"bucket": "mechanical", "rules": []
		}))
		.unwrap()
	};
	let calls = vec![
		callable("x::wide", "self", 8),
		callable("x::narrow", "&self", 3),
		callable("x::free", "none", 8),
		callable("x::huge", "self", 20),
	];
	let row = |path: &str, arity: usize, cite: &str| WideBinding {
		path: path.into(),
		arity,
		cite: cite.into(),
	};
	assert!(validate(&[row("x::wide", 9, "c")], &calls).is_ok());
	let refused = |r: WideBinding, want: &str| {
		let e = validate(&[r], &calls).unwrap_err();
		assert!(e.contains(want), "{e} (wanted {want})");
	};
	refused(row("x::wide", 9, " "), "no citation");
	refused(row("x::narrow", 4, "c"), "not above 5");
	refused(row("x::free", 9, "c"), "not a method");
	refused(row("x::huge", 21, "c"), "not above 5 and at most 16");
	refused(row("x::wide", 8, "c"), "arity 9, listed 8");
	refused(row("x::nowhere", 9, "c"), "0 callables");
	let e = validate(&[row("x::wide", 9, "c"), row("x::wide", 9, "c")], &calls).unwrap_err();
	assert!(e.contains("listed twice"), "{e}");
	assert!(
		check_used(&[row("x::wide", 9, "c")], &[])
			.unwrap_err()
			.contains("not generated")
	);
	// the slot conversions, by parameter type
	assert!(slot(0, "&W").unwrap().0.contains("borrow_ref::<W>()"));
	assert!(slot(1, "&mut W").unwrap().0.contains("borrow_mut::<W>()"));
	assert!(slot(2, "&str").unwrap().0.contains("borrow_string_ref()"));
	assert_eq!(
		slot(3, "rune::Value").unwrap(),
		(String::new(), "__v3".into())
	);
	assert!(slot(4, "i64").unwrap().0.contains("rune::from_value(__v4)"));
	assert!(slot(5, "&[u8]").is_none());
	// the shim's order: count, take, convert, call, ToReturn, store
	let s = shim("f", &["&W".into(), "i64".into()]).unwrap();
	let at = |needle: &str| {
		s.find(needle)
			.unwrap_or_else(|| panic!("{needle} missing in {s}"))
	};
	let order = [
		at("bad_argument_count(len, 2)"),
		at("slice_at_mut"),
		at("std::mem::replace(__s0, rune::Value::empty())"),
		at("borrow_ref::<W>()"),
		at("rune::from_value(__v1)"),
		at("f(&*__g0, __g1)"),
		at("ToReturn::to_return(__r)"),
		at("out.store(stack, __value)"),
	];
	assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?} in {s}");
	println!("wide bindings self-test: ok");
}

#[cfg(test)]
mod tests {
	#[test]
	fn wide_bindings() {
		super::wide_bindings_self_test();
	}
}
