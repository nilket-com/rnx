# 0101 owned snapshots from downcast_get: evidence

Plan `b4dc3b1`. `ChunkedArray::downcast_get(idx)` binds on fourteen wrappers: the ten numeric ones, Boolean, String, Binary and BinaryOffset. It returns chunk `idx` as an owned `Option<Vec<Option<script value>>>`. There are 14 bindings and 14 new oracle cases, all matching. List and Struct stay refused. The refused proven-pair pool falls from 154 to 140. The operation adds one distinct available and value-tested operation. The 564 unresolved pairs are unchanged.

## Gate 1: pairs and source

The inventory records `pub fn downcast_get(&self, idx: usize) -> Option<&T::Array>` on `impl<T: PolarsDataType> ChunkedArray<T>`, with no where-clauses or method generics. The source, polars-core 0.55.2 `chunked_array/ops/downcast.rs:104-110`, is `self.chunks.get(idx)?` followed by Polars's own cast to `T::Array`. `idx` is a chunk index, and an absent chunk is `None`. The adapter adds no cast of its own; it copies the typed array Polars returns.

`probes/0101/pairs.json` lists all 16 pairs, each with its substituted `T::Array` from the old refusal (for example `PrimitiveArray<i8>`, `BinaryViewArrayGeneric<str>` or `BinaryArray<i64>`). It also records the disposition before and after, the decision and the oracle status. 14 pairs are emitted. List (`ListArray<i64>`) and Struct stay refused as not listed: their cells are arrays or rows of fields and need a nested-value policy.

**Chunk index versus row index**, from direct Rust and the script alike, on `[1, 2, 3] ++ [1, null, 3]`:

| Call | Result |
|---|---|
| `downcast_get(0)` | `[1, 2, 3]` |
| `downcast_get(1)` | `[1, null, 3]`, the whole second chunk |
| `downcast_get(2)` | `None` |
| empty receiver, `downcast_get(0)` | `Some([])` |
| empty receiver, `downcast_get(1)` | `None` |

The retained 0100 binary's SHA-256 is `e0d2d7ecc3303d02a30e2059b58c62ad2dd36ce2ece69e57d8fc156292c02884`.

## Gate 2: only this indexed return

The release file has one `[[indexed_chunk_snapshots]]` entry: key `polars_core:3509`, the path, fourteen pairs from the 0099 and 0100 tables, and a citation. `IndexedChunkSnapshot::check` requires all of the following, and names the fault otherwise:
- `ChunkedArray<T>` with impl bounds exactly `T: PolarsDataType` and no where-clauses
- `&self`
- exactly one `usize` parameter and no method generics
- a return of exactly `core::option::Option<&T::Array>`
- pairs from `NUMERIC_NATIVES` or `SCALAR_CHUNKS`, where an owner carries its own kind, without duplicates

A double listing is refused. In `emit_instantiations`, a malformed entry or an unlisted pair is refused by name. A listed pair sets `World.indexed_chunk` to (method, kind).

With the scope set, `World::ret` accepts only a top-level `Option<&X>` in which `X` is exactly the pair's concrete array: `PrimitiveArray<N>`, `BooleanArray`, `BinaryViewArrayGeneric<str>`, `BinaryViewArrayGeneric<[u8]>` or `BinaryArray<i64>`. Numeric pairs map to `support::indexed_snapshot::<N, _>` with the scalar rule, `u64` checked. Scalar kinds map to `support::indexed_snapshot_{bool,str,binview,binary_offset}`. The emitter refuses a routed binding and any other conversion. The index goes through the existing `support::narrow::<usize>`, so a negative index is refused before Polars is called.

The copiers:
- **`None`.** Copy nothing, under any bound.
- **Numeric.** Bound one chunk slot plus its cells with `snapshot_budget(1, cells)` from 0099, before allocation, then re-count while copying.
- **Payload.** `payload_one` sums the selected chunk's payload bytes through `payload_count_step` from 0100. It bounds 1 + cells + bytes with `payload_snapshot_budget`, then re-counts while copying.

Only the selected chunk is examined; no other chunk is walked or copied.

Controls:

- **`indexed-chunk self-test`.** The cited shape passes. Ten malformed shapes fail with the fault named: a blank citation, an owned receiver, a method generic, no parameter, an `i64` index, a non-optional `&T::Array` return, a `PolarsResult` return, a mispaired kind, List and a duplicate pair. A double listing fails. In the emitter, the `i8` pair uses the cast rule, the `u64` pair uses `widen`, and the `str` pair uses its copier, each with the checked index. Another array, a nested `Option<Option<&..>>`, a non-optional `&PrimitiveArray`, an unscoped return, and a string array under an `i8` scope are all unsupported with no text.
- **`indexed_chunk_drift_is_refused_by_name`, on the real generator.** Four runs:
  - A blank citation refuses all 16 pairs by name with no text.
  - A String pair given the binary kind does the same.
  - Without the Int64 pair, 13 bind and Int64 is named.
  - An inventory whose `downcast_get` returns `&T::Array` refuses all 16 as `return &T::Array is not …`.
- **Support unit test `indexed_snapshots_copy_one_chunk_under_the_bound`.** It checks the numeric copy, `None`, and a present empty chunk as `Some([])`. At a bound of 3, a 1 + 3 = 4-slot chunk is refused with `m: 1 chunks and 3 cells (4 slots), more than the bound of 3`, while `None` still returns `None`; at 4 the chunk is copied. The string chunk `["é日本", "", null]` costs 1 + 3 + 8 = 12 slots: refused at 11, copied at 12. It also covers binary views, offset binary and Boolean, present and absent.

## Gate 3: values and limits

`tests/indexed_chunk_snapshots.rs` compares the script with direct Rust `downcast_get` for all fourteen owners. For each it checks indices 0, 1 and 2 on a two-chunk receiver, and 0 and 1 on an empty receiver. It checks a result kept after its receiver is dropped, and that index −1 gives `ConversionError: idx: -1 is out of range for usize`. The receivers are:
- the numeric and Boolean ones: `[1, 2, 3] ++ [1, null, 3]` cast to the type
- String: `series_str_mixed`
- Binary and BinaryOffset: `series_binary_mixed` and `series_binary_offset_mixed`

Three rows are spelled out in the test:

| Owner | Result |
|---|---|
| Int64 | `3[1,2,3] 3[1,n,3] None 0[] None 3[1,n,3] ConversionError` |
| String | `2["é日本",""] 2[n,"z"] None 0[] None …` |
| Binary | `2[<195 40>,<>] 2[n,<0 0 255>] None 0[] None …` |

Extremes:
- Int8 gives `[-128, 127, 0]`.
- Float32 gives `[0.10000000149011612, -0.0]`.
- Float64 gives `[-0.0, NaN, inf, -inf]`.
- `series_u64_boundary`, chunk 0, gives `ConversionError: downcast_get: 9223372036854775808 does not fit a script integer` with no partial result. The receiver's `get(0)` then returns `i64::MAX`, and its chunk 1 is `None`.

**Only the selected chunk is bounded.** The two-chunk Int64 receiver costs 8 slots whole, and its chunk 1 costs 4:

| Bound | Call | Result |
|---:|---|---|
| 3 | `downcast_get(1)` | refused: `downcast_get: 1 chunks and 3 cells (4 slots), more than the bound of 3` |
| 4 | `downcast_get(1)` | `[1, n, 3]` |
| 4 | `chunks()` on the same receiver | refused: `8 slots, more than the bound of 4` |
| 10 | string chunk 0 (1 + 2 + 8 = 11) | refused |
| 11 | string chunk 0 | copied |
| 1 | absent index 5 | `None` |

The receivers are reused after each refusal.

## Gate 4: reconcile and cost

**Oracle.** The Rust side frames a `downcast_get` result as the owned optional chunk: `__r.map(|a| a.iter()…collect())`, formatted as `Option<Vec<Option<_>>>`. There are 14 new cases, one per binding, all matching. The standard argument is index 2 and the typed fixtures have one chunk, so every oracle case returns `None` on both sides. The direct tests above cover present chunks, empty chunks, values and bounds. Among old cases only `LazyFrame::unique_generic` moved, from row_order_differs to match, which is the known permutation. The oracle has 2,369 cases, all verified. Tally of the committed debug-profile results file: 2,253 match, 98 both_error, 17 both_panic, 1 row_order_differs.

| Measure | 0100 | 0101 |
|---|---:|---:|
| Available to a script | 2,130 | 2,131 |
| Value-tested | 1,374 | 1,375 |
| Unsupported | 1,913 | 1,912 |
| Bindings (new) | | +14 |
| Oracle cases | 2,355 | 2,369 |
| Refused proven pairs | 154 | 140 |
| Unresolved pairs | 564 | 564 |

**Verification.** All of these passed:

- The generator self-test: 24 suites.
- The drift check (`--check`).
- The debug suite with `generated,test-support`: 131 tests plus 1 ignored.
- Both release configurations from the README, with fail-fast disabled: 9 tests with default features and 130 plus 1 ignored with `test-support`.
- `git diff --check`.

**Launch.** Cold launch was measured on the reviewed binary, SHA-256 `be75c913f6a1ff30548821524a56e9fca1cb72fd40a3a21430e270b457c983e1`, in three interleaved 60-run sets against a budget of +5 ms. At close, Codex's nonblocking note was folded in: four unused `polars_arrow::array::Array` imports were removed from the new support copiers. The rebuilt binary's SHA-256 is `b331150e419d757cda709cbfee32d4bdda3859f64de979c0a4f63fe31986bcd3`, and launch was not remeasured on it. The cleanup changes no code path. The drift check, the indexed unit test and `tests/indexed_chunk_snapshots.rs` pass on it, and the build has no warnings.

| Set | 0100 median | 0101 median | Difference | Raw samples |
|---|---:|---:|---:|---|
| 1 | 17.28 ms | 17.03 ms | −0.25 ms | `probes/0101/launch-results-1.json` |
| 2 | 17.74 ms | 17.76 ms | +0.02 ms | `probes/0101/launch-results-2.json` |
| 3 | 17.43 ms | 16.31 ms | −1.12 ms | `probes/0101/launch-results-3.json` |

## Remaining

`downcast_iter`, `downcast_as_array`, `downcast_chunks` and the other Arrow-return methods, List and Struct values, and the function-generic and callback pools keep their dispositions. The refused proven pool is 140 and the unresolved pool is 564.
