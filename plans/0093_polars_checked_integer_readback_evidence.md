# 0093 checked integer read-back: evidence

Plan `ac0f83c`, amended in this commit with the bounded allowlist agreed with Codex. Every `u64`, `usize`, `isize`, `i128` and `u128` that Polars hands to a script is now range-checked. A value outside `i64` is a `ConversionError` naming the method, never a wrapped negative. Only seven source-proven identities keep a plain integer. Operation counts are unchanged; 164 binding signatures became fallible.

## Gate 1: census

`probes/0093/census.py` compares the closed 0092 generated bindings with the current ones and classifies each read-back. `probes/0093/readback.json` holds every row with its binding, Polars path, source type, method, route and signature before and after.

| Class | Sites |
|---|---:|
| Newly checked (`support::widen`) | 193 |
| Checked before this record (0087 `chunk_lengths`, 0089 arg extrema) | 32 |
| Bounded allowlist (`support::bounded_usize`) | 8 |
| Total `u64`/`usize` read-back sites | 233 |

Newly checked sites by route:

| Route | Sites |
|---|---:|
| Direct return | 88 |
| Option | 81 |
| Tuple field | 12 |
| Iterator item | 5 |
| Callback input | 4 |
| Slice element | 3 |

By source type, 164 are `usize` and 29 are `u64`. No `isize`, `i128` or `u128` read-back is emitted in the pinned build; the generator checks them anyway. The 141 remaining `__r as i64` sites all convert `i8`, `i16`, `i32`, `u8`, `u16`, `u32` or `IdxSize`. `IdxSize` is `u32` without `bigidx`, and `support.rs` asserts that at compile time. The ten sites whose binding also mentions `u64` or `usize` were checked by hand. Eight are `get` on narrow arrays, where only the index argument is `usize`. Two are `CategoricalMapping` methods that return `CatSize` (`u32`) and take a `u64` hash argument. Hand-written adapter code (`values.rs`, `oracle.rs`, `preview.rs`) has no integer read-back into script values.

**Value trace from 0092.** `5 - [u64::MAX, 2^32 + 1, 1]` on `UInt64` wraps in Polars to `[6, 18446744069414584324, 4]`. Before this record the script read the middle value as `-4294967292`. It now reads it as a `ConversionError` with the message `get: 18446744069414584324 does not fit a script integer`. `tests/lhs_sub.rs` asserts both the Rust value and the script result.

## Why lengths are not assumed bounded

Codex asked for a strict allowlist rather than an assumption about practical memory, and the source supports that. `ChunkedArray::length` is a `usize` (polars-core 0.55.2 `chunked_array/mod.rs:143`). `append` and `append_owned` only `checked_add` it (`chunked_array/ops/append.rs:151-161`, repeated through line 280); nothing compares it with `IdxSize` or `isize::MAX`. Appended chunks share their Arrow buffers, so appending an array to itself doubles its length and costs only the chunk references. Take one 4 GiB `UInt8` chunk of 2^32 elements. Repeated self-appends reach 2^31 references of 16 bytes each, 32 GiB, and the length is then 2^63, one above `i64::MAX`. That is about 36 GiB in total, and no Polars check fails. So `len`, `null_count`, `height`, `shape`, array indices and every other unproven count use the checked conversion and return a `Result`.

## The bounded allowlist

`[[bounded_readbacks]]` in `tools/polars-gen/releases/0.55.2-joins.toml` lists inventory key, canonical path and citation. A read-back keeps a plain integer only when both key and path match. Each value below is the length of, or an index into, a `Vec` or `IndexMap`, so it is at most `isize::MAX`.

| Identity | Key | Source, polars-core 0.55.2 |
|---|---|---|
| `DataFrame::width` | `polars_core:7539` | `frame/dataframe.rs:231-233`, `self.columns.len()` |
| `DataFrame::get_column_index` | `polars_core:7620` | `frame/mod.rs:1065-1074`, `IndexMap::get_index_of` or a position in the columns |
| `DataFrame::try_get_column_index` | `polars_core:7621` | `frame/mod.rs:1076-1079`, delegates to `get_column_index` |
| `SeriesTrait::n_chunks` | `polars_core:11119` | `series/series_trait.rs:245-247`, `self.chunks().len()`; the only override (`series/implementations/extension.rs:271-273`) delegates to the storage series |
| `Column::n_chunks` | `polars_core:7348` | `frame/column/mod.rs:1833-1838`, the series' `n_chunks`, or 1 for a scalar column |
| `DataFrame::max_n_chunks` | `polars_core:7599` | `frame/mod.rs:451-457`, the maximum column `n_chunks`, or 1, or 0 when empty |
| `DataFrame::first_col_n_chunks` | `polars_core:7598` | `frame/mod.rs:442-448`, the first series column's `n_chunks`, else 0 or 1 |

`SeriesTrait::n_chunks` is emitted twice, so the seven identities give eight sites. `support::bounded_usize` converts them. A compile-time assertion requires `usize` to be at most 64 bits, and a debug assertion rechecks the bound at run time in tests.

## Gate 2: fallible conversion throughout

`World::ret` converts every risky source type with `support::widen::<T>(__r, "method")?`. The fallible bit reaches `Option`, tuples, vectors, copied slices and materialized iterators, so each affected binding returns a `Result`. The old scoped `widen_usize` flag is gone.

In `callback_input`, a scalar, optional or collection argument that fails the check calls `support::callback::unwind` with a `CallbackFailure` naming the operation. This happens before the script runs. The script sees a `CallbackError` at the routed call, the guard is restored, and the receiver is unchanged. A collection whose element conversion is fallible but not a recognized checked conversion is refused.

`checked-readback self-test` builds a synthetic inventory and asserts these cases:

- The listed `DataFrame::width` keeps a plain `i64` through `bounded_usize`.
- The same path under another key stays checked. So does a `width` on another owner, which is the unlisted same-named control.
- `height`, a `u64` hash, an `Option<u64>`, a `(usize, usize)` tuple with both fields checked, and an `isize` each become `Result`. Each uses `support::widen` named by its method, with no `as i64` or `bounded_usize` left and no unsubstituted operation placeholder.
- A `u32` keeps its lossless cast.
- `u64`, `usize` and `&[u64]` callback inputs are checked and fail through the typed unwind.

The support unit test covers the synthetic extremes that cannot be allocated: `usize::MAX`, `u64::MAX`, `i128` below `i64::MIN`, `u128::MAX`, and `i64::MAX` exactly. A debug-only test shows that `bounded_usize(usize::MAX)` trips its assertion.

No real unlisted callable shares a name with a listed one in the emitted set. For example, `ChunkedArray` has no `n_chunks` binding. So the same-name control is synthetic.

## Gate 3: an oracle that can tell

The oracle's Rust side formats a risky source value with the same check (`i64::try_from`), marking an out-of-range value as a `ConversionError`. It previously used `as i64`, which wrapped exactly as the binding did and hid the defect. Three runner controls on the `series_u64_extremes` fixture pass:

| Control | Expected | Result |
|---|---|---|
| `first()` of `u64::MAX` | both sides error | both_error |
| A wrapping Rust formatter against the checked binding | mismatch | mismatch |
| `last()` of an in-range value | match | match |

Oracle: 2,234 cases, all verified, none new. The new `series_u64_boundary` fixture feeds no producer. Tally of the committed results file: 2,117 match, 98 both_error, 17 both_panic, 2 row_order_differs. That file comes from the debug all-features run, like 0092's. A release run turns three both_panic cases into matches, because the Polars assertions behind them are debug-only; that is a build-profile difference, not this record. The only status movement is `LazyFrame::unique_generic`, from match to row_order_differs, the known run-to-run permutation. `LazyFrame::unique` also returned its rows in another order, still within its unordered policy.

## Gate 4: behaviour

`tests/checked_readback.rs` runs one script against `series_u64_boundary`, a Rust fixture holding `[i64::MAX, i64::MAX + 1, u64::MAX, null]`:

| Route | Result |
|---|---|
| `get` at `i64::MAX` | `Some(9223372036854775807)` |
| `get` at `i64::MAX + 1` and `u64::MAX` | `ConversionError get: <value> does not fit a script integer` |
| `get` of the null | `None` |
| `first`, `last` | `Some(i64::MAX)`, `None` |
| `iter`, `to_vec` | `ConversionError` at `i64::MAX + 1` |
| `cont_slice` on the extremes fixture | `ConversionError` at `u64::MAX` |
| `cont_slice` and `iter` on in-range data | values, unchanged |
| `for_each` (sink), `apply_mut` (mutating) | `CallbackError` naming the operation and the value |
| After the failure | receiver unchanged, in-range `for_each` and `apply_mut` succeed, `apply_mut(v + 10)` gives 13 |
| `width()`, `height()` | plain `3`, `Result` `3` |

Scripts and docs updated for the new `Result` returns:

- 13 hand-written test files, plus the regenerated oracle test.
- `tests/generated_e2e.rn`.
- The adapter README, which gains a section with examples.
- The generator README.

The 0079 probe scripts use that probe's own bindings and are unchanged.

Scoreboard, unchanged from 0092 because no operation was added or removed:

| Measure | 0092 | 0093 |
|---|---:|---:|
| Available to a script | 2,117 | 2,117 |
| Value-tested | 1,344 | 1,344 |
| Unsupported | 1,926 | 1,926 |
| Binding signatures made fallible | | 164 |

## Gate 5: cost

All of these passed:

- The generator self-test.
- The drift check (`--check`).
- The debug suite with `generated,test-support`: 95 tests.
- Both release configurations from the README, with fail-fast disabled: 9 tests with default features and 94 plus 1 ignored with `test-support`.
- `git diff --check`.

The generator now also fails closed on a `[[bounded_readbacks]]` entry with an empty citation.

The retained 0092 binary's SHA-256 is `566f03bfe580dc4e5dd888b6fa06885581a464b563000ccb93cc3f2479eb4803`, matching its record, and the 0093 binary's is `455cb5be71c2fb120ad0faf50ea4654947a98da08f0820dc9c52baa9c5413650`. Cold launch was measured in three interleaved 60-run sets against a budget of +5 ms:

| Set | 0092 median | 0093 median | Difference | Raw samples |
|---|---:|---:|---:|---|
| 1 | 17.89 ms | 17.86 ms | −0.03 ms | `probes/0093/launch-results-1.json` |
| 2 | 19.28 ms | 18.85 ms | −0.43 ms | `probes/0093/launch-results-2.json` |
| 3 | 15.66 ms | 16.21 ms | +0.55 ms | `probes/0093/launch-results-3.json` |

## Remaining

`hash()` and `cat_to_hash()` values spread over the whole `u64` range, so about half are now a `ConversionError`. `get_cat_with_hash` and `insert_cat_with_hash` take only hashes a script can spell. A bit-preserving hash representation would be its own record. The remaining function-generic and Arrow pools stay open.
