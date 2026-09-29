use crate::emit::{
	Binding, Emitted, Entry, OracleInfo, binding_id, rune_path, rust_ident, signature_of,
};
use crate::families::protocols::emit_foreign;
use crate::families::serde::serde_trait;
use crate::families::{Claims, Family};
use crate::model::{Callable, Inventory, Param, Supporting};
use crate::release::{FamilyTables, InstantiationScope, Release, ReleaseProvenance};
use crate::ty;
use crate::ty::{Ty, last};
use crate::world::mapping::Unsupported;
use crate::world::proof::{Applicability, subst_params};
use crate::world::{HAND_PROTOCOLS, World, Wrapper};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

/// Record 0113: the generic-impl families. For operators, FromIterator and
/// Index, one well-formed row binds and each row with one field changed is
/// refused by name with no binding text and no registration.
// ---------------------------------------------------------------- record 0115: this module's claims

/// Record 0113: `Index`, grouped into one `INDEX_GET` per owner (decided in `finish`).
pub(crate) struct IndexClaim;
pub(crate) static INDEX: IndexClaim = IndexClaim;

impl Family for IndexClaim {
	fn name(&self) -> &'static str {
		"index"
	}
	fn claim<'a>(&self, cx: &mut Claims<'a, '_>, c: &'a Callable) -> bool {
		if !index_row(c) {
			return false;
		}
		match index_arm(cx.world, c) {
			Ok((o, k, x)) => {
				cx.index_rows.push(c);
				cx.index_pending.push((c.key.clone(), o, k, x));
			}
			Err(why) => cx.out.unsupported(c, "index", &why),
		}
		true
	}
}

/// Record 0113: `FromIterator` constructors, by the admitted item grammar.
pub(crate) struct FromIterClaim;
pub(crate) static FROM_ITER: FromIterClaim = FromIterClaim;

impl Family for FromIterClaim {
	fn name(&self) -> &'static str {
		"from_iter"
	}
	fn claim<'a>(&self, cx: &mut Claims<'a, '_>, c: &'a Callable) -> bool {
		if !from_iter_row(c) {
			return false;
		}
		emit_from_iter(cx.world, cx.out, c);
		true
	}
}

/// Record 0113: a unary `Neg`/`Not` in the generic bucket with the exact
/// concrete shape goes to the existing unary protocol route.
pub(crate) struct UnaryClaim;
pub(crate) static UNARY: UnaryClaim = UnaryClaim;

impl Family for UnaryClaim {
	fn name(&self) -> &'static str {
		"unary"
	}
	fn claim<'a>(&self, cx: &mut Claims<'a, '_>, c: &'a Callable) -> bool {
		if !unary_generic_shape(c) {
			return false;
		}
		emit_foreign(cx.world, cx.out, c);
		true
	}
}

/// Record 0113: generic-bucket operators are decided as a family (in `finish`).
pub(crate) struct OperatorClaim;
pub(crate) static OPERATOR: OperatorClaim = OperatorClaim;

impl Family for OperatorClaim {
	fn name(&self) -> &'static str {
		"operator"
	}
	fn claim<'a>(&self, cx: &mut Claims<'a, '_>, c: &'a Callable) -> bool {
		if generic_op(c).is_none() {
			return false;
		}
		match op_arms(cx.world, c) {
			Ok(arms) => {
				cx.op_rows.push(c);
				cx.op_pending.extend(arms);
			}
			Err(why) => cx.out.unsupported(c, "operator", &why),
		}
		true
	}
}

/// Record 0113: every other generic-bucket trait impl gets its family's
/// named contract (serde keeps its own lane inside `emit_callable`).
pub(crate) struct FamilyRefusal;
pub(crate) static FAMILY_REFUSAL: FamilyRefusal = FamilyRefusal;

impl Family for FamilyRefusal {
	fn name(&self) -> &'static str {
		"generic_impl_refusal"
	}
	fn claim<'a>(&self, cx: &mut Claims<'a, '_>, c: &'a Callable) -> bool {
		if serde_trait(c).is_some() {
			return false;
		}
		match generic_impl_refusal(c) {
			Some(why) => {
				cx.out.unsupported(c, "0113 family", &why);
				true
			}
			None => false,
		}
	}
}

/// Record 0113 (moved verbatim from the pipeline): the operator and
/// `INDEX_GET` groups, decided after every callable has been claimed.
pub(crate) fn finish(cx: Claims<'_, '_>) {
	let Claims {
		world,
		mut out,
		op_rows,
		op_pending,
		index_rows,
		index_pending,
		..
	} = cx;
	let op_used = emit_op_groups(&world, &mut out, op_pending);
	let index_used = emit_index_groups(&world, &mut out, &index_pending);
	for c in index_rows {
		let (_, owner, key, output) = index_pending.iter().find(|a| a.0 == c.key).unwrap().clone();
		match index_used.get(&c.key) {
			Some(Ok(())) => {
				let w = &world.wrappers[&owner];
				let info = OracleInfo {
					rune_owner: Some(rune_path(w)),
					rune_name: "[]".into(),
					receiver: "protocol".into(),
					owner: Some((owner.clone(), w.rust.clone())),
					callee: "IndexGen".into(),
					params: vec![(key.to_string(), String::new())],
					param_names: vec![],
					ret_canonical: Some(output.clone()),
					ret_rust: world.wrappers[&output].rust.clone(),
					fallible: true,
					generics: BTreeMap::new(),
					implementors: vec![],
					deref: false,
				};
				out.generated_with(
					c,
					&format!("{}[{key}]", rune_path(w)),
					Some("record 0113: INDEX_GET, the Column cloned out".into()),
					info,
				);
				let b = &mut out.entries.last_mut().unwrap().bindings[0];
				b.id = binding_id(
					&c.canonical_path,
					Some(&format!("{owner}|{}", c.key)),
					false,
				);
			}
			Some(Err(why)) => out.unsupported(c, "index", why),
			None => out.unsupported(c, "index", "no arm"),
		}
	}
	for c in op_rows {
		match op_used.get(&c.key) {
			Some(Ok(arms)) if !arms.is_empty() => op_entry(&world, &mut out, c, arms),
			Some(Err(why)) => out.unsupported(c, "operator", why),
			_ => out.unsupported(c, "operator", "no arm chosen"),
		}
	}
}

pub(crate) fn generic_impls_self_test() {
	let series = "polars_core::series::Series";
	let frame = "polars_core::frame::dataframe::DataFrame";
	let column = "polars_core::frame::column::Column";
	let sup = |path: &str| Supporting {
		key: path.to_string(),
		kind: "struct".into(),
		canonical_path: path.to_string(),
		found_paths: vec![format!("polars::{}", path.rsplit("::").next().unwrap())],
		crate_paths: vec![path.to_string()],
		public_fields: 0,
		fields_canonical: vec![],
		variant_shapes: vec![],
		variant_payloads: vec![],
		generic: false,
		lifetime: false,
		hidden: false,
		derived: vec!["Clone".into()],
		alias_target: None,
		implementors: vec![],
		impls: vec![],
	};
	#[allow(clippy::too_many_arguments)]
	let row = |key: &str,
	        owner: &str,
	        trait_path: &str,
	        name: &str,
	        receiver: &str,
	        params: Vec<&str>,
	        ret: &str,
	        impl_for: &str,
	        assoc: Vec<(&str, &str)>,
	        bounds: Vec<(&str, &str)>| Callable {
		key: key.into(),
		kind: "foreign_trait_impl".into(),
		krate: "polars_core".into(),
		owner: owner.into(),
		name: name.into(),
		canonical_path: format!("{owner} as {trait_path}"),
		found_paths: vec![],
		crate_paths: vec![],
		receiver: receiver.into(),
		params: params
			.into_iter()
			.enumerate()
			.map(|(i, t)| Param {
				name: format!("a{i}"),
				ty: t.into(),
				ty_canonical: t.into(),
			})
			.collect(),
		ret: None,
		ret_canonical: Some(ret.into()),
		generics_canonical: vec![],
		impl_for: Some(impl_for.into()),
		impl_bounds: bounds
			.into_iter()
			.map(|(a, b)| (a.to_string(), b.to_string()))
			.collect(),
		impl_head: None,
		impl_where: vec![],
		impl_assoc: assoc
			.into_iter()
			.map(|(a, b)| (a.to_string(), b.to_string()))
			.collect(),
		docs_first: None,
		owner_generic: false,
		is_unsafe: false,
		is_async: false,
		deprecated: false,
		hidden: false,
		implementors: vec![],
		trait_reachable: true,
		derived: false,
		bucket: "generic".into(),
		rules: vec![],
		trait_lifetimes: vec![],
	};
	let add = "core::ops::arith::Add";
	let res = "core::result::Result<polars_core::series::Series, polars_error::PolarsError>";
	let rs = format!("&{series}");
	let calls = vec![
		// review of 0113: an owned-head twin of op_ok, fed first
		row(
			"op_owned_self",
			series,
			add,
			"Add",
			"self",
			vec!["Self"],
			"Self::Output",
			series,
			vec![("Output", res)],
			vec![],
		),
		row(
			"op_ok",
			series,
			add,
			"Add",
			"self",
			vec!["Self"],
			"Self::Output",
			&rs,
			vec![("Output", res)],
			vec![],
		),
		row(
			"op_scalar",
			series,
			add,
			"Add<T>",
			"self",
			vec!["T"],
			"Self::Output",
			&rs,
			vec![("Output", series)],
			vec![("T", "num_traits::Num + num_traits::cast::NumCast")],
		),
		row(
			"op_bad_recv",
			series,
			add,
			"Add",
			"&self",
			vec!["Self"],
			"Self::Output",
			&rs,
			vec![("Output", res)],
			vec![],
		),
		row(
			"op_bad_assoc",
			series,
			add,
			"Add",
			"self",
			vec!["Self"],
			"Self::Output",
			&rs,
			vec![("Target", res)],
			vec![],
		),
		row(
			"op_bad_param",
			series,
			add,
			"Add",
			"self",
			vec!["u8"],
			"Self::Output",
			&rs,
			vec![("Output", res)],
			vec![],
		),
		row(
			"op_bad_bound",
			series,
			add,
			"Add<T>",
			"self",
			vec!["T"],
			"Self::Output",
			&rs,
			vec![("Output", series)],
			vec![("T", "core::marker::Copy")],
		),
		row(
			"op_outward",
			series,
			add,
			"Add<&Series>",
			"self",
			vec!["&polars_core::series::Series"],
			"Self::Output",
			&format!("&{frame}"),
			vec![("Output", frame)],
			vec![],
		),
		row(
			"op_bad_out",
			series,
			add,
			"Add",
			"self",
			vec!["Self"],
			"Self::Output",
			&rs,
			vec![("Output", "alloc::string::String")],
			vec![],
		),
		row(
			"fi_ok",
			series,
			"core::iter::traits::collect::FromIterator",
			"FromIterator<u8>",
			"none",
			vec!["I"],
			"Self",
			series,
			vec![],
			vec![],
		),
		row(
			"fi_bad_item",
			series,
			"core::iter::traits::collect::FromIterator",
			"FromIterator<Foo>",
			"none",
			vec!["I"],
			"Self",
			series,
			vec![],
			vec![],
		),
		row(
			"fi_bad_recv",
			series,
			"core::iter::traits::collect::FromIterator",
			"FromIterator<u16>",
			"&self",
			vec!["I"],
			"Self",
			series,
			vec![],
			vec![],
		),
		row(
			"fi_bad_ret",
			series,
			"core::iter::traits::collect::FromIterator",
			"FromIterator<u32>",
			"none",
			vec!["I"],
			"u32",
			series,
			vec![],
			vec![],
		),
		row(
			"ix_ok",
			frame,
			"core::ops::index::Index",
			"Index<usize>",
			"&self",
			vec!["usize"],
			"&Self::Output",
			frame,
			vec![("Output", column)],
			vec![],
		),
		row(
			"ix_range",
			frame,
			"core::ops::index::Index",
			"Index<Range<usize>>",
			"&self",
			vec!["core::ops::range::Range<usize>"],
			"&Self::Output",
			frame,
			vec![("Output", "[polars_core::frame::column::Column]")],
			vec![],
		),
		row(
			"ix_bad_key",
			frame,
			"core::ops::index::Index",
			"Index<usize>",
			"&self",
			vec!["i64"],
			"&Self::Output",
			frame,
			vec![("Output", column)],
			vec![],
		),
		row(
			"ix_bad_ret",
			frame,
			"core::ops::index::Index",
			"Index<&str>",
			"&self",
			vec!["&str"],
			"Self::Output",
			frame,
			vec![("Output", column)],
			vec![],
		),
	];
	let inv = Inventory {
		callables: calls.clone(),
		supporting: vec![sup(series), sup(frame), sup(column)],
		provenance: None,
	};
	let release = Release {
		name: "t".into(),
		source: "t".into(),
		provenance: ReleaseProvenance::default(),
		instantiation: InstantiationScope::default(),
		api_crates: vec!["polars_core".into()],
		unordered: vec![],
		excluded_oracle: vec![],
		refused: vec![],
		families: FamilyTables::default(),
	};
	let world = World::new(&inv, &release, &["mechanical"]);
	let get = |k: &str| calls.iter().find(|c| c.key == k).unwrap();
	// operators: each malformed row is refused before any text (op_arms emits none)
	for bad in [
		"op_bad_recv",
		"op_bad_assoc",
		"op_bad_param",
		"op_bad_bound",
		"op_outward",
		"op_bad_out",
	] {
		assert!(
			op_arms(&world, get(bad)).is_err(),
			"{bad}: a malformed operator row was admitted"
		);
	}
	let mut arms = op_arms(&world, get("op_owned_self")).expect("op_owned_self");
	arms.extend(op_arms(&world, get("op_ok")).expect("op_ok"));
	arms.extend(op_arms(&world, get("op_scalar")).expect("op_scalar"));
	assert_eq!(
		arms.len(),
		4,
		"owned Self + borrowed Self + i64 + f64: {arms:?}"
	);
	let mut out = Emitted {
		from_names: BTreeMap::new(),
		functions: String::new(),
		registrations: vec![],
		catalogue: vec![],
		entries: vec![],
		taken: BTreeMap::new(),
		fn_index: 0,
	};
	let used = emit_op_groups(&world, &mut out, arms);
	assert!(
		out.functions.contains("protocol = ADD")
			&& out.functions.contains("rune::from_value::<i64>")
			&& out.functions.contains("rhs.borrow_ref::<"),
		"{}",
		out.functions
	);
	assert_eq!(
		out.registrations.len(),
		1,
		"one Rune slot per (type, operator)"
	);
	assert!(used["op_ok"].is_ok() && used["op_scalar"].is_ok());
	// the borrowed impl wins its operand kind: its body, its row generated, the owned twin refused
	assert!(
		out.functions.contains("&this.0 + &__b.0")
			&& !out.functions.contains("this.0.clone() + __b.0.clone()"),
		"{}",
		out.functions
	);
	assert!(
		used["op_owned_self"].is_err(),
		"the owned twin must be refused: {:?}",
		used["op_owned_self"]
	);
	// FromIterator: the well-formed row binds; each malformed row writes nothing
	for (k, want) in [
		("fi_ok", true),
		("fi_bad_item", false),
		("fi_bad_recv", false),
		("fi_bad_ret", false),
	] {
		let before = (out.functions.len(), out.registrations.len());
		emit_from_iter(&world, &mut out, get(k));
		let wrote = (out.functions.len(), out.registrations.len()) != before;
		let e = out.entries.last().unwrap();
		assert_eq!(wrote, want, "{k}: {} {:?}", e.status, e.reason);
		assert_eq!(
			e.status == "generated",
			want,
			"{k}: {} {:?}",
			e.status,
			e.reason
		);
	}
	assert!(out.functions.contains("path = ") && out.functions.contains("::from_iter_u8)"));
	// Index: ranges, a key that is not the trait argument, and a non-reference return are refused
	for bad in ["ix_range", "ix_bad_key", "ix_bad_ret"] {
		assert!(
			index_arm(&world, get(bad)).is_err(),
			"{bad}: a malformed Index row was admitted"
		);
	}
	let (o, k, x) = index_arm(&world, get("ix_ok")).expect("ix_ok");
	let before = out.registrations.len();
	let used = emit_index_groups(&world, &mut out, &[("ix_ok".into(), o, k, x)]);
	assert!(
		used["ix_ok"].is_ok()
			&& out.registrations.len() == before + 1
			&& out.functions.contains("protocol = INDEX_GET")
	);
	println!("generic-impls self-test: ok");
}

/// Record 0113: the named contract each remaining generic-bucket trait impl
/// family lacks in this record (serde, operators, FromIterator and Index are
/// decided by their own rules). Every row gets its family's reason instead
/// of the bare bucket label.
pub(crate) fn generic_impl_refusal(c: &Callable) -> Option<String> {
	if c.kind != "foreign_trait_impl" || c.bucket != "generic" {
		return None;
	}
	let path = c
		.canonical_path
		.split(" as ")
		.nth(1)
		.unwrap_or("")
		.split('<')
		.next()
		.unwrap_or("");
	let tr = last(path);
	let why = match tr {
		"Deref" | "DerefMut" | "AsRef" | "AsMut" | "Borrow" | "BorrowMut" => {
			"borrowed view: the target is a reference into Polars storage, and no owned snapshot rule covers it in this record"
		}
		"Iterator"
		| "IntoIterator"
		| "DoubleEndedIterator"
		| "ExactSizeIterator"
		| "ParallelIterator"
		| "IntoParallelIterator" => {
			"iteration: the items borrow the receiver or have no proven finite bound, and no snapshot rule covers them in this record"
		}
		"From" | "TryFrom" | "Into" | "FromParallelIterator" => {
			"conversion: an outward target, a foreign or Arrow source, or a generic head without a proven value mapping"
		}
		"Extend" => {
			"in-place mutation: Extend's partial-mutation contract on failure is not defined in this record"
		}
		"Clone" | "Debug" | "Default" | "PartialEq" | "Eq" | "Hash" | "PartialOrd" | "Ord" => {
			"value protocol on a generic or lifetime-bearing head: the wrapped instantiations carry their own protocols"
		}
		"Binary" | "LowerHex" | "UpperHex" | "Octal" | "LowerExp" | "UpperExp" | "Display" => {
			"formatting: a bounded string builder for this format is not defined in this record"
		}
		"Write" | "AsyncWrite" | "Read" | "AsyncRead" | "Seek" | "Error" | "DnsResolver"
		| "Future" => "service trait: no Rune abstraction with a proven resource or lifetime contract",
		_ => "generic trait impl without a family rule in this record",
	};
	Some(format!("{tr}: {why}"))
}

/// Record 0113: `Index` rows in the generic bucket.
pub(crate) fn index_row(c: &Callable) -> bool {
	c.kind == "foreign_trait_impl"
		&& c.bucket == "generic"
		&& c.canonical_path
			.split(" as ")
			.nth(1)
			.is_some_and(|p| p.split('<').next() == Some("core::ops::index::Index"))
}

/// Record 0113: prove an `Index` row: `&self`, one key parameter equal to the
/// recorded trait argument (`usize` or `&str`), `&Self::Output` with `Output`
/// a wrapped Clone type (cloned out, no borrow escapes); the head the owner.
/// Returns (owner, key, output).
pub(crate) fn index_arm(
	world: &World,
	c: &Callable,
) -> Result<(String, &'static str, String), String> {
	let arg = c
		.name
		.strip_prefix("Index<")
		.and_then(|s| s.strip_suffix('>'))
		.unwrap_or("");
	if c.receiver != "&self"
		|| c.params.len() != 1
		|| c.ret_canonical.as_deref() != Some("&Self::Output")
	{
		return Err(format!(
			"Index: receiver `{}`, {} params, return {:?} is not the index shape",
			c.receiver,
			c.params.len(),
			c.ret_canonical
		));
	}
	if c.impl_for.as_deref() != Some(c.owner.as_str())
		|| !c.impl_bounds.is_empty()
		|| !c.impl_where.is_empty()
	{
		return Err(format!(
			"Index: head {:?} with bounds {:?} is not the concrete owner",
			c.impl_for, c.impl_bounds
		));
	}
	let [(assoc, output)] = c.impl_assoc.as_slice() else {
		return Err(format!(
			"Index: associated types {:?} are not exactly `Output`",
			c.impl_assoc
		));
	};
	if assoc != "Output" {
		return Err(format!("Index: associated type `{assoc}` is not `Output`"));
	}
	let key: &'static str = match (arg, c.params[0].ty_canonical.as_str()) {
		("usize", "usize") => "usize",
		("&str", "&str") => "&str",
		_ => {
			return Err(format!(
				"Index<{arg}>: key `{}` is not an admitted key (usize, &str; range keys return an unsized slice view)",
				c.params[0].ty_canonical
			));
		}
	};
	if output.starts_with('[') {
		return Err(format!(
			"Index<{arg}>: output `{output}` is an unsized slice view"
		));
	}
	if world.wrapper_for(output).is_none() || !world.clonable.contains(output) {
		return Err(format!(
			"Index<{arg}>: output `{output}` is not a wrapped Clone type"
		));
	}
	if world.wrapper_for(&c.owner).is_none() {
		return Err(format!("Index: owner `{}` is not wrapped", c.owner));
	}
	Ok((c.owner.clone(), key, output.clone()))
}

/// Record 0113: one Rune `INDEX_GET` per owner over its proven keys; the value
/// is cloned out of Polars' reference. A missing name or position panics in
/// Polars exactly as the Rust `Index` does (the engine boundary propagates
/// panics; the oracle compares them).
pub(crate) fn emit_index_groups(
	world: &World,
	out: &mut Emitted,
	arms: &[(String, String, &'static str, String)],
) -> BTreeMap<String, Result<(), String>> {
	let mut groups: BTreeMap<String, Vec<&(String, String, &'static str, String)>> =
		BTreeMap::new();
	for a in arms {
		groups.entry(a.1.clone()).or_default().push(a);
	}
	let mut used = BTreeMap::new();
	for (owner, arms) in groups {
		let w = &world.wrappers[&owner];
		let outs: BTreeSet<&str> = arms.iter().map(|a| a.3.as_str()).collect();
		if outs.len() != 1
			|| out
				.taken
				.contains_key(&(w.rust.clone(), "<Index>".to_string()))
		{
			for a in &arms {
				used.insert(
					a.0.clone(),
					Err(
						"the INDEX_GET slot cannot hold these keys (different outputs, or taken)"
							.to_string(),
					),
				);
			}
			continue;
		}
		let out_w = &world.wrappers[*outs.iter().next().unwrap()];
		let idx = out.fn_index;
		out.fn_index += 1;
		let ident = rust_ident("x", &format!("{owner}#Index"), idx);
		let mut body = String::new();
		for a in &arms {
			match a.2 {
				"usize" => body.push_str(&format!("if let Ok(__i) = rune::from_value::<i64>(key.clone()) {{ let __i = support::narrow::<usize>(__i, \"index\")?; return Ok({}(this.0[__i].clone())); }} ", out_w.rust)),
				_ => body.push_str(&format!("if let Ok(__s) = rune::from_value::<String>(key.clone()) {{ return Ok({}(this.0[__s.as_str()].clone())); }} ", out_w.rust)),
			}
		}
		writeln!(out.functions, "/// Polars: `{owner} as core::ops::index::Index` (record 0113: one INDEX_GET over its proven keys).\n#[rune::function(instance, protocol = INDEX_GET)]\nfn {ident}(this: &{}, key: rune::Value) -> Result<{}, Error> {{ {body}Err(Error::conversion(\"{}[]: the key is not an admitted key\")) }}", w.rust, out_w.rust, last(&owner)).unwrap();
		out.registrations
			.push(format!("m.function_meta({ident})?;"));
		out.taken.insert(
			(w.rust.clone(), "<Index>".to_string()),
			format!("{owner} (record 0113 index group)"),
		);
		for a in &arms {
			used.insert(a.0.clone(), Ok(()));
		}
	}
	used
}

/// Record 0113: `FromIterator` rows in the generic bucket.
pub(crate) fn from_iter_row(c: &Callable) -> bool {
	c.kind == "foreign_trait_impl"
		&& c.bucket == "generic"
		&& c.canonical_path.split(" as ").nth(1).is_some_and(|p| {
			p.split('<').next() == Some("core::iter::traits::collect::FromIterator")
		})
}

/// Record 0113: the admitted `FromIterator` item grammar, from the trait
/// argument the inventory records in `name`: (owned vector element, iterator
/// over `__v` producing the exact item, constructor suffix). `None` is a
/// refusal: the item is outside the grammar.
pub(crate) fn from_iter_item(
	item: &str,
	owner: &str,
	native: Option<&str>,
) -> Option<(String, String, String)> {
	const PRIMS: &[&str] = &[
		"bool", "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64",
	];
	let into = "__v.into_iter()".to_string();
	if PRIMS.contains(&item) {
		return Some((item.to_string(), into, item.to_string()));
	}
	if let Some(p) = item.strip_prefix('&').filter(|p| PRIMS.contains(p)) {
		return Some((p.to_string(), "__v.iter()".into(), format!("ref_{p}")));
	}
	if let Some(p) = item
		.strip_prefix("Option<")
		.and_then(|s| s.strip_suffix('>'))
		.filter(|p| PRIMS.contains(p))
	{
		return Some((
			format!("core::option::Option<{p}>"),
			into,
			format!("option_{p}"),
		));
	}
	match item {
		"String" => Some(("alloc::string::String".into(), into, "string".into())),
		"&str" => Some((
			"alloc::string::String".into(),
			"__v.iter().map(|s| s.as_str())".into(),
			"str".into(),
		)),
		"Option<&str>" => Some((
			"core::option::Option<alloc::string::String>".into(),
			"__v.iter().map(|s| s.as_deref())".into(),
			"option_str".into(),
		)),
		"Option<String>" => Some((
			"core::option::Option<alloc::string::String>".into(),
			into,
			"option_string".into(),
		)),
		"Option<<T as PolarsNumericType>::Native>" => native.map(|n| {
			(
				format!("core::option::Option<{n}>"),
				into,
				"option_native".into(),
			)
		}),
		i if i == last(owner) => Some((owner.to_string(), into, "self".into())),
		_ => None,
	}
}

/// Record 0113: `Type::from_iter_<item>(vector)`: the script vector converted
/// by the existing argument rules (narrow integers checked, `Option` for
/// nulls), then `<Owner as FromIterator<_>>::from_iter` over the exact item.
pub(crate) fn emit_from_iter(world: &World, out: &mut Emitted, c: &Callable) {
	let item = c
		.name
		.strip_prefix("FromIterator<")
		.and_then(|s| s.strip_suffix('>'))
		.unwrap_or("")
		.to_string();
	let shape_ok = c.receiver == "none"
		&& c.params.len() == 1
		&& c.impl_assoc.is_empty()
		&& c.impl_where.is_empty()
		&& c.generics_canonical.is_empty();
	if !shape_ok {
		out.unsupported(
			c,
			"from_iter shape",
			&format!(
				"receiver `{}`, {} params, assoc {:?}",
				c.receiver,
				c.params.len(),
				c.impl_assoc
			),
		);
		return;
	}
	let head = c.impl_for.clone().unwrap_or_default();
	// concrete receivers: the owner, an alias by exact identity, or each alias a generic ChunkedArray<T> head admits
	let mut receivers: Vec<(String, Option<String>)> = Vec::new();
	if head.contains("<T>") && head.starts_with("polars_core::chunked_array::ChunkedArray<") {
		let mut probe = c.clone();
		probe.impl_head = Some(head.clone());
		let mut cands: Vec<(&String, &Wrapper)> = world
			.wrappers
			.iter()
			.filter(|(_, w)| {
				w.rule == "alias"
					&& w.base.as_deref() == Some("polars_core::chunked_array::ChunkedArray")
					&& w.identity
						.starts_with("polars_core::chunked_array::ChunkedArray<")
			})
			.collect();
		cands.sort_by(|a, b| a.0.cmp(b.0));
		let mut seen: BTreeSet<&str> = BTreeSet::new();
		for (path, w) in cands {
			if !seen.insert(w.identity.as_str()) {
				continue;
			}
			if let (Applicability::Proven, subst) = world.applicability(&probe, &w.identity) {
				let native = world
					.resolve_projections(&subst_params("T::Native", &subst))
					.ok();
				receivers.push((path.clone(), native));
			}
		}
	} else if !c.impl_bounds.is_empty() {
		out.unsupported(
			c,
			"from_iter head",
			&format!(
				"bounds {:?} on `{head}` are not decided here",
				c.impl_bounds
			),
		);
		return;
	} else if world.wrapper_for(&head).is_some() {
		receivers.push((head.clone(), None));
	} else if let Some(p) = world.by_identity.get(&ty::parse(&head).render()) {
		receivers.push((p.clone(), None));
	}
	if receivers.is_empty() {
		out.unsupported(
			c,
			"from_iter head",
			&format!("`{head}` is not a wrapped receiver"),
		);
		return;
	}
	let ret_ok = c.ret_canonical.as_deref() == Some("Self")
		|| c.ret_canonical.as_deref() == Some(c.owner.as_str());
	if !ret_ok {
		out.unsupported(
			c,
			"from_iter shape",
			&format!("return {:?} is not `Self`", c.ret_canonical),
		);
		return;
	}
	let mut done: Vec<(String, String, &'static str, Option<String>)> = Vec::new();
	let mut infos: Vec<OracleInfo> = Vec::new();
	let mut why: Option<String> = None;
	for (recv, native) in receivers {
		let w = &world.wrappers[&recv];
		let Some((elem, iter, suffix)) = from_iter_item(&item, &recv, native.as_deref())
			.or_else(|| from_iter_item(&item, &c.owner, native.as_deref()))
		else {
			why = Some(format!(
				"item `{item}` is outside the admitted grammar (primitives, their Option and & forms, strings, the owner itself)"
			));
			continue;
		};
		let vec_ty = format!("alloc::vec::Vec<{elem}>");
		let arg = match world.arg(&ty::parse(&vec_ty), "v", &BTreeMap::new(), Some(&recv), 0) {
			Ok(a) => a,
			Err(Unsupported(r, what)) => {
				why = Some(format!("item vector `{vec_ty}`: {r} ({what})"));
				continue;
			}
		};
		let name = format!("from_iter_{suffix}");
		let key = (w.rust.clone(), name.clone());
		if let Some(prev) = out.taken.get(&key) {
			why = Some(format!("name `{name}` taken on this type by {prev}"));
			continue;
		}
		let idx = out.fn_index;
		out.fn_index += 1;
		let ident = rust_ident("i", &format!("{}#{recv}#{}", c.canonical_path, c.key), idx);
		let pre: String = arg.pre.iter().map(|p| format!("{p} ")).collect();
		// review of 0113: the whole call is bounded before anything is copied
		let pre = format!(
			"support::vec_len_bounded(&v, \"{}::{name}\")?; {pre}",
			last(&recv)
		);
		writeln!(out.functions, "/// Polars: `{0}` (record 0113: FromIterator<{item}>).\n#[rune::function(free, path = {1}::{name})]\nfn {ident}(v: {2}) -> Result<{1}, Error> {{ {pre}let __v: Vec<{3}> = {4}; Ok({1}(<{5} as core::iter::FromIterator<_>>::from_iter({iter}))) }}", c.canonical_path, w.rust, arg.rust_ty, rust_spell_elem(&elem, world), arg.conv, w.spell).unwrap();
		out.registrations
			.push(format!("m.function_meta({ident})?;"));
		out.taken.insert(key, c.canonical_path.clone());
		let info = OracleInfo {
			rune_owner: Some(rune_path(w)),
			rune_name: name.clone(),
			receiver: "protocol".into(),
			owner: Some((recv.clone(), w.rust.clone())),
			callee: "FromIterGen".into(),
			params: vec![(arg.shape.clone(), vec_ty.clone())],
			param_names: vec![iter.clone()],
			ret_canonical: Some(recv.clone()),
			ret_rust: w.rust.clone(),
			fallible: true,
			generics: BTreeMap::new(),
			implementors: vec![],
			deref: false,
		};
		done.push((
			format!("{}::{name}", rune_path(w)),
			recv.clone(),
			"free",
			None,
		));
		infos.push(info);
	}
	if done.is_empty() {
		out.unsupported(c, "from_iter", why.as_deref().unwrap_or("no receiver"));
		return;
	}
	let mut info = infos[0].clone();
	info.implementors = infos.iter().filter_map(|i| i.owner.clone()).collect();
	out.generated_on(c, &done, info);
	let entry = out.entries.last_mut().unwrap();
	for (b, i) in entry.bindings.iter_mut().zip(infos) {
		// several rows share a path (one per item type): the key keeps ids unique
		b.id = binding_id(
			&c.canonical_path,
			Some(&format!(
				"{}|{}",
				b.receiver.clone().unwrap_or_default(),
				c.key
			)),
			false,
		);
		b.info = Some(i);
	}
}

/// Record 0113: the Rust spelling of a vector element type the item grammar admits.
pub(crate) fn rust_spell_elem(elem: &str, world: &World) -> String {
	match elem {
		"alloc::string::String" => "String".into(),
		"core::option::Option<alloc::string::String>" => "Option<String>".into(),
		e if e.starts_with("core::option::Option<") => {
			format!("Option<{}>", &e["core::option::Option<".len()..e.len() - 1])
		}
		e => world
			.wrappers
			.get(e)
			.map(|w| w.spell.clone())
			.unwrap_or_else(|| e.to_string()),
	}
}

/// Record 0113: `Neg`/`Not` in the generic bucket, admitted only on the exact
/// concrete shape: `self`, no parameter, `Self::Output` with `Output` the
/// owner itself, the head the owner, no bound, clause or generic.
pub(crate) fn unary_generic_shape(c: &Callable) -> bool {
	let path = c.canonical_path.split(" as ").nth(1).unwrap_or("");
	c.kind == "foreign_trait_impl"
		&& c.bucket == "generic"
		&& matches!(path, "core::ops::arith::Neg" | "core::ops::bit::Not")
		&& c.receiver == "self"
		&& c.params.is_empty()
		&& c.ret_canonical.as_deref() == Some("Self::Output")
		&& c.impl_for.as_deref() == Some(c.owner.as_str())
		&& c.impl_assoc.len() == 1
		&& c.impl_assoc[0].0 == "Output"
		&& c.impl_assoc[0].1 == c.owner
		&& c.impl_bounds.is_empty()
		&& c.impl_where.is_empty()
		&& c.generics_canonical.is_empty()
}

/// Record 0113: the arithmetic and bitwise operator family on generic-bucket
/// impls. (Rust trait name, canonical path, Rune protocol, operator.)
pub(crate) const GENERIC_OPS: &[(&str, &str, &str, &str)] = &[
	("Add", "core::ops::arith::Add", "ADD", "+"),
	("Sub", "core::ops::arith::Sub", "SUB", "-"),
	("Mul", "core::ops::arith::Mul", "MUL", "*"),
	("Div", "core::ops::arith::Div", "DIV", "/"),
	("Rem", "core::ops::arith::Rem", "REM", "%"),
	("BitAnd", "core::ops::bit::BitAnd", "BIT_AND", "&"),
	("BitOr", "core::ops::bit::BitOr", "BIT_OR", "|"),
	("BitXor", "core::ops::bit::BitXor", "BIT_XOR", "^"),
];

/// Record 0113: the scalar bounds a script `i64`/`f64` is proven to satisfy
/// (num-traits 0.2.19, pinned in both locks: `Num` src/lib.rs:175,392,
/// `ToPrimitive` src/cast.rs:196,375, `NumCast` src/cast.rs:692,696).
pub(crate) const SCALAR_OP_BOUNDS: &[&str] = &[
	"num_traits::Num + num_traits::cast::NumCast",
	"num_traits::Num + num_traits::cast::ToPrimitive",
];

/// Record 0113: the right-hand side of one operator arm.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum OpRhs {
	/// A wrapped value (canonical path), borrowed or owned.
	Wrapped(String, bool),
	I64,
	F64,
}

/// Record 0113: one proven operator arm: `lhs op rhs` on the wrapper whose
/// canonical path is `lhs`, borrowed or owned, returning the wrapped `out`.
#[derive(Clone, Debug)]
pub(crate) struct OpArm {
	pub(crate) key: String,
	pub(crate) path: String,
	pub(crate) lhs: String,
	pub(crate) lhs_ref: bool,
	pub(crate) rhs: OpRhs,
	pub(crate) out: String,
	pub(crate) fallible: bool,
	pub(crate) proto: &'static str,
	pub(crate) op: &'static str,
}

/// Record 0113: the operator rows this family decides (by trait path), for
/// the generic bucket only; concrete non-generic operators keep `emit_foreign`.
pub(crate) fn generic_op(
	c: &Callable,
) -> Option<&'static (&'static str, &'static str, &'static str, &'static str)> {
	if c.kind != "foreign_trait_impl" || c.bucket != "generic" {
		return None;
	}
	let path = c.canonical_path.split(" as ").nth(1)?.split('<').next()?;
	GENERIC_OPS.iter().find(|(_, p, _, _)| *p == path)
}

/// Record 0113: prove a generic-bucket operator row, exactly, into arms: the
/// shape (`self`, one parameter, `Self::Output`, `Output` from the impl
/// record), the head (the owner or `&owner`; a generic `ChunkedArray<T>`
/// head per alias through `applicability`), the trait argument (`Self`, a
/// wrapped type, or a scalar parameter with exactly a cited bound), and the
/// output (a wrapped type, or `Result<wrapped, PolarsError>`).
pub(crate) fn op_arms(world: &World, c: &Callable) -> Result<Vec<OpArm>, String> {
	let (tname, _, proto, op) = generic_op(c).ok_or("not an operator row")?;
	if c.receiver != "self"
		|| c.params.len() != 1
		|| c.ret_canonical.as_deref() != Some("Self::Output")
	{
		return Err(format!(
			"{tname}: receiver `{}`, {} params, return {:?} is not the operator shape",
			c.receiver,
			c.params.len(),
			c.ret_canonical
		));
	}
	let [(assoc, output)] = c.impl_assoc.as_slice() else {
		return Err(format!(
			"{tname}: associated types {:?} are not exactly `Output`",
			c.impl_assoc
		));
	};
	if assoc != "Output" {
		return Err(format!(
			"{tname}: associated type `{assoc}` is not `Output`"
		));
	}
	let head = c.impl_for.clone().unwrap_or_default();
	let lhs_ref = head.starts_with('&');
	let head_ty = head.trim_start_matches('&').to_string();
	let head_base = head_ty.split('<').next().unwrap_or("").to_string();
	if head_base != c.owner {
		return Err(format!(
			"{tname}: outward listing, the impl is for `{head}`, bound by that type's own listing"
		));
	}
	// the trait argument, carried by the recorded name
	let arg = c
		.name
		.strip_prefix(tname)
		.unwrap_or("")
		.trim()
		.trim_start_matches('<')
		.trim_end_matches('>')
		.trim()
		.to_string();
	let arg = if arg.is_empty() {
		"Self".to_string()
	} else {
		arg
	};
	let param = c.params[0].ty_canonical.clone();
	// concrete receivers: the owner itself, or each alias a generic head admits
	let mut receivers: Vec<(String, BTreeMap<String, String>)> = Vec::new();
	let generic_head = head_ty.contains('<');
	let non_scalar_bounds: Vec<(String, String)> = c
		.impl_bounds
		.iter()
		.filter(|(_, b)| !SCALAR_OP_BOUNDS.contains(&b.as_str()))
		.cloned()
		.collect();
	if !c.impl_where.is_empty() {
		return Err(format!(
			"{tname}: where clauses {:?} are not decided here",
			c.impl_where
		));
	}
	if generic_head {
		let mut probe = c.clone();
		probe.impl_head = Some(ty::parse(&head_ty).render());
		probe.impl_bounds = non_scalar_bounds.clone();
		probe.impl_where = vec![];
		let mut cands: Vec<(&String, &Wrapper)> = world
			.wrappers
			.iter()
			.filter(|(_, w)| {
				w.rule == "alias"
					&& w.base.as_deref() == Some(head_base.as_str())
					&& w.identity.starts_with(&format!("{head_base}<"))
			})
			.collect();
		cands.sort_by(|a, b| a.0.cmp(b.0));
		let mut seen: BTreeSet<&str> = BTreeSet::new();
		for (path, w) in cands {
			if !seen.insert(w.identity.as_str()) {
				continue;
			}
			if let (Applicability::Proven, subst) = world.applicability(&probe, &w.identity) {
				receivers.push((path.clone(), subst));
			}
		}
		if receivers.is_empty() {
			return Err(format!(
				"{tname}: no wrapped instantiation of `{head_ty}` satisfies {:?}",
				non_scalar_bounds
			));
		}
	} else {
		if !non_scalar_bounds.is_empty() {
			return Err(format!(
				"{tname}: bounds {non_scalar_bounds:?} on a concrete head are not decided here"
			));
		}
		if world.wrapper_for(&head_ty).is_none() {
			return Err(format!("{tname}: `{head_ty}` is not wrapped"));
		}
		receivers.push((head_ty.clone(), BTreeMap::new()));
	}
	let mut arms = Vec::new();
	for (recv, subst) in receivers {
		let identity = world.wrappers[&recv].identity.clone();
		// the right-hand side
		let rhs: Vec<OpRhs> = if arg == "Self" {
			if param != "Self" {
				return Err(format!("{tname}<Self>: parameter `{param}` is not `Self`"));
			}
			vec![OpRhs::Wrapped(recv.clone(), lhs_ref)]
		} else if let Some((_, bound)) = c.impl_bounds.iter().find(|(n, _)| *n == arg) {
			if !SCALAR_OP_BOUNDS.contains(&bound.as_str()) || param != arg {
				return Err(format!(
					"{tname}<{arg}>: scalar bound `{bound}` / parameter `{param}` is not a cited native bound"
				));
			}
			vec![OpRhs::I64, OpRhs::F64]
		} else {
			let (r_ref, r_ty) = match arg.strip_prefix('&') {
				Some(x) => (true, x.to_string()),
				None => (false, arg.clone()),
			};
			let full = if param
				.trim_start_matches('&')
				.ends_with(&format!("::{r_ty}"))
				|| param.trim_start_matches('&') == r_ty
			{
				param.trim_start_matches('&').to_string()
			} else {
				return Err(format!(
					"{tname}<{arg}>: parameter `{param}` does not match the trait argument"
				));
			};
			if param.starts_with('&') != r_ref {
				return Err(format!(
					"{tname}<{arg}>: parameter `{param}` borrows differently from the trait argument"
				));
			}
			match world.wrapper_for(&full).or_else(|| {
				world
					.by_identity
					.get(&full)
					.and_then(|p| world.wrapper_for(p))
			}) {
				Some(_) => vec![OpRhs::Wrapped(
					world
						.by_identity
						.get(&full)
						.cloned()
						.unwrap_or(full.clone()),
					r_ref,
				)],
				None => {
					return Err(format!(
						"{tname}<{arg}>: the right-hand side `{full}` is not wrapped"
					));
				}
			}
		};
		// the output, substituted
		let out_ty = subst_params(output, &subst);
		let (fallible, out_inner) = match ty::parse(&out_ty) {
			Ty::Path { path, args }
				if (path == "core::result::Result"
					&& args.len() == 2
					&& args[1].render() == "polars_error::PolarsError") =>
			{
				(true, args[0].render())
			}
			other => (false, other.render()),
		};
		let out = if out_inner == "Self"
			|| out_inner == head_ty
			|| ty::parse(&out_inner).render() == identity
		{
			recv.clone()
		} else if world.wrapper_for(&out_inner).is_some() {
			out_inner.clone()
		} else if let Some(p) = world.by_identity.get(&out_inner) {
			p.clone()
		} else {
			return Err(format!("{tname}: output `{out_ty}` is not a wrapped type"));
		};
		for r in rhs {
			arms.push(OpArm {
				key: c.key.clone(),
				path: c.canonical_path.clone(),
				lhs: recv.clone(),
				lhs_ref,
				rhs: r,
				out: out.clone(),
				fallible,
				proto,
				op,
			});
		}
	}
	Ok(arms)
}

/// Record 0113: one Rune operator per (wrapper, protocol), dispatching on the
/// runtime right-hand side over its proven arms; a borrowed-head arm serves
/// its operand kind, an owned-head arm only where no borrowed arm exists for
/// that kind (and the owner is Clone). Returns the per-key disposition.
pub(crate) fn emit_op_groups(
	world: &World,
	out: &mut Emitted,
	arms: Vec<OpArm>,
) -> BTreeMap<String, Result<Vec<OpArm>, String>> {
	let mut groups: BTreeMap<(String, &'static str), Vec<OpArm>> = BTreeMap::new();
	for a in arms {
		groups.entry((a.lhs.clone(), a.proto)).or_default().push(a);
	}
	let mut used: BTreeMap<String, Result<Vec<OpArm>, String>> = BTreeMap::new();
	for ((lhs, proto), mut arms) in groups {
		let w = &world.wrappers[&lhs];
		let tname = GENERIC_OPS.iter().find(|x| x.2 == proto).unwrap().0;
		let refuse = |used: &mut BTreeMap<String, Result<Vec<OpArm>, String>>,
		              arms: &[OpArm],
		              why: String| {
			for a in arms {
				used.entry(a.key.clone()).or_insert(Err(why.clone()));
			}
		};
		if HAND_PROTOCOLS.iter().any(|(o, t)| *o == lhs && *t == tname) {
			refuse(
				&mut used,
				&arms,
				format!(
					"operator on a hand-written wrapper: `{tname}` is owned by the hand-written adapter"
				),
			);
			continue;
		}
		if let Some(prev) = out.taken.get(&(w.rust.clone(), format!("<{tname}>"))) {
			refuse(
				&mut used,
				&arms,
				format!("protocol taken on this type by {prev}"),
			);
			continue;
		}
		// one arm per operand kind: borrowed first
		// review of 0113: order by operand kind (which ignores the operand's own
		// borrow), then borrowed head first, so a borrowed impl wins its kind
		let kind_of = |r: &OpRhs| match r {
			OpRhs::Wrapped(p, _) => format!("W:{p}"),
			other => format!("{other:?}"),
		};
		arms.sort_by_key(|a| (kind_of(&a.rhs), !a.lhs_ref, a.key.clone()));
		let mut chosen: Vec<OpArm> = Vec::new();
		for a in &arms {
			let kind = match &a.rhs {
				OpRhs::Wrapped(p, _) => format!("W:{p}"),
				other => format!("{other:?}"),
			};
			if chosen.iter().any(|c| {
				(match &c.rhs {
					OpRhs::Wrapped(p, _) => format!("W:{p}"),
					other => format!("{other:?}"),
				}) == kind
			}) {
				continue;
			}
			if !a.lhs_ref && !world.clonable.contains(&lhs) {
				continue;
			}
			chosen.push(a.clone());
		}
		let outs: BTreeSet<&str> = chosen.iter().map(|a| a.out.as_str()).collect();
		if outs.len() != 1 {
			refuse(
				&mut used,
				&arms,
				format!(
					"`{tname}` arms return different types {outs:?}: one Rune slot cannot hold them"
				),
			);
			continue;
		}
		let out_w = &world.wrappers[*outs.iter().next().unwrap()];
		let idx = out.fn_index;
		out.fn_index += 1;
		let ident = rust_ident("o", &format!("{lhs}#{tname}"), idx);
		let mut body = String::new();
		for a in &chosen {
			let lhs_e = if a.lhs_ref {
				"&this.0".to_string()
			} else {
				"this.0.clone()".to_string()
			};
			let call = |rhs_e: &str| {
				let r = format!("{lhs_e} {} {rhs_e}", a.op);
				if a.fallible {
					format!("return Ok({}(({r}).map_err(Error::from)?));", out_w.rust)
				} else {
					format!("return Ok({}({r}));", out_w.rust)
				}
			};
			match &a.rhs {
				OpRhs::Wrapped(p, r_ref) => {
					let rw = &world.wrappers[p];
					let rhs_e = if *r_ref {
						"&__b.0".to_string()
					} else {
						"__b.0.clone()".to_string()
					};
					body.push_str(&format!(
						"if let Ok(__b) = rhs.borrow_ref::<{}>() {{ {} }} ",
						rw.rust,
						call(&rhs_e)
					));
				}
				OpRhs::I64 => body.push_str(&format!(
					"if let Ok(__v) = rune::from_value::<i64>(rhs.clone()) {{ {} }} ",
					call("__v")
				)),
				OpRhs::F64 => body.push_str(&format!(
					"if let Ok(__v) = rune::from_value::<f64>(rhs.clone()) {{ {} }} ",
					call("__v")
				)),
			}
		}
		let kinds: Vec<String> = chosen
			.iter()
			.map(|a| match &a.rhs {
				OpRhs::Wrapped(p, _) => last(p).to_string(),
				OpRhs::I64 => "int".into(),
				OpRhs::F64 => "float".into(),
			})
			.collect();
		writeln!(out.functions, "/// Polars: `{}` (record 0113: one operator over its proven impls: {}).\n#[rune::function(instance, protocol = {proto})]\nfn {ident}(this: &{}, rhs: rune::Value) -> Result<{}, Error> {{ {body}Err(Error::conversion(\"{} {}: the right-hand side is not {}\")) }}", tname, kinds.join(", "), w.rust, out_w.rust, last(&lhs), chosen[0].op, kinds.join(", ")).unwrap();
		out.registrations
			.push(format!("m.function_meta({ident})?;"));
		out.taken.insert(
			(w.rust.clone(), format!("<{tname}>")),
			format!("{lhs} (record 0113 operator group)"),
		);
		for a in &arms {
			if chosen
				.iter()
				.any(|c| c.key == a.key && c.rhs == a.rhs && c.lhs == a.lhs)
			{
				match used.entry(a.key.clone()).or_insert(Ok(vec![])) {
					Ok(v) => v.push(a.clone()),
					Err(_) => {}
				}
			}
		}
		for a in &arms {
			used.entry(a.key.clone()).or_insert(Err(format!("`{tname}` on {}: the operand kind is served by the borrowed-head impl, or the owner is not Clone", last(&lhs))));
		}
	}
	used
}

/// Record 0113: the entry of an operator row: one binding per arm (operand
/// kind), each with its own oracle information.
pub(crate) fn op_entry(world: &World, out: &mut Emitted, c: &Callable, arms: &[OpArm]) {
	let tname = GENERIC_OPS.iter().find(|x| x.2 == arms[0].proto).unwrap().0;
	let mut bindings = Vec::new();
	for a in arms.iter() {
		let w = &world.wrappers[&a.lhs];
		let (rhs_rune, rhs_rust, rhs_label) = match &a.rhs {
			OpRhs::Wrapped(p, r) => (
				format!("W:{p}"),
				format!("{}{p}", if *r { "&" } else { "" }),
				last(p).to_string(),
			),
			OpRhs::I64 => ("int".to_string(), "2i64".to_string(), "i64".to_string()),
			OpRhs::F64 => ("float".to_string(), "1.5f64".to_string(), "f64".to_string()),
		};
		let info = OracleInfo {
			rune_owner: Some(rune_path(w)),
			rune_name: a.op.to_string(),
			receiver: "protocol".into(),
			owner: Some((a.lhs.clone(), w.rust.clone())),
			callee: "GenericOp".into(),
			params: vec![(rhs_rune, rhs_rust)],
			param_names: vec![
				if a.lhs_ref {
					"ref".into()
				} else {
					"owned".into()
				},
				if a.fallible {
					"fallible".into()
				} else {
					"plain".into()
				},
			],
			ret_canonical: Some(a.out.clone()),
			ret_rust: world.wrappers[&a.out].rust.clone(),
			fallible: true,
			generics: BTreeMap::new(),
			implementors: vec![],
			deref: false,
		};
		let receiver = format!("{}|{rhs_label}", a.lhs);
		bindings.push(Binding {
			// several rows share a path (owned and borrowed heads): the key keeps ids unique
			id: binding_id(
				&c.canonical_path,
				Some(&format!("{receiver}|{}", c.key)),
				false,
			),
			rune: format!("{} {} {rhs_label}", rune_path(w), a.op),
			receiver: Some(a.lhs.clone()),
			route: "protocol",
			route_reason: None,
			reentry: None,
			disposition: None,
			case_id: None,
			callee: None,
			info: Some(info),
		});
	}
	let first = bindings[0].info.clone().unwrap();
	out.entries.push(Entry {
		key: c.key.clone(),
		canonical_path: c.canonical_path.clone(),
		kind: c.kind.clone(),
		bucket: c.bucket.clone(),
		status: "generated",
		fallible: Some(true),
		signature: signature_of(c),
		execution: None,
		oracle: Some(first),
		reason: None,
		rune: Some(
			bindings
				.iter()
				.map(|b| b.rune.clone())
				.collect::<Vec<_>>()
				.join("; "),
		),
		note: Some(format!(
			"record 0113: `{tname}` through the type's dispatching operator"
		)),
		bindings,
		exceptions: vec![],
		counterpart: None,
	});
}

#[cfg(test)]
mod tests {
	#[test]
	fn generic_impls() {
		super::generic_impls_self_test();
	}
}
