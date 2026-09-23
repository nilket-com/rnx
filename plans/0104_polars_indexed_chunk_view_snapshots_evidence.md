# 0104 owned snapshots from downcast_chunks: evidence

Plan `a706169`. `ChunkedArray::downcast_chunks()` binds on the fourteen numeric, Boolean, String, Binary and BinaryOffset wrappers. Polars's indexed `Chunks` view is read in ascending order into an owned `Vec<Vec<Option<script value>>>`. There are 14 bindings and 14 new oracle cases, all matching. List and Struct stay refused. The refused proven-pair pool falls from 112 to 98. The operation adds one distinct available and value-tested operation. The 564 unresolved pairs are unchanged.

## Gate 1: pairs and the view

The inventory records `pub fn downcast_chunks(&self) -> Chunks<'_, T::Array>` on `impl<T: PolarsDataType> ChunkedArray<T>`, with no parameters, method generics or where-clauses; its canonical return is `polars_core::chunked_array::ops::downcast::Chunks<T::Array>`. In polars-core 0.55.2, `chunked_array/ops/downcast.rs:10-47` defines `Chunks` as a borrow of `&[ArrayRef]`, with `get(index) -> Option<&T>` (a cast under `T::Array`) and `len()`. Lines 99-102 define `downcast_chunks` as `Chunks::new(&self.chunks)`.

`probes/0104/pairs.json` lists all 16 pairs, each with its view element taken from the old refusal (for example `Chunks<BinaryArray<i64>>`). It also records the disposition before and after, the decision and the oracle status. 14 pairs are emitted. List and Struct stay refused as not listed.

**View probe.** Direct Rust and the script agree:

| Receiver | `len()` | `get(i)` |
|---|---:|---|
| `[1, 2, 3] ++ [1, null, 3]` | 2 | `get(0)` = `[1,2,3]`, `get(1)` = `[1,null,3]`, `get(2)` = `None` |
| empty receiver | 1 | `get(0)` = `[]` |

The support unit test covers a middle empty chunk, a shorter view, an absent in-range `get` and a changed shape. The receiver is reused after each call.

The retained 0103 binary's SHA-256 is `6eff4543390e7c37b56da7b258f57ddd0515128d244411c02a5c14817983f296`.

## Gate 2: only the cited return

The release file has one `[[view_snapshots]]` entry: key `polars_core:3508`, the path, fourteen pairs from the 0099 and 0100 tables, and a citation. `ViewSnapshot::check` requires no parameters and a return whose exact canonical text, whitespace-normalised, is `polars_core::chunked_array::ops::downcast::Chunks<T::Array>`. It then applies 0101's checks: the citation; `ChunkedArray<T: PolarsDataType>` with no where-clauses; `&self` with no generics; and pairs from the fixed tables, each owner with its own kind, without duplicates. A double listing is refused.

In `emit_instantiations`, a malformed entry or an unlisted pair is refused by name, and a listed pair sets `World.view_snapshot`. With it set, `World::ret` accepts only `Chunks<X>` at the top level, where the parsed argument `X` equals the pair's array by `Ty` equality rather than by `render`. The emitter refuses a routed binding and any other conversion.

The binding calls `<Owner>::downcast_chunks(&this.0)` once. It then calls `support::view_snapshot::<N, _>` or `support::view_snapshot_{bool,str,binview,binary_offset}` with `this.0.chunks()`, `__r.len()`, `|__i| __r.get(__i)` and the method name. Polars's `Chunks` type is never named by the adapter and never reaches the script.

The copiers:
1. Preflight the same receiver's chunks as in 0103: typed downcasts, checked cells and bytes, one inclusive bound, and no allocation proportional to the input.
2. Refuse a view whose `len()` differs from the preflight chunk count: `…: the view has 2 chunks, 3 were counted`.
3. Read `get(0..len)` in order. An in-range `None` is `…: the view has no chunk 1 of 3`.
4. Copy through 0103's `iter_copy`, now generalised to fallible items, which checks each chunk's length against its preflight chunk and requires the final slot count to equal the preflight total.

Nothing partial is returned. The 0099 to 0103 methods keep their budgets and mappings. Their copiers pass their items as `Ok`, and all their tests still pass.

Controls:

- **`view-snapshot self-test`.** The cited shape passes. Eight malformed shapes fail with the fault named: a blank citation, an owned receiver, a parameter, `Chunks<T>`, an optional view, a String-as-binary pair, Struct and a duplicate pair. A double listing fails. In the emitter, the `i8` and `str` pairs call `downcast_chunks(&this.0)` exactly once and pass the view's `len` and `get`. `Chunks` of another array, an optional view, a bare array, an unscoped view and a string view under an `i8` scope are unsupported with no text.
- **`view_snapshot_drift_is_refused_by_name`, on the real generator.** Four runs:
  - A blank citation refuses all 16 pairs by name with no text.
  - A Float32 pair given `f64` does the same.
  - Without the Int64 pair, 13 bind and Int64 is named.
  - An inventory whose view element becomes `Chunks<T>` refuses all 16 by name with no text.
- **Support unit test `view_snapshots_check_the_view_against_the_preflight`.**
  - `[a, empty, a]` is read in ascending order with the middle empty chunk kept.
  - A view reporting 2 of 3 chunks is refused.
  - An absent in-range `get(1)` is refused.
  - A shorter chunk at index 2 gives `m: chunk 2 has 1 cells, 2 were counted`.
  - 3 + 4 = 7 slots are refused at 6 and accepted at 7.
  - 50 empty chunks are refused at 49 by their outer slots alone.
  - A string chunk read as numeric is a typed preflight error.
  - Fewer payload bytes than counted gives `m: 3 slots copied, 5 were counted`.

## Gate 3: values and the whole-view bound

`tests/view_snapshots.rs` is 0103's test with `downcast_chunks` in place of `downcast_iter`. The Rust reference reads the view by `len()` and `get(i)`, and asserts `get(len)` is `None`. Across all fourteen owners it checks a two-chunk receiver, the empty receiver `1[]`, a result kept after its receiver is dropped, and a re-read. Three rows are spelled out, and they are identical to 0103's: Int64 `2[1,2,3|1,n,3]`, String `2["é日本",""|n,"z"]` and BinaryOffset `2[<195 40>,<>|n,<0 0 255>]`.

Extremes match as well: Int8, Float32 `0.1` rounding, and Float64 NaN, infinities and negative zero. `series_u64_boundary` gives `ConversionError: downcast_chunks: 9223372036854775808 does not fit a script integer`, and the receiver stays reusable.

The two-chunk Int64 view costs 8 slots. At a bound of 7 it is refused with `downcast_chunks: 2 chunks and 6 cells (8 slots), more than the bound of 7`, while its single chunk still fits through `downcast_get(1)`; at 8 it is copied. The string view costs 15 slots: refused at 14, copied at 15.

## Gate 4: reconcile and cost

**Oracle.** The Rust side frames a `downcast_chunks` view as `(0..len).map(|i| get(i)….collect())` over `Vec<Vec<Option<_>>>`. There are 14 new cases, one per binding, all matching. Among old cases only `LazyFrame::unique` moved, from row_order_differs to match, which is the known permutation. The oracle has 2,411 cases, all verified. Tally of the committed debug-profile results file: 2,295 match, 98 both_error, 17 both_panic, 1 row_order_differs.

| Measure | 0103 | 0104 |
|---|---:|---:|
| Available to a script | 2,133 | 2,134 |
| Value-tested | 1,377 | 1,378 |
| Unsupported | 1,910 | 1,909 |
| Bindings (new) | | +14 |
| Oracle cases | 2,397 | 2,411 |
| Refused proven pairs | 112 | 98 |
| Unresolved pairs | 564 | 564 |

**Verification.** All of these passed:

- The generator self-test: 27 suites.
- The drift check (`--check`).
- The debug suite with `generated,test-support`: 147 tests plus 1 ignored.
- Both release configurations from the README, with fail-fast disabled: 9 tests with default features and 146 plus 1 ignored with `test-support`.
- `git diff --check`.

The release build has no warnings.

**Launch.** The 0104 binary's SHA-256 is `621d4853fa881ea504d48d40683989629317adcaff93ec9976c80d9754937ec5`. Cold launch was measured in three interleaved 60-run sets against a budget of +5 ms:

| Set | 0103 median | 0104 median | Difference | Raw samples |
|---|---:|---:|---:|---|
| 1 | 18.96 ms | 16.88 ms | −2.08 ms | `probes/0104/launch-results-1.json` |
| 2 | 16.53 ms | 16.37 ms | −0.16 ms | `probes/0104/launch-results-2.json` |
| 3 | 18.01 ms | 17.58 ms | −0.43 ms | `probes/0104/launch-results-3.json` |

## Remaining

`downcast_into_iter`, `layout`, the Arrow-array input methods, List and Struct nested arrays, and the function-generic pool keep their dispositions. The refused proven pool is 98 and the unresolved pool is 564.
