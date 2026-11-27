# Implementation plan 0351: The gate and contracts repeat no work

Status: Proposed

Paired ADR: ../adr/0351_the_gate_and_contracts_repeat_no_work.md.

1. The boundary was reported before this plan: the fields that
   enter the reuse and anchor keys, the isolation of the contract
   cases, what the contracts check about their binary, and the
   expected savings.
2. Decision 1, with Control 1.
3. Decisions 2 and 3, with Controls 2 and 3.
4. fmt, Clippy, preflight and the targeted checks in separate
   state. Then the stages of Control 5 by their stage commands.
   Sign. Source review, one complete final, Results.

## Results

Changes landed in `tools/final`, `rune/tools/xtask/manifests/native.json`,
`rune/tools/xtask/src/native.rs` (optional positive `weight` field on
`Case` for `deny_unknown_fields`, leaving xtask's own runner order
unchanged), and `ci/data_consumers.json` (adding the 16,947-byte
manifest consumer entry after weights crossed the 16 KiB hygiene
threshold). No engine source files changed.

Decision 1 (gate reuse identity): `fixed_route::measurement_policy`
retains every policy field that affects built images or captured counts
(`schema`, `anchor`, `kernels`, `modes`, `sizes`, `meter`,
`native_setup`, `installed_setup`, `relative_relocations`, and any
unrecognized future key fail-closed) and excludes only the five
declaration and scoring fields (`source_transitions`, `inventory`,
`anchor_transition`, `per_record_per_mille`, `cumulative_per_mille`).
`fixed_route::reuse_recipe` and schema-2 `measurement_recipe` strip the
two scoring bands from the embedded recipe policy subset while keeping
schema-1 Python parity witnesses unchanged. `gate_inputs::save` and
`load` match on `reuse_recipe` and `measurement_policy` while recording
`full_policy`. Schema-2 captures write `measurement_policy_sha256`
alongside full `policy_sha256`; `fixed_route::score` checks full
`policy_sha256` on baseline and candidate receipts and allows a reused
anchor with `reuse_proof` to match `measurement_policy_sha256` while
keeping its original `policy_sha256`.

Control 1 witnesses:
- `gate_inputs::tests::reuse_requires_the_published_source_recipe_policy_and_each_image`:
  accepts single-field changes to `source_transitions`, `inventory`,
  `anchor_transition`, `per_record_per_mille`, and
  `cumulative_per_mille`; verifies the planted-back full-policy digest
  differs and would refuse all five; refuses single-field changes to
  `schema`, `anchor`, `kernels`, `modes`, `sizes`, `meter`,
  `native_setup`, `installed_setup`, and `relative_relocations`.
- `fixed_route::reused_anchor_accepts_declaration_and_band_changes_and_refuses_measurement_changes`:
  verifies a reused anchor with `reuse_proof` passes under changed
  `source_transitions`, `inventory`, and `anchor_transition` while
  keeping its prior `policy_sha256`, refuses without `reuse_proof`, and
  refuses a `meter` mutation.

Decision 2 (longest-first contracts): added integer `weight` (rounded
from 0348 final case durations) to all 50 cases in
`rune/tools/xtask/manifests/native.json`, validated as a positive
integer when present in `selection::native_manifest`.
`contracts::run_cases` keeps exclusive `wrong_arity-fresh` first and
alone, sorts the 49 shared cases by descending weight (default 1, ties
in manifest order), and writes `rows` back in manifest order. Verified
by `contracts::run_cases_dispatches_by_weight_descending_and_preserves_manifest_result_order`.

Base dispatch order (50 cases): `wrong_arity-fresh`, `vec_local-fresh`,
`loop_binders-fresh`, `loop_iterator-fresh`, `labeled_blocks-fresh`,
`remaining_patterns-fresh`, `function_values-fresh`,
`type_values-fresh`, `option_methods-fresh`, `object_methods-fresh`,
`object_order-fresh`, `iter_receivers-fresh`, `numeric_methods-fresh`,
`iter_terminals-fresh`, `object_keys-fresh`, `iter_adapters-fresh`,
`static_methods-fresh`, `dynamic_methods-fresh`, `struct-methods`,
`struct-constructors`, `struct-enum_variants`, `struct-nominal_clone`,
`struct-small_forms`, `struct-text_methods`, `struct-text_pipeline`,
`fresh73`, `strict133`, `fresh166`, `strict165`, `strict190`,
`strict84`, `strict111`, `strict131`, `pipeline-stack2MiB`,
`format-specs-fresh`, `IndexSet-negatives`, `expression100`, `root71`,
`copy8`, `migration`, `expression-ownership`, `shadow`,
`range-range_index-contract`, `range-range_index-endpoint_contract`,
`range-loop_iterator-contract`, `range-field_iter-contract-fresh`,
`range-experimental_core-heap-ranges-`,
`range-experimental_core-heap-range_iterator_tests-`,
`range-experimental_core-heap-children_tests-`,
`live-oracle-blocking-bounds`.

Candidate dispatch order (50 cases): `wrong_arity-fresh`, `shadow`,
`iter_adapters-fresh`, `iter_terminals-fresh`, `static_methods-fresh`,
`range-range_index-contract`, `numeric_methods-fresh`,
`dynamic_methods-fresh`, `format-specs-fresh`, `object_methods-fresh`,
`range-field_iter-contract-fresh`, `struct-text_methods`, `fresh73`,
`strict190`, `struct-nominal_clone`, `struct-text_pipeline`,
`strict165`, `struct-enum_variants`, `fresh166`,
`option_methods-fresh`, `strict133`, `struct-constructors`,
`strict131`, `remaining_patterns-fresh`, `struct-small_forms`,
`strict111`, `type_values-fresh`, `loop_binders-fresh`,
`range-range_index-endpoint_contract`, `labeled_blocks-fresh`,
`strict84`, `expression100`, `function_values-fresh`, `struct-methods`,
`vec_local-fresh`, `object_order-fresh`, `iter_receivers-fresh`,
`object_keys-fresh`, `root71`, `loop_iterator-fresh`,
`range-loop_iterator-contract`, `pipeline-stack2MiB`,
`IndexSet-negatives`, `copy8`, `migration`, `expression-ownership`,
`range-experimental_core-heap-ranges-`,
`range-experimental_core-heap-range_iterator_tests-`,
`range-experimental_core-heap-children_tests-`,
`live-oracle-blocking-bounds`.

Decision 3 (reuse check-1 binary): `stage.rs` resolves check 1's
`engine-dev` target directory via `contracts::suite_reuse_target` and
passes it to `contracts::run_with_target` / `build_with_target`. Cargo
runs the unchanged `--no-run --message-format=json` command against
that target, verifies all requested manifest features are present and
no sibling-recipe features are enabled, copies the executable to
`rune-tests`, and runs the unchanged `readelf` DWARF check and
`BINARY.json` digest proof. Verified by
`contracts::build_with_target_reuses_suite_target_and_rejects_mismatched_features`.

Back-to-back quiet standalone `rune-suite` measurement (identical
environment, clean `engine-dev` target, base `5a021fa7` then candidate):
- Base `5a021fa7`: `rune-suite` PASS in 1,085.593 s; `contracts` PASS
  in 469.083 s (50/50); `build.log` showed 0 fresh and 105 rebuilt
  artifacts (`Finished engine-dev profile in 42.11s`).
- Candidate: `rune-suite` PASS in 906.475 s (-179.118 s); `contracts`
  PASS in 388.806 s (-80.277 s, 50/50, within 1.6 s of
  `wrong_arity-fresh` 93.9 s + `shadow` 293.3 s); `CONTRACTS.json` case
  order identical to base; `build.log` showed 105 fresh and 0 rebuilt
  artifacts (`Finished engine-dev profile in 0.07s`).

Pre-sign checks on the candidate tree:
- `cargo +1.98.1 fmt --all -- --check`: PASS.
- `format-final`, `clippy-final`, `final-controls`: PASS.
- `format-package-xtask`, `lint-package-xtask`, `package-xtask`: PASS.
- `ci/preflight.py`: PASS (hygiene and all 60 Python checks).
- `root-tests` (`cargo +1.98.1 test --no-fail-fast --locked --features project-sources -- --test-threads=4`): PASS.
- `cross` (`rnx-final stage cross --suite all`): PASS.
- `rune-suite` (`rnx-final stage suite --stage rune-suite`): PASS (50/50 contracts).

Source review found that the first unsigned commit would have failed the
final: the xtask parser of the contract manifest rejects unknown fields
and had no weight field, so its package-xtask tests failed. The parser
was given the field and every stage that reads a changed file was run on
the corrected tree before signing.

The complete final ran on 1b108f90 as FINAL-v1 and passed all 107
stages. RESULT.json has sha256
7f37395401c19e95ebb862b02afa62433f33e0ef58a9c3b00af6c3f059ae5f21.
Entry through cleanup took 4,233.0 seconds against the 5,100 second
limit. The run entered with 186,409,488,384 bytes free against the
151,152,450,560 required. The longest chain was the gate, which ran from
3,593 to 4,178 seconds. The Rune suite ended at 1,936 seconds with 697.6
seconds in the contracts. The gate inputs published with ADR 0350 were
recorded under the old identity, so they could not match the new one and
the gate rebuilt and measured the base on the clock. The anchor lookup
missed and the anchor build took 97.3 seconds. The next record is the
first whose final can reuse them. The source condition passed over 40
items with no transition declared and none needed. All 32 per-record
rows passed and all 32 were exact. All 32 cumulative rows passed. For
Control 4, the 107 stage names equal those of the final of ADR 0350, and
the 50 contract cases have the same names in the same order of results.
For Control 5, against the final of ADR 0350: the contracts took 697.6
seconds against 774.0; the suite stage took 1,817.9 against 1,805.0; the
gate took 584.8 against 554.3; the whole run took 4,233.0 against
4,164.0. Inside a final the suite stage and the whole run did not get
shorter. The saving measured before signing, 179 seconds on a quiet
host, is the suite stage run alone. The cross stage ran the host path
control in 40.7 seconds with 25,738 sources compared, the 422 pinned
combinations matched and none unexpected. The fetch stayed offline and
took 12.3 seconds. The build peaked at 121,017,085,952 bytes and shared
memory at 28,000,256 bytes. The cleanup cleaned 37 targets and removed
the run. The memory controller reported no events. The run left no new
entry in /tmp or /var/tmp.
