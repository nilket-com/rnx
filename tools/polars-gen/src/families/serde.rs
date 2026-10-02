use crate::emit::callable::emit_callable;
use crate::emit::{Emitted, OracleInfo, rune_path, rust_ident};
use crate::model::{Callable, Inventory, Param, Supporting};
use crate::release::{FamilyTables, InstantiationScope, Release, ReleaseProvenance};
use crate::ty::last;
use crate::world::World;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

/// Record 0112: the serde lane. The derive shape and the hand-written fully
/// qualified shape bind `to_json`/`from_json` on a concrete owner; a drifted
/// return, an extra parameter, a generic head, a wrong receiver or another
/// trait path is refused by name with no binding text.
pub(crate) fn serde_self_test() {
	let owner = "polars_core::datatypes::TimeUnit";
	let sup = Supporting {
		trait_params: vec![],
		key: owner.to_string(),
		kind: "enum".into(),
		canonical_path: owner.to_string(),
		found_paths: vec!["polars::TimeUnit".into()],
		crate_paths: vec![owner.to_string()],
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
	let imp =
		|key: &str, path: &str, receiver: &str, params: Vec<&str>, ret: &str, impl_for: &str| {
			Callable {
				key: key.into(),
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
				ret_canonical: Some(ret.into()),
				generics_canonical: vec![],
				impl_for: Some(impl_for.into()),
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
				bucket: "generic".into(),
				rules: vec![],
				trait_lifetimes: vec![],
			}
		};
	let ser = "serde_core::ser::Serialize";
	let de = "serde_core::de::Deserialize";
	let calls = vec![
		imp(
			"ser",
			ser,
			"&self",
			vec!["__S"],
			"core::result::Result<__S::Ok, __S::Error>",
			owner,
		),
		{
			let mut c = imp(
				"de",
				de,
				"none",
				vec!["__D"],
				"core::result::Result<Self, __D::Error>",
				owner,
			);
			c.trait_lifetimes = vec!["'de".into()];
			c
		},
		// review of 0112: a borrowing `Deserialize<'static>` is not DeserializeOwned
		{
			let mut c = imp(
				"bad_static",
				de,
				"none",
				vec!["__D"],
				"core::result::Result<Self, __D::Error>",
				owner,
			);
			c.trait_lifetimes = vec!["'static".into()];
			c
		},
		imp(
			"bad_ret",
			ser,
			"&self",
			vec!["__S"],
			"core::result::Result<(), __S::Error>",
			owner,
		),
		imp(
			"bad_params",
			ser,
			"&self",
			vec!["__S", "u8"],
			"core::result::Result<__S::Ok, __S::Error>",
			owner,
		),
		imp(
			"bad_head",
			de,
			"none",
			vec!["D"],
			"core::result::Result<Self, D::Error>",
			"polars_core::datatypes::TimeUnit<T>",
		),
		imp(
			"bad_recv",
			de,
			"&self",
			vec!["D"],
			"core::result::Result<Self, D::Error>",
			owner,
		),
		imp(
			"bad_path",
			"serde::ser::Serialize",
			"&self",
			vec!["S"],
			"core::result::Result<S::Ok, S::Error>",
			owner,
		),
	];
	// review of 0112: an owner whose public field reaches a frame, and a
	// struct with no public field, are IPC-bearing or unknown: refused
	let holder = "polars_core::datatypes::Holder";
	let opaque = "polars_core::datatypes::Opaque";
	let frame = "polars_core::frame::dataframe::DataFrame";
	let mk_sup = |path: &str, kind: &str, fields: Vec<(&str, &str)>, public: usize| Supporting {
		trait_params: vec![],
		key: path.to_string(),
		kind: kind.into(),
		canonical_path: path.to_string(),
		found_paths: vec![format!("polars::{}", path.rsplit("::").next().unwrap())],
		crate_paths: vec![path.to_string()],
		public_fields: public,
		fields_canonical: fields
			.into_iter()
			.map(|(a, b)| (a.to_string(), b.to_string()))
			.collect(),
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
	let on = |key: &str, o: &str| {
		let mut c = imp(
			key,
			ser,
			"&self",
			vec!["__S"],
			"core::result::Result<__S::Ok, __S::Error>",
			o,
		);
		c.owner = o.into();
		c.canonical_path = format!("{o} as {ser}");
		c
	};
	let mut calls = calls;
	calls.push(on("holder_ser", holder));
	calls.push(on("opaque_ser", opaque));
	// round 2: a nested opaque struct (AsOfOptions -> Option<Scalar>) and a
	// cited frame-free leaf (a PlSmallStr field)
	let nested = "polars_core::datatypes::AsOfLike";
	let named = "polars_core::datatypes::Named";
	calls.push(on("nested_ser", nested));
	calls.push({
		let mut c = imp(
			"nested_de",
			de,
			"none",
			vec!["__D"],
			"core::result::Result<Self, __D::Error>",
			nested,
		);
		c.owner = nested.into();
		c.canonical_path = format!("{nested} as {de}");
		c.trait_lifetimes = vec!["'de".into()];
		c
	});
	calls.push(on("named_ser", named));
	// round 3: a tuple struct `Stats(pub Arc<DataFrame>)`: one public field, none recorded
	let tuple = "polars_core::datatypes::Stats";
	calls.push(on("tuple_ser", tuple));
	let inv = Inventory {
		callables: calls.clone(),
		supporting: vec![
			sup,
			mk_sup(
				holder,
				"struct",
				vec![("df", &format!("alloc::sync::Arc<{frame}>"))],
				1,
			),
			mk_sup(opaque, "struct", vec![], 0),
			mk_sup(frame, "struct", vec![], 0),
			mk_sup(
				nested,
				"struct",
				vec![("tolerance", &format!("core::option::Option<{opaque}>"))],
				1,
			),
			mk_sup(
				named,
				"struct",
				vec![("name", "polars_utils::pl_str::PlSmallStr")],
				1,
			),
			mk_sup("polars_utils::pl_str::PlSmallStr", "struct", vec![], 0),
			mk_sup(tuple, "struct", vec![], 1),
		],
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
		emit_callable(&world, &mut out, c, &["mechanical"]);
		wrote.insert(
			c.key.clone(),
			(out.functions.len(), out.registrations.len()) != before,
		);
	}
	let st = |k: &str| {
		out.entries
			.iter()
			.find(|e| e.key == k)
			.map(|e| (e.status.to_string(), e.reason.clone().unwrap_or_default()))
			.unwrap()
	};
	assert_eq!(st("ser").0, "generated", "{:?}", st("ser"));
	assert_eq!(st("de").0, "generated", "{:?}", st("de"));
	assert!(
		out.functions.contains("path = to_json")
			&& out.functions.contains("support::to_json(&this.0"),
		"{}",
		out.functions
	);
	assert!(
		out.functions.contains("::from_json)") && out.functions.contains("support::json_len_ok(s,"),
		"{}",
		out.functions
	);
	// the hand-written fully qualified spelling is the same shape
	let mut q = calls[0].clone();
	q.key = "qualified".into();
	q.ret_canonical =
		Some("core::result::Result<<S as Serializer>::Ok, <S as Serializer>::Error>".into());
	q.params[0].ty_canonical = "S".into();
	assert!(serde_shape(&q, "Serialize").is_ok());
	for bad in [
		"bad_ret",
		"bad_params",
		"bad_head",
		"bad_recv",
		"bad_static",
	] {
		let (s, r) = st(bad);
		assert!(
			s == "unsupported" && r.starts_with("serde shape"),
			"{bad}: {s} {r}"
		);
		assert!(!wrote[bad], "{bad}: a refused shape wrote binding text");
	}
	// another trait path is not the serde lane at all: never emitted as JSON
	assert!(
		serde_trait(calls.iter().find(|c| c.key == "bad_path").unwrap()).is_none()
			&& !wrote["bad_path"]
	);
	for k in ["holder_ser", "opaque_ser"] {
		let (s, r) = st(k);
		assert!(
			s == "unsupported" && r.starts_with("serde through an unbounded IPC buffer"),
			"{k}: {s} {r}"
		);
		assert!(!wrote[k], "{k}: an IPC-bearing owner wrote binding text");
	}
	for k in ["nested_ser", "nested_de"] {
		let (s, r) = st(k);
		assert!(
			s == "unsupported" && r.contains("no public field"),
			"{k}: a nested opaque struct must refuse: {s} {r}"
		);
		assert!(!wrote[k], "{k}: wrote binding text");
	}
	assert_eq!(
		st("named_ser").0,
		"generated",
		"a cited frame-free leaf binds: {:?}",
		st("named_ser")
	);
	let (s, r) = st("tuple_ser");
	assert!(
		s == "unsupported" && r.contains("tuple fields are not extracted"),
		"tuple_ser: {s} {r}"
	);
	assert!(!wrote["tuple_ser"], "tuple_ser: wrote binding text");
	println!("serde self-test: ok");
}

/// Record 0112: `Some("Serialize" | "Deserialize")` for a serde trait impl,
/// by its canonical trait path (the pinned serde splits its traits into
/// `serde_core`).
pub(crate) fn serde_trait(c: &Callable) -> Option<&'static str> {
	match c
		.canonical_path
		.split(" as ")
		.nth(1)
		.unwrap_or("")
		.split('<')
		.next()
		.unwrap_or("")
	{
		"serde_core::ser::Serialize" => Some("Serialize"),
		"serde_core::de::Deserialize" => Some("Deserialize"),
		_ => None,
	}
}

/// Record 0112: the exact serde shape the pinned inventories record, on a
/// concrete head: `Serialize::serialize(&self, S) -> Result<S::Ok, S::Error>`
/// and `Deserialize::deserialize(D) -> Result<Self, D::Error>`, `S`/`D` a
/// generic parameter; `impl_for` is the owner itself with no bound.
pub(crate) fn serde_shape(c: &Callable, tr: &str) -> Result<(), String> {
	let params: Vec<&str> = c.params.iter().map(|p| p.ty_canonical.as_str()).collect();
	let generic = |g: &str| {
		let g = g.strip_prefix("__").unwrap_or(g);
		g.chars().next().is_some_and(|ch| ch.is_ascii_uppercase())
			&& g.chars().all(|ch| ch.is_ascii_alphanumeric())
	};
	let ret = c.ret_canonical.as_deref().unwrap_or("");
	let ok = match tr {
		"Serialize" => {
			c.receiver == "&self"
				&& params.len() == 1
				&& generic(params[0])
				// the derive spells `S::Ok`; a hand-written impl (Series) the fully
				// qualified `<S as Serializer>::Ok`: the same projection, nothing else
				&& (ret == format!("core::result::Result<{0}::Ok, {0}::Error>", params[0])
					|| ret == format!("core::result::Result<<{0} as Serializer>::Ok, <{0} as Serializer>::Error>", params[0]))
		}
		_ => {
			c.receiver == "none"
				&& params.len() == 1
				&& generic(params[0])
				&& (ret == format!("core::result::Result<Self, {}::Error>", params[0])
					|| ret
						== format!(
							"core::result::Result<Self, <{} as Deserializer>::Error>",
							params[0]
						))
		}
	};
	// review of 0112: a `Deserialize<'static>` borrows static data and is not
	// `DeserializeOwned`, so it cannot parse a script's text; the owned form is
	// `impl<'de> Deserialize<'de>`, one generic lifetime argument
	let lifetimes_ok = match tr {
		"Serialize" => c.trait_lifetimes.is_empty(),
		_ => c.trait_lifetimes.len() == 1 && c.trait_lifetimes[0] != "'static",
	};
	if !lifetimes_ok {
		return Err(format!(
			"{tr}: trait lifetime arguments {:?} are not the owned form",
			c.trait_lifetimes
		));
	}
	if !ok {
		return Err(format!(
			"{tr}: receiver `{}`, params {params:?}, return `{ret}` is not the recorded shape",
			c.receiver
		));
	}
	if c.owner_generic
		|| c.impl_for.as_deref() != Some(c.owner.as_str())
		|| !c.impl_bounds.is_empty()
		|| !c.impl_where.is_empty()
	{
		return Err(format!(
			"{tr}: impl on `{}` with bounds {:?} is not a concrete head of the owner",
			c.impl_for.as_deref().unwrap_or("?"),
			c.impl_bounds
		));
	}
	Ok(())
}

/// Record 0112 (review round 2): opaque structs (no public field) whose
/// private fields were read in the pinned sources at both pins and hold only
/// primitives, strings, or (for `Schema`) its own type arguments, which the
/// walk visits; every other opaque struct stays refused (for example
/// `Breaks(Series)`, and `Categories`, since `DataType` serializes an `Enum`'s
/// categories as a `Series`: polars-core src/datatypes/_serde.rs:222-226).
pub(crate) const FRAME_FREE_OPAQUE: &[(&str, &str)] = &[
	(
		"polars_utils::pl_str::PlSmallStr",
		"PlSmallStr(Inner), Inner = compact_str::CompactString: polars-utils src/pl_str.rs:17,26 (da47b74 and 0.55.2)",
	),
	(
		"polars_utils::pl_ref_str::PlRefStr",
		"PlRefStr(Inner), Inner = Arc<str>: polars-utils src/pl_ref_str.rs:18,23 (both pins)",
	),
	(
		"polars_utils::pl_path::PlRefPath",
		"PlRefPath { inner: PlRefStr }: polars-utils src/pl_path.rs:88-90 (both pins)",
	),
	(
		"polars_core::datatypes::temporal::time_zone::TimeZone",
		"TimeZone { inner: PlSmallStr }: polars-core src/datatypes/temporal/time_zone.rs:9-12 (both pins)",
	),
	(
		"polars_utils::compression::GzipLevel",
		"GzipLevel(u8): polars-utils src/compression.rs:59 (both pins)",
	),
	(
		"polars_utils::compression::ZstdLevel",
		"ZstdLevel(i32): polars-utils src/compression.rs:92 (both pins)",
	),
	(
		"polars_utils::bool::UnsafeBool",
		"UnsafeBool(bool): polars-utils src/bool.rs:7 (both pins)",
	),
	(
		"polars_core::chunked_array::ops::binning::Fractions",
		"Fractions(Vec<f64>): polars-core src/chunked_array/ops/binning.rs:47 (da47b74)",
	),
	(
		"polars_core::datatypes::dtype::CompatLevel",
		"CompatLevel(pub(crate) u16): polars-core src/datatypes/dtype.rs:1774 (da47b74), :1425 (0.55.2)",
	),
	(
		"polars_core::datatypes::ListType",
		"ListType {}: polars-core src/datatypes/mod.rs:249 (da47b74), :247 (0.55.2)",
	),
	(
		"polars_utils::compression::BrotliLevel",
		"BrotliLevel(u32): polars-utils src/compression.rs:28 (both pins)",
	),
	(
		"polars_core::chunked_array::flags::StatisticsFlags",
		"bitflags! StatisticsFlags: u32: polars-core src/chunked_array/flags.rs:11-14 (both pins)",
	),
	// a container: its keys are PlSmallStr and its values/metadata are its type
	// arguments, which the walk visits on their own
	(
		"polars_schema::schema::Schema",
		"Schema<Field, Metadata> { fields: PlIndexMap<PlSmallStr, Field>, metadata: Metadata }: polars-schema src/schema.rs:13-16 (both pins)",
	),
];

/// Record 0112 (review round 1): `Some(reason)` when a serde value of
/// `owner` may carry a frame or series (serialized through an in-memory IPC
/// buffer, deserialized from a possibly compressed IPC payload): the buffered
/// types themselves, any type whose public fields or enum payloads reach one
/// (recursively, over the inventory's supporting records), and any struct
/// with no public field, whose content the inventory cannot show.
pub(crate) fn serde_ipc_bearing(world: &World, owner: &str) -> Option<String> {
	const BUFFERED: &[&str] = &[
		"polars_core::frame::dataframe::DataFrame",
		"polars_core::series::Series",
		"polars_core::frame::column::Column",
	];
	fn paths(t: &str) -> Vec<String> {
		let mut out = Vec::new();
		let mut cur = String::new();
		for ch in t.chars() {
			if ch.is_ascii_alphanumeric() || ch == '_' || ch == ':' {
				cur.push(ch);
			} else {
				if cur.contains("::") {
					out.push(cur.trim_matches(':').to_string());
				}
				cur.clear();
			}
		}
		if cur.contains("::") {
			out.push(cur.trim_matches(':').to_string());
		}
		out
	}
	fn walk(
		world: &World,
		p: &str,
		seen: &mut BTreeSet<String>,
		trail: &mut Vec<String>,
	) -> Option<String> {
		if BUFFERED.contains(&p) {
			return Some(format!("{} reaches {p}", trail.join(" -> ")));
		}
		if !seen.insert(p.to_string()) {
			return None;
		}
		// review of 0112 (round 2): an unresolved Polars path is unknown, not
		// safe; only a non-Polars leaf (std, core, alloc, a foreign crate) is
		// taken as frame-free, and its generic arguments are walked as paths
		let Some(s) = world.types.get(p) else {
			return if p.starts_with("polars") {
				Some(format!(
					"{} reaches {p}, which has no supporting record",
					trail.join(" -> ")
				))
			} else {
				None
			};
		};
		// review of 0112 (round 2): an opaque leaf whose private fields were
		// read at both pins and hold no frame, series or column
		if FRAME_FREE_OPAQUE.iter().any(|(q, _)| *q == p) {
			return None;
		}
		// review of 0112 (round 3): the inventory records only named fields, so a
		// tuple struct's public fields (`TableStatistics(pub Arc<DataFrame>)`) are
		// missing; more public fields than recorded ones is unknown content
		if s.kind == "struct" && s.public_fields > s.fields_canonical.len() {
			return Some(format!(
				"{} reaches {p}: {} public field(s), {} recorded (tuple fields are not extracted), its content cannot be shown to be frame-free",
				trail.join(" -> "),
				s.public_fields,
				s.fields_canonical.len()
			));
		}
		// a struct with no public field is opaque at any depth, not only at the root
		if s.kind == "struct" && s.public_fields == 0 {
			return Some(format!(
				"{} reaches {p}: no public field, its content cannot be shown to be frame-free",
				trail.join(" -> ")
			));
		}
		let mut kids: Vec<String> = s
			.fields_canonical
			.iter()
			.flat_map(|(_, f)| paths(f))
			.collect();
		kids.extend(
			s.variant_payloads
				.iter()
				.flat_map(|(_, ts)| ts.iter().flat_map(|x| paths(x))),
		);
		if let Some(target) = &s.alias_target {
			kids.extend(paths(target));
		}
		for k in kids {
			trail.push(last(&k).to_string());
			let r = walk(world, &k, seen, trail);
			trail.pop();
			if r.is_some() {
				return r;
			}
		}
		None
	}
	walk(
		world,
		owner,
		&mut BTreeSet::new(),
		&mut vec![last(owner).to_string()],
	)
}

/// Record 0112: `value.to_json()` and `Type::from_json(s)` through serde_json,
/// under the JSON byte bound (`support::to_json`, `json_len_ok`), inside the
/// engine boundary.
pub(crate) fn emit_serde(world: &World, out: &mut Emitted, c: &Callable) {
	let tr = serde_trait(c).unwrap();
	if let Err(why) = serde_shape(c, tr) {
		out.unsupported(c, "serde shape", &why);
		return;
	}
	let owner = &c.owner;
	let Some(w) = world.wrapper_for(owner) else {
		out.unsupported(c, "owner not wrapped", owner.clone().as_str());
		return;
	};
	// review of 0112: Polars serializes a frame or series by encoding IPC into
	// an in-memory buffer first (polars-core serde/df.rs:158-170,
	// serde/series.rs:39-51 at both pins), and a deserialized IPC payload may
	// be compressed; neither is bounded by the JSON text. Such owners, and
	// owners whose recorded structure reaches one or cannot be shown not to,
	// are refused in both directions so the byte bound holds for every binding
	if let Some(why) = serde_ipc_bearing(world, owner) {
		out.unsupported(c, "serde through an unbounded IPC buffer", &why);
		return;
	}
	let name = if tr == "Serialize" {
		"to_json"
	} else {
		"from_json"
	};
	let key = (w.rust.clone(), name.to_string());
	if let Some(prev) = out.taken.get(&key) {
		out.unsupported(c, "name taken on this type by", prev.clone().as_str());
		return;
	}
	let idx = out.fn_index;
	let ident = rust_ident("s", &c.canonical_path, idx);
	out.fn_index += 1;
	let op = format!("{}::{name}", last(owner));
	let code = if tr == "Serialize" {
		format!(
			"#[rune::function(instance, path = to_json)]\nfn {ident}(this: &{0}) -> Result<String, Error> {{ crate::engine::run(\"{op}\", || support::to_json(&this.0, \"{op}\")).map_err(Error::engine)? }}",
			w.rust
		)
	} else {
		format!(
			"#[rune::function(free, path = {0}::from_json)]\nfn {ident}(s: &str) -> Result<{0}, Error> {{ support::json_len_ok(s, \"{op}\")?; let __v = crate::engine::run(\"{op}\", || support::from_json::<{1}>(s, \"{op}\")).map_err(Error::engine)??; Ok({0}(__v)) }}",
			w.rust, w.spell
		)
	};
	writeln!(
		out.functions,
		"/// Polars: `{}` (record 0112: serde through serde_json, JSON-byte bounded).\n{code}",
		c.canonical_path
	)
	.unwrap();
	out.registrations
		.push(format!("m.function_meta({ident})?;"));
	out.taken.insert(key, c.canonical_path.clone());
	let rune = format!("{} {name}", rune_path(w));
	let info = OracleInfo {
		rune_owner: Some(rune_path(w)),
		rune_name: name.to_string(),
		receiver: "protocol".into(),
		owner: Some((owner.to_string(), w.rust.clone())),
		callee: tr.to_string(),
		params: vec![],
		param_names: vec![],
		ret_canonical: None,
		ret_rust: "String".into(),
		fallible: true,
		generics: BTreeMap::new(),
		implementors: vec![],
		deref: false,
	};
	out.generated_with(
		c,
		&rune,
		Some("record 0112: JSON through serde_json, byte-bounded".into()),
		info,
	);
}

#[cfg(test)]
mod tests {
	#[test]
	fn serde() {
		super::serde_self_test();
	}
}
