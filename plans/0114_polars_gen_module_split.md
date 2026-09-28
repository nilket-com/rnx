# rnx 0114: split the Polars generator by concern

Status: plan. Record 0113 is closed on `origin/main` at `eee390c`. This record changes how the generator is organized and nothing it produces. It adds no bindings and moves no count: v2 stays at 3,761 available and 2,513 value-tested of 6,441 applicable callables, and 0.55.2 production stays at 2,585 and 1,727.

## Vocabulary

This record uses four layers, and "codegen" names only layer 2 → 3.

| layer | who or what | produces |
|---|---|---|
| 1. authoring | Claude and Codex, driven by plans, reviews and evidence | the generator's Rust source |
| 2. generation | `tools/polars-gen`, a deterministic Rust program | Rust source: bindings, fixtures, oracle tests, `surface.json` |
| 3. bindings | `adapters/polars/src/generated/`, compiled | Rune functions and protocols on Polars types |
| 4. scripts | Rune code written by users | `(a + b)?`, `df["x"]` |

## Why

`tools/polars-gen/src/main.rs` is 23,436 lines, with 249 top-level items. The only separate files are `model.rs` (118) and `ty.rs` (193). By concern it holds:
- 34 self-test functions, about 9,500 lines. They run as one hand-chained call tree from `--self-test`: `wrapper_self_test` → `applicability_self_test` → `iterator_self_test` → `from_naming_self_test`, which then calls the other 30 in sequence.
- the oracle and fixture generator, about 3,000 lines: `emit_oracle` alone is 1,213 lines, and `impl Oracle` is 947;
- the binding emitters, about 2,900 lines, including `emit_method_with` (668) and `emit_foreign` (390);
- the release file's schema and gates, about 1,300 lines;
- about 6,800 other lines, including three `impl World` blocks: construction and spelling (463), argument and return mapping (1,544), and bound proof (476).

160 comments cite 40 different records. The file grew one layer-1 diff at a time, each the smallest change that passed review. A record's rule went into whichever shared function it had to change: `main`'s dispatch chain, `emit_callable`, `emit_method_with`, `emit_foreign`'s trait `match`, or `emit_oracle`'s callee `match`. The rule did not go into a module for its concern. Most round-1 review findings in 0110–0113 were a new rule trusting what an older gate in the same function had assumed. That interaction is hard to see when unrelated rules share a function body.

## The split

The crate becomes modules under `tools/polars-gen/src/`. Every item moves verbatim; only visibility (`pub(crate)`) and `use` lines change.

- `main.rs`: the CLI only (arguments, loading, exit codes) and the `--self-test` list.
- `pipeline.rs`: the ordered dispatch chain from `main`, verbatim.
- `release.rs`: `Release`, provenance, `load`/`check_provenance`/`is_api`, and the scope, exclusion and ordering gates. A family's gate struct (for example `ChunkSnapshot`) moves to that family's module, and `Release` refers to it by path.
- `world/`:
  - `mod.rs`: `World`, `Wrapper`, the `HAND_*` tables, `rune_name`, `spell*`, and the `World::new` block;
  - `mapping.rs`: `Arg`, `Ret`, `Materialize`, the iterator return rules, the integer narrowing tables, and the argument/return `impl World`;
  - `proof.rs`: `Applicability`, `split_head`, `unify*`, `sizedness`, `core_trait_on_builtin`, and the bound-proof `impl World`.
- `census.rs`: `PairRecord`, `instantiation_census`, `iterator_census`, `conversion_census`, `callback_census`, `census_summary`, and the pair-disposition pass now inline in `main`.
- `emit/`:
  - `mod.rs`: `Emitted`, `Entry`, `Binding`, `OracleInfo`, `binding_id`, `rust_ident`, `signature_of`;
  - `callable.rs`: `emit_callable`, `emit_method(_with)`, `materialized_call`, routing;
  - `free.rs`: `emit_free` and free instantiations;
  - `instantiations.rs`;
  - `types.rs`: `emit_types`, `emit_struct_extras`;
  - `output.rs`: assembling `functions.rs`, `catalogue.rs`, `mod.rs` and `surface.json`, then write or `--check`.
- `families/`, one module per admission rule, each holding its gate, its release schema, its emitter and its self-test:
  - `snapshots.rs`: 0099–0106 chunk, indexed, array, iter, view, owned-iter and layout snapshots;
  - `bounds.rs`: 0091–0098 external bounds, sized self, null-aware returns, method scalar generics, hash tokens, bounded read-back;
  - `callbacks.rs`: 0079/0080;
  - `conversions.rs`: 0078 `From`, name plan, duplicates;
  - `generic_inputs.rs`: 0082–0086 slices, generic inputs, bitmaps;
  - `receivers.rs`: 0109/0110 move semantics, arity, proven trait receivers;
  - `protocols.rs`: 0111 `emit_foreign`, `protocol_shape`;
  - `serde.rs`: 0112;
  - `generic_impls.rs`: 0113 (operators, `FromIterator`, `Index`, and the named refusals, which share one self-test);
  - `returns.rs`: 0077 iterator returns, 0088 cow returns, 0093 checked read-back;
  - `free_instantiations.rs`: 0089.
- `oracle/`:
  - `fixtures.rs`: `FIXTURES`, `TYPED_FIXTURES`, `Recipe`;
  - `mod.rs`: `Oracle`;
  - `emit.rs`: `emit_oracle`;
  - `harness.rs.txt`, which holds the Rust test harness `emit_oracle` writes into `generated_oracle.rs`. Today that harness is a raw string, and the move-check below would otherwise read its `struct Case` / `fn main` lines as items. It is included with `include_str!`, byte for byte.

The module list is a starting map, not a contract. Items move to where their callers and tests are, and the evidence records the final tree with line counts. A module over about 1,500 lines needs a stated reason.

## A pure move

This record changes no control flow. `main`'s ordered dispatch chain (the instantiation and iterator-return branches, `index_row`, `from_iter_row`, `unary_generic_shape`, `generic_op`, `generic_impl_refusal`, then `emit_callable`) moves verbatim into `pipeline.rs`, together with the deferred operator and `INDEX_GET` groups and the census disposition pass. `emit_oracle` moves verbatim into `oracle/emit.rs`, callee `match` included.

Each family's gate and emitter functions move to that family's module, and are still called from the same place in the same order. A rule body that sits inline in `emit_callable`, `emit_method_with` or `emit_foreign` stays inline in this record.

A registry would replace the chain with a claims/admit table, a finish step and an oracle-arm dispatch. That changes control flow, which the golden pairs and the line-multiset check cannot prove equivalent for inventories they do not contain. It is therefore a separate follow-up, planned after this split is stable.

## Self-tests

Each of the 34 self-test functions moves next to the code it tests, and keeps its body.

The nested calls that chain them (`wrapper_self_test` → … → `from_naming_self_test` → 30 more) are replaced by one ordered list in `main.rs`, which `polars-gen --self-test` runs in today's order. The flag keeps its documented behaviour and its output, and golden compares that output.

Each function also gets one `#[test]` wrapper, so `cargo test --release --locked` in `tools/polars-gen` names a failing test instead of aborting a 34-deep chain. The evidence lists the 34 functions against their wrappers, one to one. If a test depends on process state or order, the evidence says so and that wrapper is serialized.

## Proof that nothing changed

**1. Byte identity of the generator's output.** Build the generator at `eee390c` and at the implementation. Run both over every input pair the repository uses:
- the production 0.55.2 pair: `probes/0072/out/0.55.2-adapter-narrow/result/inventory.json`, `releases/0.55.2-joins.toml`, and buckets `mechanical,conversion,option_struct,callback,generic_fn`;
- the v2 pair: `probes/0108/out/v2py/result/inventory.json` and `probes/0108/v2.toml` with the same buckets;
- the default-bucket invocation;
- each other file under `tools/polars-gen/releases/` whose inventory is present and which generates at `eee390c`. A release file that does not generate at the base is named as such and left out.

Each run writes into a scratch adapter. `diff -r` must be empty over:
- the seven output files;
- stdout, stderr and the exit code.

Golden also records, and compares:
- every inventory × release pair that fails provenance at the base, with its refusal text and exit code (44 of the 60 base runs);
- `--check` on the committed adapter (it passes);
- `--check` against a copy of the adapter with one byte changed in `functions.rs` (drift is reported and it exits 1);
- the stdout of `--self-test`.

`probes/0114/golden.sh` does all of this. The evidence records the SHA-256 of each output at both commits.

At the base the harness is deterministic: two runs are identical, 60 runs take 12 s, and 16 generate (all five 0072 inventories and the v2 inventory under their release files).

**2. Nothing is lost in the move.** `probes/0114/moved.py` normalizes the base `main.rs` and the new tree:
- it strips `pub(crate)`/`pub(super)`, `use` and `mod` lines, blank lines, and indentation;
- it treats the moved harness file as one block.

It then compares the multisets of lines. The only lines allowed to differ are those in a reviewed allowlist inside the script: the `#[test]` wrappers, the self-test list that replaces the nested calls, the lines that assemble `pipeline`'s function around the moved chain, and visibility. Each allowlisted line gives its reason. A dropped or duplicated gate line fails the script.

**3. The self-tests.** All 34 are accounted for, and they pass both under `cargo test` and under `--self-test`. A mutation control confirms that the moved tests still bite: reverting 0113's operator-order fix (`kind_of`) must fail `generic_impls`, as it did in 0113.

**4. The existing replays.** Replays 0108–0113 pass unchanged. They do not run the generator, so they show only that no evidence was touched.

The generated adapter is byte-identical, so its compiled code cannot change. The adapter suites, the v2 oracle and the launch measurement therefore do not need to be re-run. If proof 1 finds any difference, the implementation is wrong, and the difference is fixed rather than explained away.

## Authoring rule from here on

The evidence shows the tree meets this rule. A future family record:
- puts its gate, release schema, emitter and self-test in its own module under `families/`;
- adds one call site in `pipeline.rs`, and one oracle arm in `oracle/emit.rs`, until the registry follow-up replaces both with registrations;
- touches `world/`, `emit/` or `oracle/` only for a shared capability the plan names, such as a new argument mapping. It never touches them to host its own rule.

Review can then check a record's diff by its paths: the family module plus the named call sites and shared changes. Any other hunk needs its own justification. The next rule, generic_methods (793 rows), is the first record to test this.

## Stop rule

Stop and report if:
- byte identity cannot be reached without reordering a check or changing an emitted string;
- a self-test proves to have relied on the old call order in a way that changes what it asserts.

Either finding is a hidden behavioural dependency, and it goes to Codex before any workaround. This is one plan and one implementation commit. No count, binding, evidence bundle or adapter file changes.
