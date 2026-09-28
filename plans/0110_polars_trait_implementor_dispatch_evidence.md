# rnx 0110 evidence: trait methods through proven implementors

The v2 figures come from `probes/0108/build.sh` (da47b74, the Python wheel's 145 features) and are committed in `probes/0110/evidence/`. `probes/0110/verify.sh` replays them against 0108's pinned baselines, and reproduces the census, batch table and target table byte for byte.

## 1. The extractor gap, repaired once

The 0072 extractor recorded a trait's implementors only while walking each reachable type's own impl list, in that type's crate. An impl lives in the crate that writes it, so `impl polars_ops::…::StringNameSpaceImpl for StringChunked` (written in polars_ops for a polars_core type) was never seen. That is why 285 of the 335 rows had an empty implementor list.

The extractor now also walks every impl item of every documented crate and joins it by trait identity (`docs.resolve` on the impl's trait) to the traits whose methods are already callables. This recovers 1,185 impls at v2 and 1,017 at 0.55.2 adapter-narrow.

No trait or callable is added. At both pins the callable identities (key, canonical path, bucket) are byte-identical to before, and only `implementors` and the supporting `impls` change. The 0108/0109 denominators therefore stand (8,389 full; 6,552 applicable). After the repair, 13 of the 335 rows have no impl record at all.

## 2. Dispatch by proof

`proven_trait_receivers` in the generator reads the trait's impl records:
- A **concrete head** resolves through its exact canonical identity. For example, `ChunkedArray<StringType>` resolves to the `StringChunked` wrapper through `by_identity`.
- A **generic head** (`impl<T: B> Tr for ChunkedArray<T>`, or a bare `T`) is tried on each alias wrapper of its base. It is admitted only when the existing `applicability` proof discharges the head and every impl bound and where clause. This is the proof inherent generic-owner methods use.
- **Blanket impls and generic traits** give no receiver.

Each candidate that fails keeps its reason on the refusal. A proven pair then goes through the unchanged `emit_method` path (arguments, return, arity, callbacks, borrowing, 0109 movement, engine routing), so a proven impl can still be refused by an independent check. There is no per-method release entry.

Self-test `trait-receivers` checks four things:
- a concrete impl gives its alias wrapper;
- a generic impl gives exactly the alias whose bound holds (`Int64`, not `Boolean`);
- a blanket impl gives nothing;
- a generic trait is refused with its reason.

**Fix found on the way.** The recovered impl records made `PlSmallStr` an admissible internal-type wrapper. That turned string returns into an opaque wrapper: 4 v2 cases went from `match` to `broken` and 1 to `mismatch` in the first run. Types the generator converts natively to Rune scalars (`SCALAR_MAPPED`: `String`, `PlSmallStr`) are now never wrapped. The same stray wrapper had reached the 0.55.2 production build (wrappers 275 → 274 after the fix). All five cases match again.

## 3. The 335 rows (`probes/0110/evidence/targets.json.gz`)

| class | rows |
|---|---:|
| available | **239** (366 bindings) |
| impl recorded, no proven wrapped receiver (bare-parameter heads such as `impl<T: …> Tr for T` that no wrapper satisfies, `BTreeMap`/`uN` receivers, mixed blanket rows) | 44 |
| proven receiver, refused by an independent check (arity, borrowed-slice arguments, `Self` not inferable, fixed-array shapes) | 21 |
| generic trait | 18 |
| no impl record | 13 |

## 4. v2 (the target)

| | 0109 | 0110 |
|---|---:|---:|
| applicable | 6,552 | 6,552 |
| available | 3,059 (46.7%) | **3,312 (50.5%)** |
| value-tested | 1,937 | **2,132 (32.5%)** |
| missing `owner_wrapper` | 450 | 190 |
| to 90% | 2,838 | 2,585 |

The gain is +253: the 239 trait rows, plus 14 other rows whose only blocker was an implementor list. For example, `generic_inference` falls from 1,532 to 1,527.

Oracle (first run; see the review round below for the final run): 3,533 cases, 3,061 matches, **0 mismatches and 0 broken**.
- The 3,006 cases shared with 0109 all hold. The only moves are three joins passing between `match` and `row_order_differs` under their approved unordered policy.
- The 527 new cases:

  | new cases | count |
  |---|---:|
  | match | 405 |
  | both_error | 64 |
  | fixture_failed | 48 |
  | both_panic | 9 |
  | row_order_differs | 1 |

  The one `row_order_differs` is `PolarsUpsample::upsample`, covered by a new cited unordered policy (below).
- The v2 no-default stage passes.

**New cited unordered policy (v2.toml).** `PolarsUpsample::upsample` with a non-empty `by`. polars-time da47b74 `src/upsample.rs:55-58` documents `upsample_stable` as `upsample` "but order of the DataFrame is maintained when `by` is specified". Both call `upsample_impl`, with `stable` false (line 107) or true (line 119). Two direct Rust runs returned different group orders, which the oracle reported as nondeterministic until the policy was recorded. This is an operation's documented contract, like the joins, not a binding gate.

## 5. 0.55.2 production

- Available (0077 scoreboard): 2,218 → **2,318**.
- Value-tested: 1,428 → **1,499**.
- Unsupported: 1,824 → 1,724.

The largest gains are `ListNameSpaceImpl` 19, `Logical` 14, `BinaryNameSpaceImpl` 10, `SchemaExt` 5, `NumOpsDispatch` 5, `ChunkAggSeries` 4, `SeriesMethods` 4, `ChunkUnique` 3 and `DataFrameJoinOps` 3.

The oracle grew from 2,489 to 2,802 cases (269 new matches), with no regression. `LazyFrame::unique` moved between approved row orders.

Suites, with debug last: `--release` exit 0; `--release --features test-support` exit 0; debug `--features generated,test-support` exit 0.

The focused test `tests/trait_dispatch.rs` passes 5 of 5:
- `ListNameSpaceImpl::lst_lengths`, `BinaryNameSpaceImpl::size_bytes` and `DataFrameJoinOps::inner_join` match direct Polars.
- `ChunkUnique::n_unique` dispatches separately on `StringChunked` and `BooleanChunked` and matches.
- The refusals keep a blanket-only reason, and no binding has a bare generic base as its receiver.

## 6. Formatting

rnx's `cargo fmt` had never been run on the Polars toolchain. Generated code is now skipped by rustfmt through markers the generator emits: `#[rustfmt::skip]` on the generated submodules in `generated/mod.rs` (not on the hand-written `support.rs`), and `#![cfg_attr(rustfmt, rustfmt::skip)]` in `tests/generated_oracle.rs`.

rustfmt's `ignore` option was tried first. It is unstable, and stable rustfmt 1.9.0 rejects it: "unstable features are only available in nightly channel".

`cargo fmt` was then run on the three crates this record touches: adapters/polars, tools/polars-gen and probes/0072/extract. The generator's `--check` still passes, so formatting changed no generated byte. The formatting lands in this impl commit, not a separate chore commit.

## 7. Launch

Measured with `probes/0073/launch.py`, 60 interleaved launches per set, old = 0109 (97b51f4), new = 0110. Budget +5 ms.

| set | old median | new median | delta |
|---|---:|---:|---:|
| 1 | 19.13 ms | 21.07 ms | +1.94 |
| 2 | 18.35 ms | 19.39 ms | +1.04 |
| 3 | 20.61 ms | 20.81 ms | +0.20 |

## Census tooling

`probes/0108/census.py` now reads its pinned baseline inventory from the 0108 bundle in live mode. Later records re-extract the local 0072 inventory (as this one did), and the census must not follow it. The 0108 and 0109 replays still pass.

## Review round 1 (Codex): the proof is the only source of receivers

**Finding.** Receivers were seeded from `c.implementors` whenever a label named a wrapper, then augmented with the proven ones. A generic trait, or a concrete impl with undecided bounds, could therefore still reach `emit_method` through its label.

**Fix.** The receiver list is now exactly `proven_trait_receivers`, and an implementor label alone admits nothing. The `trait-receivers` self-test now also runs the full `emit_callable` path with `implementors = [wrapped StringChunked]` on two traits, and requires both to be unsupported with their proof reasons:
- a generic trait;
- a trait whose only impl is concrete with a bound (`Self: Send`).

**Mutation check.** With the label seeding put back, the self-test fails ("a generic trait with a wrapped label must not emit"). The fix was then restored.

**Effect on real data.** No status changed at either pin, so the legacy path admitted nothing the proof rejects. Four callables list their receivers in proof order instead of label order:
- `ChunkNestingUtils::propagate_nulls`
- `ChunkNestingUtils::trim_lists_to_normalized_offsets`
- `IntoSeries::is_series`
- `Container::iter_chunks`

Their representative oracle cases therefore moved to another receiver: 4 case ids per surface. All 8 cases match. The rest is unchanged:
- v2: 3,533 cases, 3,060 matches, 0 mismatches, 0 broken. `left_join` moved to `row_order_differs`, which its approved policy permits.
- Production: 2,802 cases, no status change.

The bundle was regenerated from this run (`probes/0110/verify.sh` passes). The counts are unchanged: 3,312 of 6,552 applicable available at v2, 2,132 value-tested, and the same 239 / 44 / 21 / 18 / 13 target classes. All three production suites pass, debug last.

The no-op loop in the extractor (`let tk …; let _ = tk`) was removed. The extractor output is unchanged.
