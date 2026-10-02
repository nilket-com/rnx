use crate::emit::{Emitted, OracleInfo, rune_path, rust_ident};
use crate::families::conversions::{emit_from, from_impl_is_on_its_owner, impl_identity};
use crate::model::{Callable, Inventory, Param, Supporting};
use crate::release::{FamilyTables, InstantiationScope, Release, ReleaseProvenance};
use crate::text::sanitize;
use crate::ty;
use crate::world::mapping::Unsupported;
use crate::world::{HAND_PROTOCOLS, World};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// Record 0111: protocol admission. `Hash` binds HASH only with a proven
/// `Eq` (which installs EQ); without `Eq` it is refused; `TrivialClone` is
/// a marker; `PartialOrd` on `&Self` binds PARTIAL_CMP; bitflags traits are
/// refused by name.
pub(crate) fn protocols_self_test() {
	let keyed = "polars_core::datatypes::TimeUnit";
	let loose = "polars_ops::frame::join::JoinType";
	let sup = |path: &str| Supporting {
		trait_params: vec![],
		key: path.to_string(),
		kind: "enum".into(),
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
	let imp = |owner: &str,
	           tr: &str,
	           path: &str,
	           receiver: &str,
	           params: Vec<&str>,
	           ret: Option<&str>| Callable {
		key: format!("{owner}:{tr}"),
		kind: "foreign_trait_impl".into(),
		krate: "polars_core".into(),
		owner: owner.into(),
		name: path.rsplit("::").next().unwrap().into(),
		canonical_path: format!("{owner} as {path}"),
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
		ret_canonical: ret.map(String::from),
		generics_canonical: vec![],
		impl_for: Some(owner.into()),
		impl_bounds: vec![],
		impl_head: None,
		impl_where: vec![],
		impl_assoc: vec![],
		docs_first: None,
		owner_generic: false,
		is_unsafe: false,
		is_async: false,
		deprecated: false,
		hidden: false,
		implementors: vec![],
		trait_reachable: true,
		derived: false,
		bucket: "mechanical".into(),
		rules: vec![],
		trait_lifetimes: vec![],
	};
	let opt = "core::option::Option<core::cmp::Ordering>";
	let calls = vec![
		imp(keyed, "Eq", "core::cmp::Eq", "&self", vec![], None),
		// the real inventory shape: `fn hash<H: Hasher>(&self, state: &mut H)`
		imp(
			keyed,
			"Hash",
			"core::hash::Hash",
			"&self",
			vec!["&mut __H"],
			None,
		),
		imp(
			keyed,
			"TrivialClone",
			"core::clone::TrivialClone",
			"none",
			vec![],
			None,
		),
		imp(
			keyed,
			"PartialOrd",
			"core::cmp::PartialOrd",
			"&self",
			vec!["&Self"],
			Some(opt),
		),
		imp(
			keyed,
			"Flags",
			"bitflags::traits::Flags",
			"&self",
			vec![],
			Some("u32"),
		),
		imp(
			loose,
			"Hash",
			"core::hash::Hash",
			"&self",
			vec!["&mut H"],
			None,
		),
		// review of 0111: drifted shapes must be refused before any text
		imp(
			keyed,
			"bad_hash_param",
			"core::hash::Hash",
			"&self",
			vec!["&mut u64"],
			None,
		),
		imp(
			keyed,
			"bad_hash_ret",
			"core::hash::Hash",
			"&self",
			vec!["&mut H"],
			Some("bool"),
		),
		imp(
			keyed,
			"bad_hash_path",
			"other::hash::Hash",
			"&self",
			vec!["&mut H"],
			None,
		),
		imp(
			keyed,
			"bad_partial_ord_ret",
			"core::cmp::PartialOrd",
			"&self",
			vec!["&Self"],
			Some("bool"),
		),
		imp(
			keyed,
			"bad_ord_params",
			"core::cmp::Ord",
			"&self",
			vec!["&Self", "&Self"],
			Some("core::cmp::Ordering"),
		),
		imp(
			keyed,
			"bad_from_str_recv",
			"core::str::traits::FromStr",
			"&self",
			vec!["&str"],
			Some("core::result::Result<Self, Self::Err>"),
		),
		imp(
			keyed,
			"bad_from_str_ret",
			"core::str::traits::FromStr",
			"none",
			vec!["&str"],
			Some("Self"),
		),
		imp(
			keyed,
			"bad_eq_param",
			"core::cmp::Eq",
			"&self",
			vec!["&Self"],
			None,
		),
	];
	let inv = Inventory {
		callables: calls.clone(),
		supporting: vec![sup(keyed), sup(loose)],
		provenance: None,
	};
	let release = Release {
		name: "t".into(),
		source: "t".into(),
		provenance: ReleaseProvenance::default(),
		instantiation: InstantiationScope::default(),
		api_crates: vec!["polars_core".into(), "polars_ops".into()],
		unordered: vec![],
		excluded_oracle: vec![],
		refused: vec![],
		families: FamilyTables::default(),
	};
	let world = World::new(&inv, &release, &["mechanical"]);
	let mut out = Emitted {
		from_names: BTreeMap::new(),
		functions: String::new(),
		registrations: vec![],
		catalogue: vec![],
		consuming: vec![],
		entries: vec![],
		taken: BTreeMap::new(),
		fn_index: 0,
		frozen_ids: Default::default(),
	};
	let mut wrote: BTreeMap<String, bool> = BTreeMap::new();
	for c in &calls {
		let before = (out.functions.len(), out.registrations.len());
		emit_foreign(&world, &mut out, c);
		wrote.insert(
			c.key.clone(),
			(out.functions.len(), out.registrations.len()) != before,
		);
	}
	for bad in [
		"bad_hash_param",
		"bad_hash_ret",
		"bad_hash_path",
		"bad_partial_ord_ret",
		"bad_ord_params",
		"bad_from_str_recv",
		"bad_from_str_ret",
		"bad_eq_param",
	] {
		let k = format!("{keyed}:{bad}");
		let e = out.entries.iter().find(|e| e.key == k).unwrap();
		assert!(
			e.status == "unsupported"
				&& e.reason
					.as_deref()
					.unwrap_or("")
					.starts_with("protocol shape"),
			"{bad}: {} {:?}",
			e.status,
			e.reason
		);
		assert!(!wrote[&k], "{bad}: a refused shape wrote binding text");
	}
	let st = |k: &str| {
		out.entries
			.iter()
			.find(|e| e.key == k)
			.map(|e| (e.status.to_string(), e.reason.clone().unwrap_or_default()))
			.unwrap()
	};
	assert_eq!(st(&format!("{keyed}:Eq")).0, "adapted");
	assert!(
		out.functions.contains("protocol = EQ") && out.functions.contains("this.0 == other.0"),
		"Eq installs EQ from ==:\n{}",
		out.functions
	);
	assert_eq!(st(&format!("{keyed}:Hash")).0, "generated");
	assert!(
		out.functions.contains("protocol = HASH")
			&& out
				.functions
				.contains("core::hash::Hash::hash(&this.0, hasher)"),
		"{}",
		out.functions
	);
	let (s, r) = st(&format!("{loose}:Hash"));
	assert!(
		s == "unsupported" && r.starts_with("Hash without a proven Eq"),
		"{s} {r}"
	);
	let (s, r) = st(&format!("{keyed}:TrivialClone"));
	assert!(s == "adapted" && r.starts_with("marker:"), "{s} {r}");
	assert_eq!(st(&format!("{keyed}:PartialOrd")).0, "generated");
	assert!(out.functions.contains("protocol = PARTIAL_CMP"));
	let (s, r) = st(&format!("{keyed}:Flags"));
	assert!(
		s == "unsupported" && r.starts_with("bitflags trait impl"),
		"{s} {r}"
	);
	println!("protocols self-test: ok");
}

/// Record 0111 (review round 1): the exact shape each mapped protocol trait
/// has in the pinned inventories. `Ok` for traits this record does not map.
pub(crate) fn protocol_shape(c: &Callable, owner: &str, tname: &str) -> Result<(), String> {
	let path = c
		.canonical_path
		.split(" as ")
		.nth(1)
		.unwrap_or("")
		.split('<')
		.next()
		.unwrap_or("");
	let params: Vec<&str> = c.params.iter().map(|p| p.ty_canonical.as_str()).collect();
	let ret = c.ret_canonical.as_deref();
	let self_ref = |p: &str| p == "&Self" || p == format!("&{owner}");
	let generic_hasher = |p: &str| {
		p.strip_prefix("&mut ").is_some_and(|g| {
			let g = g.strip_prefix("__").unwrap_or(g);
			g.chars().next().is_some_and(|ch| ch.is_ascii_uppercase())
				&& g.chars().all(|ch| ch.is_ascii_alphanumeric())
		})
	};
	let (want_path, ok) = match tname {
		"TrivialClone" => (
			"core::clone::TrivialClone",
			c.receiver == "none" && params.is_empty() && ret.is_none(),
		),
		"Drop" => (
			"core::ops::drop::Drop",
			c.receiver == "&mut self" && params.is_empty() && ret.is_none(),
		),
		"Eq" => (
			"core::cmp::Eq",
			matches!(c.receiver.as_str(), "&self" | "none") && params.is_empty() && ret.is_none(),
		),
		"Hash" => (
			"core::hash::Hash",
			c.receiver == "&self"
				&& params.len() == 1
				&& generic_hasher(params[0])
				&& ret.is_none(),
		),
		"PartialEq" => (
			"core::cmp::PartialEq",
			c.receiver == "&self"
				&& params.len() == 1
				&& self_ref(params[0])
				&& ret == Some("bool"),
		),
		"PartialOrd" => (
			"core::cmp::PartialOrd",
			c.receiver == "&self"
				&& params.len() == 1
				&& self_ref(params[0])
				&& ret == Some("core::option::Option<core::cmp::Ordering>"),
		),
		"Ord" => (
			"core::cmp::Ord",
			c.receiver == "&self"
				&& params.len() == 1
				&& self_ref(params[0])
				&& ret == Some("core::cmp::Ordering"),
		),
		"FromStr" => (
			"core::str::traits::FromStr",
			c.receiver == "none"
				&& params == ["&str"]
				&& matches!(
					ret,
					Some("core::result::Result<Self, Self::Err>")
						| Some("polars_error::PolarsResult<Self>")
				),
		),
		_ => return Ok(()),
	};
	if path != want_path {
		return Err(format!("{tname}: trait path `{path}` is not `{want_path}`"));
	}
	if !ok {
		return Err(format!(
			"{tname}: receiver `{}`, params {params:?}, return {ret:?} is not the recorded shape",
			c.receiver
		));
	}
	Ok(())
}

/// Record 0111: a one-parameter trait method whose parameter is `&Self` or `&Owner`.
pub(crate) fn self_param(c: &Callable, owner: &str) -> bool {
	c.params.len() == 1
		&& (c.params[0].ty_canonical == format!("&{owner}") || c.params[0].ty_canonical == "&Self")
}

/// Record 0111: the owner has a recorded foreign impl of `trait_` (by short name, unbounded).
pub(crate) fn owner_has_trait(world: &World, owner: &str, trait_: &str) -> bool {
	world.impls.get(owner).is_some_and(|v| {
		v.iter()
			.any(|(short, _, bounds)| short == trait_ && bounds.iter().all(|(_, b)| b.is_empty()))
	})
}

/// Record 0078: assignment operators with their Rune protocols.
pub(crate) const ASSIGN_OPS: &[(&str, &str, &str, &str)] = &[
	("SubAssign", "SUB_ASSIGN", "-=", "sub_assign"),
	("BitAndAssign", "BIT_AND_ASSIGN", "&=", "bitand_assign"),
	("BitOrAssign", "BIT_OR_ASSIGN", "|=", "bitor_assign"),
	("BitXorAssign", "BIT_XOR_ASSIGN", "^=", "bitxor_assign"),
];

pub(crate) const OPS: &[(&str, &str, &str)] = &[
	("Add", "ADD", "+"),
	("Sub", "SUB", "-"),
	("Mul", "MUL", "*"),
	("Div", "DIV", "/"),
	("Rem", "REM", "%"),
	("BitAnd", "BIT_AND", "&"),
	("BitOr", "BIT_OR", "|"),
	("BitXor", "BIT_XOR", "^"),
];

pub(crate) fn emit_foreign(world: &World, out: &mut Emitted, c: &Callable) {
	let owner = &c.owner;
	let tname = c.name.split('<').next().unwrap_or("").to_string();
	if tname == "From" && !from_impl_is_on_its_owner(c) {
		// listed on its source type's page (wrapped or not); a duplicate only
		// when the retained listing on the impl's `for` type is identified
		// (same crate and impl id); its status is settled in
		// `resolve_duplicates` once every entry exists. Otherwise it is an
		// outward conversion `From<Owner> for Target` with no wrapped
		// constructor to bind.
		let target = c.impl_for.clone().unwrap_or_default();
		let counterpart = out
			.from_names
			.keys()
			.find(|k| impl_identity(k) == impl_identity(&c.key) && k.as_str() != c.key.as_str())
			.cloned();
		match counterpart {
			Some(k) => {
				out.adapted(c, &format!("duplicate listing of impl {}: rustdoc lists it on this source type's page as well; its retained listing is on {}", impl_identity(&c.key), target), "");
				out.entries.last_mut().unwrap().counterpart = Some(k);
			}
			None => out.unsupported(
				c,
				"outward conversion, no retained listing on its target",
				&format!(
					"From<{}> for {target}",
					owner.rsplit("::").next().unwrap_or("")
				),
			),
		}
		return;
	}
	let Some(w) = world.wrapper_for(owner) else {
		out.unsupported(c, "owner not wrapped", owner.clone().as_str());
		return;
	};
	// record 0111 (review round 1): every mapped trait is admitted only on its
	// exact recorded shape (trait path, receiver, parameters, return); a
	// drifted shape is a named refusal before any binding text is written
	if let Err(why) = protocol_shape(c, owner, &tname) {
		out.unsupported(c, "protocol shape", &why);
		return;
	}
	// record 0111: markers with no operation of their own, cited
	if tname == "TrivialClone" {
		out.adapted(c, "marker: core::clone::TrivialClone is an unstable optimization marker with no method; its Clone::clone must be equivalent to a copy and it is \"not part of any API guarantee\" (rustc 1.98.1 core/src/clone.rs:255-283), so Clone carries the operation", &format!("{} {tname}", rune_path(w)));
		return;
	}
	if tname == "Drop" {
		out.adapted(c, "marker: the destructor runs when the owning Rune value is dropped, exactly once, moved or not (tests/protocols.rs drop control); there is no script-callable drop", &format!("{} {tname}", rune_path(w)));
		return;
	}
	// record 0111: a proven Rust `Eq` installs Rune's EQ from the same `==`
	// (Rune maps compare keys with EQ, rune 0.14.2 hashbrown/table.rs:287-335);
	// the row stays a marker, the callable is equality itself
	if tname == "Eq" && !w.hand {
		let key = (w.rust.clone(), "<EQ>".to_string());
		if !out.taken.contains_key(&key) {
			let idx = out.fn_index;
			let ident = rust_ident("p", &c.canonical_path, idx);
			out.fn_index += 1;
			writeln!(out.functions, "/// Polars: `{}` (record 0111: EQ from the Rust `==`, Eq proven).\n#[rune::function(instance, protocol = EQ)]\nfn {ident}(this: &{1}, other: &{1}) -> bool {{ this.0 == other.0 }}", c.canonical_path, w.rust).unwrap();
			out.registrations
				.push(format!("m.function_meta({ident})?;"));
			out.taken.insert(key, c.canonical_path.clone());
		}
		out.adapted(
			c,
			"marker: total equality; installs Rune EQ from the Rust ==",
			&format!("{} Eq (EQ)", rune_path(w)),
		);
		return;
	}
	if matches!(tname.as_str(), "Eq" | "StructuralPartialEq" | "Copy") {
		out.adapted(
			c,
			"implied by the PartialEq or Clone protocol",
			&format!("{} {tname}", rune_path(w)),
		);
		return;
	}
	if HAND_PROTOCOLS
		.iter()
		.any(|(o, t)| *o == owner && *t == tname)
	{
		out.adapted(
			c,
			"hand-written protocol",
			&format!("{} {tname}", rune_path(w)),
		);
		return;
	}
	let key = (w.rust.clone(), format!("<{tname}>"));
	if let Some(prev) = out.taken.get(&key) {
		out.unsupported(c, "protocol taken on this type by", prev.clone().as_str());
		return;
	}
	let idx = out.fn_index;
	let ident = rust_ident("p", &c.canonical_path, idx);
	if tname == "From" {
		let name = out
			.from_names
			.get(&c.key)
			.cloned()
			.unwrap_or_else(|| "from".into());
		emit_from(world, out, c, &name);
		return;
	}
	let (code, note) = match tname.as_str() {
		t if ASSIGN_OPS.iter().any(|(n, _, _, _)| *n == t) => {
			let (_, proto, op, method) = ASSIGN_OPS.iter().find(|(n, _, _, _)| *n == t).unwrap();
			let rhs_ok = c
				.params
				.first()
				.is_some_and(|p| p.ty_canonical == *owner || p.ty_canonical == "Self");
			if !rhs_ok {
				out.unsupported(
					c,
					"assignment operand",
					&c.params
						.first()
						.map(|p| p.ty_canonical.clone())
						.unwrap_or_default(),
				);
				return;
			}
			if !world.clonable.contains(owner) {
				out.unsupported(c, "operator on a non-Clone type", tname.as_str());
				return;
			}
			// `x op= y` mutates the Rune value `x` in place; `y` is cloned out and stays usable
			(
				format!(
					"#[rune::function(instance, protocol = {proto})]\nfn {ident}(this: &mut {0}, rhs: &{0}) {{ <{1} as core::ops::{t}>::{method}(&mut this.0, rhs.0.clone()) }}",
					w.rust, w.spell
				),
				*op,
			)
		}
		"Not" => {
			if !world.clonable.contains(owner) {
				out.unsupported(c, "operator on a non-Clone type", tname.as_str());
				return;
			}
			// Rune 0.14.2 has no unary NOT protocol: bound as a method, the receiver unchanged
			(
				format!(
					"#[rune::function(instance, path = not_)]\nfn {ident}(this: &{0}) -> {0} {{ {0}(<{1} as core::ops::Not>::not(this.0.clone())) }}",
					w.rust, w.spell
				),
				"not_() (Rune has no unary NOT protocol; bound as a method, `not` is a keyword)",
			)
		}
		"Display" => (
			format!(
				"#[rune::function(instance, protocol = DISPLAY_FMT)]\nfn {ident}(this: &{0}, f: &mut rune::runtime::Formatter) -> rune::runtime::VmResult<()> {{ use rune::alloc::fmt::TryWrite; let s = format!(\"{{}}\", this.0); rune::vm_write!(f, \"{{s}}\") }}",
				w.rust
			),
			"DISPLAY_FMT",
		),
		"Debug" => (
			format!(
				"#[rune::function(instance, protocol = DEBUG_FMT)]\nfn {ident}(this: &{0}, f: &mut rune::runtime::Formatter) -> rune::runtime::VmResult<()> {{ use rune::alloc::fmt::TryWrite; let s = format!(\"{{:?}}\", this.0); rune::vm_write!(f, \"{{s}}\") }}",
				w.rust
			),
			"DEBUG_FMT",
		),
		"PartialEq" if self_param(c, owner) => (
			format!(
				"#[rune::function(instance, protocol = PARTIAL_EQ)]\nfn {ident}(this: &{0}, other: &{0}) -> bool {{ this.0 == other.0 }}",
				w.rust
			),
			"PARTIAL_EQ",
		),
		"Clone" => (
			format!(
				"#[rune::function(instance, protocol = CLONE)]\nfn {ident}(this: &{0}) -> {0} {{ {0}(this.0.clone()) }}",
				w.rust
			),
			"CLONE",
		),
		"Default" => (
			format!(
				"#[rune::function(free, path = {0}::default_)]\nfn {ident}() -> {0} {{ {0}(<{1}>::default()) }}",
				w.rust, w.spell
			),
			"default_() (renamed: `default` is a Rune keyword)",
		),
		t if OPS.iter().any(|(n, _, _)| *n == t) => {
			let (_, proto, op) = OPS.iter().find(|(n, _, _)| *n == t).unwrap();
			let Some(rhs) = c.params.first() else {
				out.unsupported(c, "operator without rhs", tname.as_str());
				return;
			};
			let a = match world
				.arg(
					&ty::parse(&rhs.ty_canonical),
					"rhs",
					&BTreeMap::new(),
					Some(owner),
					0,
				)
				.and_then(|a| {
					if a.pre.is_empty() {
						Ok(a)
					} else {
						Err(Unsupported("operand needs a temporary", String::new()))
					}
				}) {
				Ok(a) => a,
				Err(Unsupported(why, what)) => {
					out.unsupported(c, why, &format!("rhs ({what})"));
					return;
				}
			};
			let ret = match c
				.ret_canonical
				.as_deref()
				.map(|r| world.ret(&ty::parse(r), Some(owner), 0))
			{
				Some(Ok(r)) => r,
				Some(Err(Unsupported(why, what))) => {
					out.unsupported(c, why, &format!("return ({what})"));
					return;
				}
				None => {
					out.unsupported(c, "operator without output", tname.as_str());
					return;
				}
			};
			let fallible = ret.fallible || a.fallible;
			let ret_ty = if fallible {
				format!("Result<{}, Error>", ret.rust_ty)
			} else {
				ret.rust_ty.clone()
			};
			let body = if fallible {
				format!("Ok({})", ret.conv)
			} else {
				ret.conv.clone()
			}
			.replace("__OP__", &c.name);
			(
				format!(
					"#[rune::function(instance, protocol = {proto})]\nfn {ident}(this: &{0}, rhs: {1}) -> {ret_ty} {{ let __r = this.0.clone() {op} {2}; {body} }}",
					w.rust, a.rust_ty, a.conv
				),
				*proto,
			)
		}
		_ if !world.clonable.contains(owner) && OPS.iter().any(|(n, _, _)| *n == tname) => {
			out.unsupported(c, "operator on a non-Clone type", tname.as_str());
			return;
		}
		"Neg" if !world.clonable.contains(owner) => {
			out.unsupported(c, "operator on a non-Clone type", tname.as_str());
			return;
		}
		// record 0113: Rune 0.14.2 has no unary-minus protocol (rune-core
		// src/protocol.rs lists none): bound as a method, like `not_`
		"Neg" => (
			format!(
				"#[rune::function(instance, path = neg)]\nfn {ident}(this: &{0}) -> {0} {{ {0}(-this.0.clone()) }}",
				w.rust
			),
			"neg() (Rune has no unary-minus protocol; bound as a method)",
		),
		"Hash" if w.hand => {
			out.unsupported(
				c,
				"Hash on a hand-written wrapper",
				"its EQ is not generated",
			);
			return;
		}
		"Hash" if !owner_has_trait(world, owner, "Eq") => {
			out.unsupported(
				c,
				"Hash without a proven Eq",
				"Rune map keys compare with EQ; a hashable key without total equality is refused",
			);
			return;
		}
		"Hash" => (
			format!(
				"#[rune::function(instance, protocol = HASH)]\nfn {ident}(this: &{0}, hasher: &mut rune::runtime::Hasher) {{ core::hash::Hash::hash(&this.0, hasher) }}",
				w.rust
			),
			"HASH (with EQ from the proven Eq)",
		),
		"PartialOrd" if self_param(c, owner) => (
			format!(
				"#[rune::function(instance, protocol = PARTIAL_CMP)]\nfn {ident}(this: &{0}, other: &{0}) -> Option<core::cmp::Ordering> {{ core::cmp::PartialOrd::partial_cmp(&this.0, &other.0) }}",
				w.rust
			),
			"PARTIAL_CMP",
		),
		"Ord" if self_param(c, owner) => (
			format!(
				"#[rune::function(instance, protocol = CMP)]\nfn {ident}(this: &{0}, other: &{0}) -> core::cmp::Ordering {{ core::cmp::Ord::cmp(&this.0, &other.0) }}",
				w.rust
			),
			"CMP",
		),
		"FromStr" if c.params.len() == 1 && c.params[0].ty_canonical == "&str" => (
			format!(
				"#[rune::function(free, path = {0}::parse)]\nfn {ident}(s: &str) -> Result<{0}, Error> {{ <{1} as core::str::FromStr>::from_str(s).map({0}).map_err(|e| Error::conversion(&format!(\"parse: {{e:?}}\"))) }}",
				w.rust, w.spell
			),
			"parse(s) (FromStr; a failed parse is a catchable conversion error)",
		),
		"Flags" | "PublicFlags" => {
			out.unsupported(c, "bitflags trait impl", "its operations are the type's inherent bitflags methods, bound separately; the trait adds Bits-typed internals with no script value");
			return;
		}
		"Write" => {
			out.unsupported(c, "mutable I/O trait", "Write needs a byte-buffer input, a write count and I/O error routing that no binding boundary defines");
			return;
		}
		"TryFrom" => {
			out.unsupported(
				c,
				"conversion source without a script value",
				tname.as_str(),
			);
			return;
		}
		_ => {
			out.unsupported(
				c,
				"trait impl not mapped to a Rune protocol",
				tname.as_str(),
			);
			return;
		}
	};
	out.fn_index += 1;
	writeln!(out.functions, "/// Polars: `{}`.\n{code}", c.canonical_path).unwrap();
	out.registrations
		.push(format!("m.function_meta({ident})?;"));
	out.taken.insert(key, c.canonical_path.clone());
	let rune = format!("{} {note}", rune_path(w));
	let assign = ASSIGN_OPS.iter().find(|(n, _, _, _)| *n == tname);
	let info = OracleInfo {
		rune_owner: Some(rune_path(w)),
		rune_name: match assign {
			Some((_, _, op, _)) => op.to_string(),
			None if tname == "Not" => "not_".to_string(),
			None => format!("<{tname}>"),
		},
		receiver: match assign {
			Some(_) => "&mut self".into(),
			None if tname == "Not" => "self".into(),
			None => "protocol".into(),
		},
		owner: Some((owner.to_string(), w.rust.clone())),
		callee: match assign {
			Some((_, _, _, method)) => format!("<{} as core::ops::{tname}>::{method}", w.spell),
			None if tname == "Not" => format!("<{} as core::ops::Not>::not", w.spell),
			None => tname.clone(),
		},
		params: match assign {
			Some(_) => vec![(format!("W:{owner}"), owner.to_string())],
			None => c
				.params
				.iter()
				.map(|p| (String::new(), p.ty_canonical.clone()))
				.collect(),
		},
		param_names: c.params.iter().map(|p| sanitize(&p.name)).collect(),
		ret_canonical: if assign.is_some() {
			None
		} else {
			c.ret_canonical.clone()
		},
		ret_rust: if assign.is_some() {
			"()".into()
		} else {
			String::new()
		},
		fallible: false,
		generics: BTreeMap::new(),
		implementors: vec![],
		deref: false,
	};
	out.generated_with(c, &rune, Some(format!("derived: {}", c.derived)), info);
}

#[cfg(test)]
mod tests {
	#[test]
	fn protocols() {
		super::protocols_self_test();
	}
}
