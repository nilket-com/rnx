# 0102 owned snapshots from downcast_as_array: evidence

Plan `adeeca3`. `ChunkedArray::downcast_as_array()` binds on the fourteen numeric, Boolean, String, Binary and BinaryOffset wrappers. It returns the receiver's single array as an owned `Vec<Option<script value>>`. There are 14 bindings and 14 new oracle cases, all matching real values. List and Struct stay refused. The refused proven-pair pool falls from 140 to 126. The operation adds one distinct available and value-tested operation. The 564 unresolved pairs are unchanged.

## Gate 1: pairs, source and the panic boundary

The inventory records `pub fn downcast_as_array(&self) -> &T::Array` on `impl<T: PolarsDataType> ChunkedArray<T>`, with no parameters, method generics or where-clauses. The source, polars-core 0.55.2 `chunked_array/ops/downcast.rs:112-118`, is `assert_eq!(self.chunks.len(), 1); self.downcast_get(0).unwrap()`.

`probes/0102/pairs.json` lists all 16 pairs, each with its substituted `T::Array` taken from the old refusal. It also records the disposition before and after, the decision and the oracle status. 14 pairs are emitted. List and Struct stay refused as not listed, since their values are nested.

The panic boundary, observed:

| Receiver | Result |
|---|---|
| one chunk | returns that array |
| empty receiver (one empty chunk) | `[]` |
| two chunks, in direct Rust and in the script | Polars panics with ``assertion `left == right` failed``, `left: 2`, `right: 1` |
| zero chunks, from `ChunkedArray::from_chunk_iter` with no arrays | panics the same way, with `left: 0` |

A zero-chunk array is reachable in Rust through the safe public `ChunkedArray::from_chunk_iter`, but no script path constructs one.

The retained 0101 binary's SHA-256 is `b331150e419d757cda709cbfee32d4bdda3859f64de979c0a4f63fe31986bcd3`.

## Gate 2: only the cited single-array return

The release file has one `[[array_snapshots]]` entry: key `polars_core:3510`, the path, fourteen pairs from the 0099 and 0100 tables, and a citation. `ArraySnapshot::check` requires no parameters and a return of exactly `&T::Array`. It then applies 0101's checks: the citation; `ChunkedArray<T>` with exactly `T: PolarsDataType` and no where-clauses; `&self` and no method generics; and pairs from the fixed tables with each owner carrying its own kind, without duplicates. A double listing is refused.

In `emit_instantiations`, a malformed entry or an unlisted pair is refused by name. A listed pair sets `World.array_snapshot`. With it set, `World::ret` accepts only a top-level `&X` in which `X` is exactly the pair's concrete array. It maps numeric pairs to `support::array_snapshot::<N, _>` with the scalar rule, `u64` checked, and scalar kinds to `support::array_snapshot_{bool,str,binview,binary_offset}`. The emitter refuses a routed binding and any other conversion.

The binding calls Polars's `downcast_as_array` once, so the one-chunk assertion runs first and is kept. It then copies the borrowed array before returning. There is no second optional layer and no adapter unwrap or cast.

**Shared copiers.** 0101's indexed copiers were refactored to delegate to these single-array cores: `indexed_snapshot*` is now `a.map(array_snapshot*).transpose()`. So both methods share one bounded copy: 1 chunk slot plus cells, plus payload bytes through `payload_count_step`, checked before allocation and re-counted while copying.

Controls:

- **`array-snapshot self-test`.** The cited shape passes. Ten malformed shapes fail with the fault named: a blank citation, an owned receiver, a method generic, a where-clause, a parameter, an `Option<&T::Array>` return, a `PolarsResult` return, a mispaired kind, Struct and a duplicate pair. A double listing fails. In the emitter, the `i8`, `u64` (via `widen`) and `binary` pairs produce one vector, with no leading optional layer. Another array, an `Option<&..>`, an owned array, an unscoped return, and a binary array under a `str` scope are unsupported with no text.
- **`array_snapshot_drift_is_refused_by_name`, on the real generator.** Four runs:
  - A blank citation refuses all 16 pairs by name with no text.
  - A Boolean pair given `u8` does the same.
  - Without the Int64 pair, 13 bind and Int64 is named.
  - An inventory whose `downcast_as_array` returns `Option<&T::Array>` refuses all 16 by name.
- **Support unit test `array_snapshots_copy_one_array_under_the_bound`.** An empty array costs one slot and passes at 1. `[1, null]` is refused at 2 with `m: 1 chunks and 2 cells (3 slots), more than the bound of 2`, and passes at 3. Three 2-byte binary values, each small alone, total 1 + 3 + 6 = 10 slots: refused at 9, copied at 10. A byte-sum overflow is refused without a large allocation. 0101's indexed tests still pass on the delegating copiers.

## Gate 3: values, ownership, panic and budget

`tests/array_snapshots.rs` compares the script with direct Rust `downcast_as_array` for all fourteen owners. Each is checked on a one-chunk receiver, on its empty receiver, and on a result kept after its receiver is dropped. The receivers are:
- numeric and Boolean: `([1, 2, 3] ++ [1, null, 3]).rechunk()` cast to the type
- String: `series_str_mixed` rechunked
- Binary and BinaryOffset: the mixed binary fixtures, rechunked

Three rows are spelled out in the test:

| Owner | Result |
|---|---|
| Int64 | `6[1,2,3,1,n,3] 0[] 6[1,2,3,1,n,3]` |
| String | `4["é日本","",n,"z"] 0[] …` |
| BinaryOffset | `4[<195 40>,<>,n,<0 0 255>] 0[] …` |

Extremes:
- Int8 gives `[-128, 127, 0]`.
- Float32 gives `[0.10000000149011612, -0.0]`.
- Float64 gives `[-0.0, NaN, inf, -inf]`.
- `series_u64_boundary` gives `ConversionError: downcast_as_array: 9223372036854775808 does not fit a script integer`, with no partial vector. The receiver's `get(0)` then returns `i64::MAX`.

The multi-chunk test catches the panic with `catch_unwind` on both sides. Both report Polars's `assertion \`left == right\` failed` with `left: 2` and `right: 1`. Neither returns chunk zero or a `MaterializeLimit`.

Bounds:

| Receiver | Cost | Refused at | Copied at |
|---|---:|---:|---:|
| one-chunk `[1, 2, 3, 1, null, 3]` | 7 | 6 (`downcast_as_array: 1 chunks and 6 cells (7 slots), more than the bound of 6`) | 7 |
| mixed binary, no value above 3 bytes | 1 + 4 + 5 = 10 | 9 (`… 1 chunks, 4 cells and 5 bytes (10 slots) …`) | 10 |
| empty array | 1 | | 1 |

## Gate 4: reconcile and cost

**Oracle.** The Rust side frames a `downcast_as_array` result as the owned vector, `__r.iter()…collect()`, formatted as `Vec<Option<_>>`. There are 14 new cases, one per binding. They all match real values, because the typed fixtures are single-chunk. Among old cases only `LazyFrame::unique_generic` moved, from match to row_order_differs, which is the known permutation. The oracle has 2,383 cases, all verified. Tally of the committed debug-profile results file: 2,266 match, 98 both_error, 17 both_panic, 2 row_order_differs.

| Measure | 0101 | 0102 |
|---|---:|---:|
| Available to a script | 2,131 | 2,132 |
| Value-tested | 1,375 | 1,376 |
| Unsupported | 1,912 | 1,911 |
| Bindings (new) | | +14 |
| Oracle cases | 2,369 | 2,383 |
| Refused proven pairs | 140 | 126 |
| Unresolved pairs | 564 | 564 |

**Verification.** All of these passed:

- The generator self-test: 25 suites.
- The drift check (`--check`).
- The debug suite with `generated,test-support`: 137 tests plus 1 ignored.
- Both release configurations from the README, with fail-fast disabled: 9 tests with default features and 136 plus 1 ignored with `test-support`.
- `git diff --check`.

The release build has no warnings.

**Launch.** The 0102 binary's SHA-256 is `e053492fabc352dc384512acca01529dd486ee1d3e9d62fea6caaacbd32d11fb`. Cold launch was measured in three interleaved 60-run sets against a budget of +5 ms:

| Set | 0101 median | 0102 median | Difference | Raw samples |
|---|---:|---:|---:|---|
| 1 | 17.02 ms | 16.84 ms | −0.18 ms | `probes/0102/launch-results-1.json` |
| 2 | 18.31 ms | 17.19 ms | −1.12 ms | `probes/0102/launch-results-2.json` |
| 3 | 18.35 ms | 18.28 ms | −0.07 ms | `probes/0102/launch-results-3.json` |

## Remaining

`downcast_iter`, `downcast_into_iter`, `downcast_chunks` and the other Arrow-return methods, List and Struct values, and the function-generic and callback pools keep their dispositions. The refused proven pool is 126 and the unresolved pool is 564.
