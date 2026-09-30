//! Record 0119: frozen bindings. A release may name a list of every binding
//! a previous record generated, as (entry key, binding id, Rune path); the
//! new surface must still generate each of them unchanged, and no two
//! generated bindings may share a Rune path. Anything else refuses
//! generation, the bindings named, so admitting a crate can never move an
//! existing script-visible path.
use crate::emit::Entry;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Frozen {
	pub(crate) key: String,
	pub(crate) id: String,
	pub(crate) rune: String,
	/// The binding's catalogue summary (its argument and return shapes), when
	/// it has one; protocol bindings name their protocol in the path instead.
	#[serde(default)]
	pub(crate) summary: Option<String>,
}

/// Record 0123: a frozen binding whose catalogue contract widens, listed
/// exactly (its Rune path, the frozen summary and the new one).
#[derive(serde::Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct Widening {
	pub(crate) rune: String,
	pub(crate) old: String,
	pub(crate) new: String,
	pub(crate) cite: String,
}

/// The problems, empty when every frozen binding survives and no Rune path
/// is generated twice. A contract change is accepted only when a listed
/// widening names it exactly; a listed widening that is not used is itself
/// a problem.
pub(crate) fn check(
	frozen: &[Frozen],
	entries: &[Entry],
	catalogue: &[(String, String)],
	widenings: &[Widening],
) -> Vec<String> {
	let mut used = std::collections::BTreeSet::new();
	let summaries: BTreeMap<&str, &str> = catalogue
		.iter()
		.map(|(r, s)| (r.as_str(), s.as_str()))
		.collect();
	let mut now: BTreeSet<(String, String, String)> = BTreeSet::new();
	let mut paths: BTreeMap<String, Vec<String>> = BTreeMap::new();
	for e in entries.iter().filter(|e| e.status == "generated") {
		for b in &e.bindings {
			now.insert((e.key.clone(), b.id.clone(), b.rune.clone()));
			paths.entry(b.rune.clone()).or_default().push(b.id.clone());
		}
	}
	let mut problems = Vec::new();
	for f in frozen {
		if !now.contains(&(f.key.clone(), f.id.clone(), f.rune.clone())) {
			problems.push(format!(
				"frozen binding {} ({}) moved or disappeared",
				f.id, f.rune
			));
		} else if let Some(s) = &f.summary {
			// the same path must keep the same contract
			let now = summaries.get(f.rune.as_str()).copied();
			let widened = widenings.iter().position(|w| {
				w.rune == f.rune
					&& w.old == *s && Some(w.new.as_str()) == now
					&& !w.cite.trim().is_empty()
			});
			if let Some(i) = widened {
				used.insert(i);
			} else if now != Some(s.as_str()) {
				problems.push(format!(
					"frozen binding {} ({}) changed its contract: `{s}` is now `{}`",
					f.id,
					f.rune,
					now.unwrap_or("(no summary)")
				));
			}
		}
	}
	for (i, w) in widenings.iter().enumerate() {
		if !used.contains(&i) {
			problems.push(format!(
				"listed widening of {} was not used (its old or new contract does not match)",
				w.rune
			));
		}
	}
	for (rune, ids) in paths {
		if ids.len() > 1 {
			problems.push(format!(
				"Rune path {rune} generated {} times ({})",
				ids.len(),
				ids.join(", ")
			));
		}
	}
	problems
}

/// The release's frozen list, a path relative to the repository: the
/// nearest ancestor of the release file holding `tools/polars-gen` (a
/// release copied under `target/` by a test resolves the same way).
pub(crate) fn load(release_path: &Path, rel: &str) -> Vec<Frozen> {
	let abs = std::fs::canonicalize(release_path).unwrap_or_else(|_| release_path.to_path_buf());
	let root = abs
		.ancestors()
		.find(|d| d.join("tools/polars-gen/Cargo.toml").is_file())
		.unwrap_or_else(|| {
			panic!(
				"frozen bindings: no repository above {}",
				release_path.display()
			)
		});
	let p = root.join(rel);
	let text = std::fs::read_to_string(&p)
		.unwrap_or_else(|e| panic!("frozen bindings {}: {e}", p.display()));
	serde_json::from_str(&text).unwrap_or_else(|e| panic!("frozen bindings {}: {e}", p.display()))
}

/// Record 0119: the freeze gate and the namespace rule, fail closed.
pub(crate) fn freeze_self_test() {
	use crate::model::{Inventory, Supporting};
	use crate::release::{FamilyTables, InstantiationScope, Release, ReleaseProvenance};
	use crate::world::World;
	// the gate: survival passes; a moved binding and a duplicate Rune path refuse
	let entry = |key: &str, bindings: &[(&str, &str)]| -> Entry {
		let mut e = crate::emit::Entry::for_test(key);
		e.status = "generated";
		e.bindings = bindings
			.iter()
			.map(|(id, rune)| crate::emit::Binding::for_test(id, rune))
			.collect();
		e
	};
	let fz = |k: &str, id: &str, r: &str| Frozen {
		key: k.into(),
		id: id.into(),
		rune: r.into(),
		summary: None,
	};
	let now = vec![entry("k1", &[("a", "polars::A::f"), ("b", "polars::B::f")])];
	let cat = vec![("polars::A::f".to_string(), "f() -> int".to_string())];
	assert!(check(&[fz("k1", "a", "polars::A::f")], &now, &cat, &[]).is_empty());
	let moved = check(&[fz("k1", "a", "polars::core::A::f")], &now, &cat, &[]);
	assert!(moved[0].contains("moved or disappeared"), "{moved:?}");
	// the same path with another contract is refused too
	let same = Frozen {
		summary: Some("f() -> int".into()),
		..fz("k1", "a", "polars::A::f")
	};
	assert!(check(&[same.clone()], &now, &cat, &[]).is_empty());
	let changed = check(
		&[Frozen {
			summary: Some("f() -> vector of int".into()),
			..same
		}],
		&now,
		&cat,
		&[],
	);
	assert!(changed[0].contains("changed its contract"), "{changed:?}");
	// record 0123: an exact listed widening accepts the change; a wrong or
	// unused one refuses
	let wide = |old: &str, new: &str| Widening {
		rune: "polars::A::f".into(),
		old: old.into(),
		new: new.into(),
		cite: "c".into(),
	};
	let frozen_int = Frozen {
		summary: Some("f() -> int".into()),
		..fz("k1", "a", "polars::A::f")
	};
	let cat_wide = vec![(
		"polars::A::f".to_string(),
		"f() -> int or float".to_string(),
	)];
	assert!(
		check(
			&[frozen_int.clone()],
			&now,
			&cat_wide,
			&[wide("f() -> int", "f() -> int or float")]
		)
		.is_empty()
	);
	assert!(
		check(
			&[frozen_int.clone()],
			&now,
			&cat_wide,
			&[wide("f() -> int", "f() -> str")]
		)
		.iter()
		.any(|p| p.contains("changed its contract"))
	);
	assert!(
		check(
			&[frozen_int],
			&now,
			&cat,
			&[wide("f() -> int", "f() -> int or float")]
		)
		.iter()
		.any(|p| p.contains("was not used"))
	);
	let dup = vec![
		entry("k1", &[("a", "polars::A::f")]),
		entry("k2", &[("c", "polars::A::f")]),
	];
	assert!(check(&[], &dup, &cat, &[])[0].contains("generated 2 times"));
	// the namespace rule: an Arrow type sharing a core short name sits under
	// polars::arrow and leaves the core path where it was
	let sup = |path: &str, kind: &str, target: Option<&str>| Supporting {
		trait_params: vec![],
		key: path.to_string(),
		kind: kind.to_string(),
		canonical_path: path.to_string(),
		found_paths: vec![format!(
			"polars::prelude::{}",
			path.rsplit("::").next().unwrap()
		)],
		crate_paths: vec![path.to_string()],
		public_fields: 0,
		fields_canonical: vec![],
		variant_shapes: vec![],
		variant_payloads: vec![],
		generic: false,
		lifetime: false,
		hidden: false,
		derived: vec![],
		alias_target: target.map(String::from),
		implementors: vec![],
		impls: vec![],
	};
	let inv = Inventory {
		callables: vec![],
		provenance: None,
		supporting: vec![
			sup("polars_core::datatypes::field::Field", "struct", None),
			sup("polars_arrow::datatypes::field::Field", "struct", None),
			sup(
				"polars_arrow::array::primitive::PrimitiveArray",
				"struct",
				None,
			),
			sup(
				"polars_arrow::legacy::index::IdxArr",
				"type_alias",
				Some("polars_arrow::array::primitive::PrimitiveArray<u32>"),
			),
		],
	};
	let release = |namespaced: bool| {
		let mut families = FamilyTables::default();
		if namespaced {
			families.namespaced_crates = vec!["polars_arrow".into()];
		}
		Release {
			name: "t".into(),
			source: "t".into(),
			provenance: ReleaseProvenance::default(),
			instantiation: InstantiationScope::default(),
			api_crates: vec!["polars_core".into(), "polars_arrow".into()],
			unordered: vec![],
			excluded_oracle: vec![],
			refused: vec![],
			families,
		}
	};
	let item = |w: &World, p: &str| {
		w.wrappers
			.get(p)
			.map(|x| format!("{}::{}", x.rune_item, x.rune_name))
	};
	let w = World::new(&inv, &release(true), &["mechanical"]);
	assert_eq!(
		item(&w, "polars_core::datatypes::field::Field").as_deref(),
		Some("::polars::Field")
	);
	assert_eq!(
		item(&w, "polars_arrow::datatypes::field::Field").as_deref(),
		Some("::polars::arrow::Field")
	);
	assert!(
		!w.by_identity
			.contains_key("polars_arrow::array::primitive::PrimitiveArray<u32>"),
		"a namespaced alias forms no alias wrapper"
	);
	// without the rule, the collision moves the core path (what the rule prevents)
	let w0 = World::new(&inv, &release(false), &["mechanical"]);
	assert_eq!(
		item(&w0, "polars_core::datatypes::field::Field").as_deref(),
		Some("::polars::core::Field")
	);
	// record 0119: a deferred prefix keeps a namespaced type at its 0118
	// treatment (no API wrapper unless a signature mentions it)
	let inv2 = Inventory {
		callables: vec![],
		provenance: None,
		supporting: vec![sup(
			"polars_arrow::array::struct_::StructArray",
			"struct",
			None,
		)],
	};
	let mut deferred = release(true);
	deferred
		.families
		.deferred_type_prefixes
		.push(crate::release::DeferredPrefix {
			prefix: "polars_arrow::array::".into(),
			cite: "c".into(),
		});
	let wd = World::new(&inv2, &deferred, &["mechanical"]);
	assert!(
		!wd.wrappers
			.contains_key("polars_arrow::array::struct_::StructArray"),
		"a deferred type forms no wrapper here"
	);
	assert!(deferred.deferred_type("polars_arrow::array::struct_::StructArray"));
	let wn = World::new(&inv2, &release(true), &["mechanical"]);
	assert_eq!(
		item(&wn, "polars_arrow::array::struct_::StructArray").as_deref(),
		Some("::polars::arrow::StructArray"),
		"without the prefix it is an ordinary namespaced wrapper"
	);
	// record 0120: an entry gaining receivers keeps its frozen ids; the new
	// first receiver no longer takes the plain id a frozen binding holds
	let mut ids = std::collections::BTreeMap::new();
	ids.insert(
		("k".to_string(), "polars::arrow::ArrayRef::len".to_string()),
		"p__len".to_string(),
	);
	let mut bs = vec![
		crate::emit::Binding::for_test("p__len", "polars::arrow::Int64Array::len"),
		crate::emit::Binding::for_test("p__len__on__arrayref", "polars::arrow::ArrayRef::len"),
	];
	let recv = vec![
		"a::Int64Array<i64>".to_string(),
		"support::ArrayRef".to_string(),
	];
	crate::emit::pin_frozen_ids(&ids, "k", "p::len", &recv, &mut bs);
	assert_eq!(bs[1].id, "p__len", "the frozen binding keeps its id");
	assert_ne!(bs[0].id, "p__len", "a new binding never takes a frozen id");
	assert!(bs[0].id.contains("__on__"), "{}", bs[0].id);
	println!("freeze self-test: ok");
}

#[cfg(test)]
mod tests {
	#[test]
	fn freeze() {
		super::freeze_self_test();
	}
}
