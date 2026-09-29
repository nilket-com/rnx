//! Record 0114: the ordered dispatch chain, moved verbatim from `main`.
use crate::census::census_dispositions;
use crate::census::{PairRecord, census_summary, instantiation_census};
use crate::emit::Emitted;
use crate::emit::callable::emit_callable;
use crate::emit::types::emit_struct_extras;
use crate::families::conversions::{plan_from_names, resolve_duplicates};
use crate::families::{Claims, claim, finish, target};
use crate::model::{Callable, Inventory};
use crate::release::Release;
use crate::world::World;
use std::collections::{BTreeMap, BTreeSet};

/// Every callable, in the fixed order, through the family rules; then the
/// deferred operator and `INDEX_GET` groups and the census dispositions.
pub(crate) fn generate(
	world: &World,
	inv: &Inventory,
	release: &Release,
	buckets: &Vec<&str>,
) -> (Emitted, Vec<PairRecord>) {
	let mut out = Emitted {
		from_names: plan_from_names(&inv),
		functions: String::new(),
		registrations: vec![],
		catalogue: vec![],
		entries: vec![],
		taken: BTreeMap::new(),
		fn_index: 0,
	};
	let mut callables: Vec<&Callable> = inv.callables.iter().collect();
	callables.sort_by(|a, b| {
		a.canonical_path
			.cmp(&b.canonical_path)
			.then(a.key.cmp(&b.key))
	});
	// inherent methods take names before trait methods do
	callables.sort_by_key(|c| match c.kind.as_str() {
		"inherent" => 0,
		"free_fn" => 1,
		"foreign_trait_impl" => 2,
		_ => 3,
	});
	// record 0076 gate 3: the instantiation census, before any binding is emitted
	let census = instantiation_census(&world, &inv);
	{
		let summary = census_summary(&census);
		println!(
			"instantiation census: {}",
			serde_json::to_string(&summary["by_family"]).unwrap()
		);
	}
	let mut census_by_method: BTreeMap<String, Vec<&PairRecord>> = BTreeMap::new();
	for p in &census {
		census_by_method.entry(p.key.clone()).or_default().push(p);
	}
	let census_keys: BTreeSet<String> = census.iter().map(|p| p.key.clone()).collect();
	// record 0115: the dispatch chain is the `CLAIM` list (first claim wins,
	// in the order the inline chain had at 32d1b40); an unclaimed callable
	// goes to `emit_callable`
	let mut cx = Claims {
		world,
		out: &mut out,
		release,
		buckets,
		census_by_method: &census_by_method,
		op_rows: Vec::new(),
		op_pending: Vec::new(),
		index_rows: Vec::new(),
		index_pending: Vec::new(),
	};
	for c in callables {
		if c.bucket == "unsupported" || c.bucket == "unknown" {
			continue; // not eligible in 0072's terms
		}
		let _target = target(c.key.clone());
		if claim(&mut cx, c) {
			continue;
		}
		emit_callable(world, cx.out, c, buckets);
	}
	finish(cx);
	// record 0116 (rule 3): every listed dtype instantiation is reachable
	if let Err(why) = crate::families::dtype_owners::check_reachable(world, &out.entries) {
		eprintln!("refusing to generate: dtype instantiations: {why}");
		std::process::exit(2);
	}
	emit_struct_extras(&world, &mut out, &buckets);
	resolve_duplicates(&mut out.entries);
	let census = census_dispositions(census, &out.entries, &census_keys, inv);
	(out, census)
}
