# rnx 0108 evidence: Polars v2 Rust API baseline

All outputs are in `probes/0108/out/`, pinned by `probes/0108/digests.txt`. The production adapter still targets 0.55.2, and the paused 0107 stash was left untouched.

## Pins

- Source: the Polars workspace at tag `py-2.0.0-rc.2`, commit `da47b7405f0e2d188e71cce7c2d588c695f4098d` (committed 2026-09-20). It is the commit 0074 probed. No Rust 2.0 crate is published; crates.io stops at 0.55.2.
- Features: the 145 `polars` features that cargo resolves for `polars-runtime-32` in that checkout (`features.sh` → `features.txt`). That runtime is the one `pip install polars` selects. `polars-runtime-64` adds only `bigidx`. The adapter-narrow build uses 13 features and 0074 used 3.
- Provenance: the hash-pinned wheel `polars-runtime-32==2.0.0rc2` reports `_BUILD_COMMIT == da47b74` (`python.sh`). The Python wheel and this pin are therefore the same source.
- Toolchain: nightly-2026-09-20 (rustc 1.100.0-nightly), rustdoc format 61. All 30 crates were documented with no failures. The adapter build also needs this nightly, because the wheel's `nightly`/`simd` features require one. That affects `:dep` users before 0.55.2 can be retired.
- Release input: `probes/0108/v2.toml`. It copies the 0.55.2-joins tables verbatim and changes three things:
  - `[provenance]` names the rev, cfg `v2py` and the 145 features.
  - `api_crates` adds `polars_defs`, `polars_sql` and `polars_config`. `polars_descriptions` and `polars_observer` are omitted because the extractor reaches no callable in them.
  - The join-order policy is re-cited at v2: `polars-defs/src/join.rs:119`, where `MaintainOrderJoin::None` is `#[default]`; `JoinArgs::new` at `join.rs:140-148`; and `polars-lazy/src/frame/mod.rs:1240, 1266, 1292`.

## Three denominators (`census.py`)

| View | Count |
|---|---:|
| Full: every reachable public callable identity | 8,389 |
| Excluded: `unsafe` (caller invariants a script cannot promise) | 224 |
| Excluded: `#[doc(hidden)]` (declared not API by Polars) | 1,123 |
| Excluded: raw pointer | 50 |
| Reported apart: marker impls / duplicate listings | 417 / 22 |
| **Applicable** | **6,553** |
| **Available to a script** (current generator, v2) | **2,818 (43.0% of applicable; 33.6% of full)** |
| **Value-tested** by the direct Rust oracle | **1,738 (26.5% of applicable)** |
| Missing, each with a root cause | 3,735 |
| Still needed for 90% of applicable | 3,080 |

Internal-looking crates (arrow, utils, compute, row, parquet) stay in the applicable count. The generator's `out_of_scope` status counts as missing, with cause `internal_crate`.

## 0.55.2 → v2

- Callable identities (the 0074 `identity`/`delta`, executed from its source): 6,126 same, 38 reshaped, 201 removed, 2,225 added. Most additions come from the wider feature set: plan 809, core 392, defs 353.
- Migration: of the 2,136 bindings a script has today, 132 are lost at v2:
  - 97: identity removed or reshaped.
  - 24: generic instantiation entries cited at 0.55.2 no longer admit.
  - 9: callbacks.
  - 2: now `#[doc(hidden)]` or `unsafe`.

  The full list is in `census.json` under `migration`.
- `merge_validities` exists at v2 with the contract 0107 assumed: `&mut self`, `chunks: &[ArrayRef]`, unit return, `T: PolarsDataType`. It belongs in batch rule 5 or 6, not in a record of its own. The 0107 stash is not applied.

## Build and direct oracle (`build.sh`)

- The current generator emits 2,809 generated plus 450 adapted entries; 2,970 are unsupported and 765 out of scope. The scratch adapter builds with `--locked` at v2.
- Two infrastructure fixes were needed, both kept apart from binding policy. Each is behaviour-preserving at 0.55.2: `--check` passes before the control change, and the production regeneration differs only in that control.
  1. **Method arity.** Rune's `InstanceFunction` goes through `Function` over (receiver, args…) (rune 0.14.2 `function/mod.rs:122-140`), so a method shares the five-argument limit. The generator comment claiming fifteen was wrong. v2's `Expr::qcut` and `qcut_uniform` (receiver + 5) failed to compile; they are now refused with an `arity` reason. The fix adds `METHOD_ARITY` and the `method-arity` self-test.
  2. **Oracle controls.**
     - The 0094 hash-token fixture now searches from `rnx-0094-5` for an upper-half name, because v2's `Categories` hash differs. At 0.55.2 the first name found is still `rnx-0094-5`.
     - The "binding that panics in Polars" control used `Column::product`, which panicked only because the `product` feature was off. It now uses `Series::select_chunk(2)`, which is out of bounds at both versions.
- Oracle: 2,797 cases, all controls failing closed.

  | Outcome | Cases |
  |---|---:|
  | match | 2,457 |
  | row_order_differs (approved) | 5 |
  | both_error | 141 |
  | both_panic | 20 |
  | fixture_failed | 172 |
  | oracle_panicked | 2 |

- Case by case against 0.55.2 (2,080 shared cases) there is **no value regression**:
  - 7 both_panic → match and 6 both_error → match, from newly enabled features (`product`, `round_series`, reductions, `dtype-array`).
  - 3 match → row_order_differs, all under the approved unordered policy (`join`, `left_join`, `unique`), as 0074 found.
  - 1 match → both_error: `TimeZone::opt_try_new`, an upstream behaviour change on which both sides agree.
  - 1 both_panic → both_error.
- The 172 fixture failures are all v2-only cases. Fixtures for the new dtypes do not build yet: `f16` 48, `i128` 47, `u128` 45, map 13, JSON 8, extension 6, decimal 4, and 1 other. They count against value-tested, not as mismatches.
- The 2 oracle panics are v2-only `ArrayChunked::{set,with}_validity` cases. The generated fixture has 2 rows and a 3-entry mask. Polars asserts on the Rust side, while the Rune binding's length guard returns an error, so the binding fails closed and the fault is the fixture's.

## Bulk route (`batches.py`)

Every missing applicable callable is assigned to exactly one rule. The counts are upper bounds; each follow-on record measures its own yield and lists its refusals.

| # | Rule | Admits | Cumulative if all land |
|---|---|---:|---:|
| 1 | namespace_replay: Expr `str`/`dt`/`list`/`arr`/`bin`/`name`/`cat`/`meta` are non-Clone newtypes of `Expr`. Keep the Expr and replay the constructor per call; builders are take-once | 245 | 46.7% |
| 2 | trait_dispatch: `StringNameSpaceImpl`, `TemporalMethods` and similar, on wrapped implementors | 335 | 51.9% |
| 3 | protocols: `Hash` 165, `TrivialClone` 106 (likely a marker; reclassify), comparisons | 312 | 56.6% |
| 4 | generic_impls: operators, `From` and `Default` on generic owners over the dtype table | 732 | 67.8% |
| 5 | generic_methods: inherent/trait generics and free functions | 800 | 80.0% |
| 6 | internal_crates: widen the scope to arrow, utils, compute, row, parquet, then re-classify | 765 | 91.7% |
| 7 | owner_wrappers | 115 | 93.4% |
| 8 | callbacks | 107 | 95.1% |
| 9 | value_grammar: foreign/borrowed values, fixed arrays, outward conversions, `&mut`, arity (incl. `qcut`) | 302 | 99.7% |
| 10 | async | 22 | 100% |

Rules 1–6 are the path to 90%. Each is one record. Fixtures for the v2-only dtypes (172 cases) belong with rule 4 or 5, where those dtypes are instantiated.

## Checks run

- The generator `--self-test` passes, including the new method-arity test.
- `census.py --self-test` checks classification, denominators, and refusal of a stale surface or an extra entry. `batches.py --self-test` checks that every row gets exactly one rule. The 0074 identity controls run on import.
- Production `--check` on 0.55.2 passes; the regeneration differs only in the replaced control.
- Production 0.55.2 suites, debug last: `--release` exit 0, `--release --features test-support` exit 0, debug `--features generated,test-support` exit 0. The only change to the committed oracle-results.json was the row text of two approved row_order_differs cases, which varies per run; it was not committed.

## Review answers (Codex, reviews/0108_review_codex.md)

1. **Committed evidence.** `probes/0108/evidence/` (1.9 MB gzip, `-n` for reproducibility) holds the quoted v2 inventory, pins, surface, oracle results, census and batches, plus the two 0.55.2 baselines: the 0106 production surface and the adapter-narrow inventory. `digests.txt` pins every file. `probes/0108/verify.sh` decompresses the bundle, checks the digests, and recomputes the census and batch table from it. Both must match the committed files byte for byte. The replay needs nothing outside the commit. Making the replay byte-identical exposed one nondeterminism: the 0074 `delta` walks a set, so the order of the reshaped list varied between runs. The census now sorts it; the counts were unchanged.
2. **Pinned inputs.** `census.py` no longer reads the worktree adapter. The baseline surface comes from `git show 1eecbf1:adapters/polars/surface.json` or the bundle, and the baseline inventory from probe 0072 or the bundle. Both are sha256-pinned, and a changed input is refused (self-test). Before counting, the census checks that:
   - the inventory's rev and cfg are da47b74 and v2py;
   - its features are exactly `features.txt`;
   - the surface's recorded release hash is `v2.toml` (the replay pins the 0108 release hash, so a later edit to `v2.toml` cannot invalidate this evidence);
   - the surface counts reconcile.

   The census inputs' hashes are recorded in `census.json` under `inputs`.
3. **Both adapter configurations at v2.** `build.sh` now also builds and tests the scratch adapter with `--no-default-features` (the hand-written adapter alone). `PROBE_STAGE=no_default` runs that stage by itself on an existing scratch. Result: `no_default: ok`, 30 test binaries built with `--locked`, 9 tests passed and 0 failed (the generated-only tests are compiled out). The hand-written sources in the scratch are unchanged from 1ae609e; only generated files differ, and those are not compiled in this configuration.

Round two (Codex): the no-default stage recorded a failed test run but returned success. A failing or silent test command now calls `fail`: the stage is recorded, and the run exits nonzero in both the full and the `PROBE_STAGE=no_default` paths. `probes/0108/controls.sh` proves it with a fake cargo (`PROBE_CARGO`) and temporary outputs (`PROBE_OUT`): a failing run exits 1 with the failure in build-status.json, a run with no test result exits 1, and a passing run exits 0. No measured output changed, and `verify.sh` still reproduces byte for byte.
