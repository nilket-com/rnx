//! Record 0073: generate Rune bindings for the Polars adapter from the
//! record 0072 inventory.
//!
//!   polars-gen <inventory.json> <adapter-dir> [--check] [--buckets mechanical,conversion,option_struct]
//!
//! Writes <adapter-dir>/src/generated/{types.rs,functions.rs,mod.rs,catalogue.rs}
//! and <adapter-dir>/surface.json. With --check, writes to a temporary
//! directory and exits nonzero if anything differs from what is committed.
mod census;
mod emit;
mod families;
mod model;
mod oracle;
mod pipeline;
mod release;
mod text;
mod ty;
mod world;

use crate::release::Release;
use crate::world::World;
use model::Inventory;
use std::path::PathBuf;

/// Record 0114: every generator self-test, in the order the former nested
/// calls ran them; `--self-test` runs this list and `cargo test` runs each
/// through its module's `#[test]` wrapper.
const SELF_TESTS: &[fn()] = &[
	crate::world::wrapper_self_test,
	crate::world::proof::applicability_self_test,
	crate::world::mapping::iterator_self_test,
	crate::families::conversions::from_naming_self_test,
	crate::families::conversions::from_emission_self_test,
	crate::families::callbacks::callback_self_test,
	crate::families::generic_inputs::slice_self_test,
	crate::families::generic_inputs::generic_input_self_test,
	crate::families::generic_inputs::bitmap_self_test,
	crate::families::generic_inputs::bitmap_input_self_test,
	crate::families::returns::iterator_return_self_test,
	crate::families::returns::cow_return_self_test,
	crate::families::free_instantiations::free_instantiation_self_test,
	crate::families::receivers::method_arity_self_test,
	crate::families::receivers::mut_return_self_test,
	crate::families::dtype_owners::dtype_owners_self_test,
	crate::world::mapping::time_zone_self_test,
	crate::families::receivers::move_semantics_self_test,
	crate::families::receivers::trait_receivers_self_test,
	crate::families::generic_traits::generic_traits_self_test,
	crate::families::protocols::protocols_self_test,
	crate::families::serde::serde_self_test,
	crate::families::generic_impls::generic_impls_self_test,
	crate::families::bounds::native_substitution_self_test,
	crate::families::bounds::method_scalar_generic_self_test,
	crate::families::returns::checked_readback_self_test,
	crate::families::bounds::hash_token_self_test,
	crate::families::bounds::null_aware_self_test,
	crate::families::bounds::sized_self_self_test,
	crate::families::bounds::external_bound_self_test,
	crate::families::snapshots::chunk_snapshot_self_test,
	crate::families::snapshots::indexed_chunk_self_test,
	crate::families::snapshots::array_snapshot_self_test,
	crate::families::snapshots::iter_snapshot_self_test,
	crate::families::snapshots::view_snapshot_self_test,
	crate::families::snapshots::owned_iter_self_test,
	crate::families::snapshots::layout_self_test,
	crate::release::policy_self_test,
];

fn main() {
	let args: Vec<String> = std::env::args().collect();
	if args.iter().any(|a| a == "--self-test") {
		for test in SELF_TESTS {
			test();
		}
		return;
	}
	if args.len() < 3 {
		eprintln!(
			"usage: polars-gen <inventory.json> <adapter-dir> --release <file> [--check] [--buckets a,b,c]"
		);
		std::process::exit(2);
	}
	let check = args.iter().any(|a| a == "--check");
	let buckets: Vec<String> = args
		.iter()
		.position(|a| a == "--buckets")
		.map(|i| args[i + 1].split(',').map(|s| s.to_string()).collect())
		.unwrap_or_else(|| {
			vec![
				"mechanical".into(),
				"conversion".into(),
				"option_struct".into(),
				"callback".into(),
			]
		});
	let buckets: Vec<&str> = buckets.iter().map(|s| s.as_str()).collect();
	let inv: Inventory =
		serde_json::from_str(&std::fs::read_to_string(&args[1]).expect("inventory"))
			.expect("inventory json");
	let release_path = args
		.iter()
		.position(|a| a == "--release")
		.map(|i| PathBuf::from(&args[i + 1]))
		.unwrap_or_else(|| {
			eprintln!("--release <file> is required");
			std::process::exit(2)
		});
	let (release, release_digest) = Release::load(&release_path);
	match release.check_provenance(&inv) {
		Ok(p) => println!("inventory provenance: {p}; release file: {}", release.name),
		Err(e) => {
			eprintln!("refusing to generate: {e}");
			std::process::exit(2);
		}
	}
	let world = World::new(&inv, &release, &buckets);
	let (out, census) = pipeline::generate(&world, &inv, &release, &buckets);
	emit::output::write(
		out,
		world,
		census,
		&inv,
		&release,
		&release_path,
		&release_digest,
		&buckets,
		&args,
		check,
	);
}
