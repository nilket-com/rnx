# rnx 0114 evidence: polars-gen split by concern

The generator's 23,436-line `main.rs` is now 31 Rust files (two of them the unchanged `model.rs` and `ty.rs`) and two harness text files under `tools/polars-gen/src/`. Its output did not change by a byte. `probes/0114/verify.sh` replays every proof below from a clean checkout. It builds the base generator (`eee390c`) in a git worktree, and takes about 80 s.

This record adds no bindings and moves no counts:
- v2: 3,761 available and 2,513 value-tested, of 6,441 applicable callables;
- 0.55.2 production: 2,585 available and 1,727 value-tested.

## Proof 1: byte identity (`probes/0114/golden.sh`)

The script runs one generator binary, first at `eee390c` and then at 0114, over every inventory × release file × bucket set in the repository:
- 6 inventories: `probes/0072/out/{0.54.4,0.55.2}-*` and `probes/0108/out/v2py`;
- 5 release files: `releases/*.toml` and `probes/0108/v2.toml`;
- 2 bucket sets: production and default.

That is 60 runs. 16 of them generate: every inventory under its own release files, including the production 0.55.2 pair and the v2 pair. The other 44 are refused on provenance, and their refusal text and exit code are compared too.

Each run keeps:
- the seven output files: `types.rs`, `functions.rs`, `catalogue.rs`, `mod.rs`, `fixtures.rs`, `tests/generated_oracle.rs`, `surface.json`;
- stdout, stderr and the exit code.

The script also records four controls:
- `--check` on the committed adapter exits 0;
- `--check` on a copy of the adapter with one byte changed in `functions.rs` reports the drift and exits 1;
- `--self-test`, both its full output and its exit code;
- a second base run, identical to the first, which shows the harness is deterministic.

`diff -r` between the base and the 0114 runs is empty. The base outputs' SHA-256 values (298 files) are committed in `probes/0114/digests.txt`, and the replay checks them. The inventories are the gitignored build outputs of 0072 and 0108. `verify.sh` restores them from committed bundles when they are absent, and checks all six against `probes/0114/inventories.sha256` either way:
- the five 0072 inventories are bundled in `probes/0114/evidence/` (`gzip -9n`, 2.6 MB);
- the v2 inventory is 0112's bundle, byte-identical.

`golden.sh` pins `LC_ALL=C`, so neither the glob order nor the digest order depends on the locale.

## Review round 1 (Codex)

**1. Digest order.** The committed `digests.txt` was sorted under this host's locale, and Codex's replay sorted `0.55.2` and `0.55.2-joins` differently, so the `diff` failed. The 298 hashes were identical as a multiset. `golden.sh` now sets `LC_ALL=C`, and the digests were regenerated under it.

**2. Clean-checkout inputs.** `golden.sh` read six gitignored inventories that `verify.sh` neither restored nor built. It now restores them from bundles, as described above.

**Replay.** With all six local inventories moved aside, `verify.sh` restored them and passed under `LC_ALL=en_US.UTF-8` and again under `LC_ALL=C.UTF-8`. The restored files match the originals' hashes.

The generated adapter is byte-identical, so its compiled code cannot have changed. As the accepted plan says, the adapter suites, the v2 oracle and the launch measurement were therefore not re-run.

## Proof 2: nothing lost in the move (`probes/0114/moved.py`)

The script splits the base `main.rs` and the new tree into top-level items, using a small Rust lexer that handles comments, strings, raw strings, char literals and lifetimes. It normalizes three things:
- whitespace;
- `pub(crate)`;
- rustfmt's trailing commas (the added visibility makes rustfmt rewrap some signatures).

It then compares the two multisets, ignoring `use` and `mod` items. The result is 241 base items; 240 of them, all but `main`, match exactly once. The only differences are the four named in the plan, and each is checked exactly:
1. **The oracle harness.** The two raw strings are now `oracle/harness_head.rs.in` and `oracle/harness_runner.rs.in`, and the script inlines them back before comparing. The head's final blank line stays in code as `concat!(include_str!(…), "\n")`, so the file passes `git diff --check`.
2. **The self-test chain.** The 32 nested tail calls (`wrapper` → `applicability` → `iterator` → `from_naming` → 29 more, then `policy` from `main`) are removed from the base items. The new `SELF_TESTS` list must name all 34 in that exact call order.
3. **`main`.** Its body equals `main` + `pipeline::generate` + `output::write` joined, apart from a pinned list of seam statements:
   - the two base self-test calls;
   - the list loop;
   - the two calls replacing the moved bodies;
   - the two new signatures;
   - the returned `(out, census)`.
4. **The `#[test]` wrappers.** There are 34, each calling only its own self-test. Each `tests` module holds nothing else.

`model.rs` and `ty.rs` are unchanged since `eee390c`.

**Mutation control.** Deleting one line of `protocol_shape`'s gate in a copy of the tree fails the check: the item appears as lost from the base and added in the new tree.

## Proof 3: self-tests

`polars-gen --self-test` prints the same 36 lines as before, and golden compares them. `cargo test --release --locked` runs 34 of 34 in parallel and they all pass. No test depended on process state or on order, so none is serialized.

| test module | self-tests |
|---|---|
| `world` | wrapper |
| `world::proof` | applicability |
| `world::mapping` | iterator |
| `release` | policy |
| `families::conversions` | from_naming, from_emission |
| `families::callbacks` | callback |
| `families::generic_inputs` | slice, generic_input, bitmap, bitmap_input |
| `families::returns` | iterator_return, cow_return, checked_readback |
| `families::free_instantiations` | free_instantiation |
| `families::receivers` | method_arity, move_semantics, trait_receivers |
| `families::protocols` | protocols |
| `families::serde` | serde |
| `families::generic_impls` | generic_impls |
| `families::bounds` | native_substitution, method_scalar_generic, hash_token, null_aware, sized_self, external_bound |
| `families::snapshots` | chunk_snapshot, indexed_chunk, array_snapshot, iter_snapshot, view_snapshot, owned_iter, layout |

**Mutation control.** Restoring 0113's pre-review operator sort (`format!("{:?}", a.rhs)` instead of `kind_of`) fails `families::generic_impls::tests::generic_impls` (panicking at `generic_impls.rs:389`), as it did in 0113.

## Proof 4: the replays

The 0108–0113 replays pass unchanged. `cargo fmt --check` and `git diff --check` pass. The build has the same 23 warnings as the base (the unread `cite` fields and the no-op `.clone()` calls), and none are new.

## The tree

Lines, total and self-test:

| file | total | self-tests | code |
|---|---:|---:|---:|
| `main.rs` | 126 | | 126 |
| `pipeline.rs` | 271 | | 271 |
| `release.rs` | 477 | 151 | 326 |
| `census.rs` | 660 | | 660 |
| `text.rs` | 125 | | 125 |
| `world/mod.rs` | 979 | 224 | 755 |
| `world/mapping.rs` | 1,821 | 73 | 1,748 |
| `world/proof.rs` | 1,221 | 462 | 759 |
| `emit/mod.rs` | 405 | | 405 |
| `emit/callable.rs` | 987 | | 987 |
| `emit/free.rs` | 253 | | 253 |
| `emit/instantiations.rs` | 490 | | 490 |
| `emit/types.rs` | 236 | | 236 |
| `emit/output.rs` | 122 | | 122 |
| `families/snapshots.rs` | 2,794 | 2,173 | 621 |
| `families/bounds.rs` | 2,224 | 1,790 | 434 |
| `families/generic_impls.rs` | 1,424 | 433 | 991 |
| `families/callbacks.rs` | 1,222 | 690 | 532 |
| `families/generic_inputs.rs` | 1,116 | 1,095 | 21 |
| `families/protocols.rs` | 825 | 306 | 519 |
| `families/conversions.rs` | 736 | 480 | 256 |
| `families/serde.rs` | 723 | 373 | 350 |
| `families/receivers.rs` | 651 | 557 | 94 |
| `families/returns.rs` | 604 | 573 | 31 |
| `families/free_instantiations.rs` | 405 | 224 | 181 |
| `oracle/emit.rs` | 1,283 | | 1,283 |
| `oracle/mod.rs` | 1,037 | | 1,037 |
| `oracle/fixtures.rs` | 343 | | 343 |
| `oracle/harness_*.rs.in` | 450 | | (text) |

Plus the unchanged `model.rs` (118) and `ty.rs` (193), and `families/mod.rs` (11).

**The modules over about 1,500 lines, and why:**
- `snapshots.rs` and `bounds.rs` are mostly self-tests: 7 and 6 families' tests, with 621 and 434 lines of code.
- `world/mapping.rs` is one 1,544-line `impl World` block of argument and return mapping. This record moves it whole, and does not split it.

**What the split exposes.** Several families hold little code of their own:
- `generic_inputs`: 21 lines;
- `returns`: 31 lines;
- `receivers`: 94 lines.

Their rules still live inline in `world/mapping.rs` (1,748 lines of code) and `emit/callable.rs` (`emit_method_with`, 987). Under the pure-move rule these bodies stayed where they were. They mark exactly where the concerns are still mixed. They are the first target of the registry follow-up Codex asked for, which will move them behind a family interface. That follow-up changes control flow, so it needs its own plan and proof.

## Authoring rule

`tools/polars-gen/README.md` now has the module map and this rule. A new family rule goes in its own module under `families/`, with one call site in `pipeline.rs` and, if it has one, one oracle arm in `oracle/emit.rs`. It changes `world/`, `emit/` or `oracle/` only for a shared capability its plan names.
