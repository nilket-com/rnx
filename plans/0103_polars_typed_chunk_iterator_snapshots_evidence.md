# 0103 owned snapshots from downcast_iter: evidence

Plan `4c59efa`. `ChunkedArray::downcast_iter()` binds on the fourteen numeric, Boolean, String, Binary and BinaryOffset wrappers. The typed chunk iterator is driven into an owned `Vec<Vec<Option<script value>>>` in chunk order. There are 14 bindings and 14 new oracle cases, all matching. List and Struct stay refused. The refused proven-pair pool falls from 126 to 112. The operation adds one distinct available and value-tested operation. The 564 unresolved pairs are unchanged.

## Gate 1: pairs and source

The inventory records `pub fn downcast_iter(&self) -> impl DoubleEndedIterator<Item = &T::Array>` on `impl<T: PolarsDataType> ChunkedArray<T>`, with no parameters, method generics or where-clauses. The source, polars-core 0.55.2 `chunked_array/ops/downcast.rs:63-70`, is `self.chunks.iter().map(|arr| cast to &T::Array)`. It is a lazy, forward, borrowing iterator over the receiver's own chunks.

`probes/0103/pairs.json` lists all 16 pairs, each with its item array taken from the old refusal (`iterator item: … PrimitiveArray<N>`, `BooleanArray`, `BinaryViewArrayGeneric<str>`, and so on). It also records the disposition before and after, the decision and the oracle status. 14 pairs are emitted. List and Struct stay refused as not listed.

**Order probe.** Through direct Rust and the script alike, `[1, 2, 3] ++ [1, null, 3]` iterates as `2[1,2,3|1,n,3]`. An empty receiver is `1[]`, one empty chunk. The support unit test covers a middle empty chunk, because Polars's `append` drops one. The receiver is reused after every call in the tests.

The retained 0102 binary's SHA-256 is `e053492fabc352dc384512acca01529dd486ee1d3e9d62fea6caaacbd32d11fb`.

## Gate 2: only the cited iterator return

The release file has one `[[iter_snapshots]]` entry: key `polars_core:3505`, the path, fourteen pairs from the 0099 and 0100 tables, and a citation. `IterSnapshot::check` requires no parameters and a return whose exact canonical text, whitespace-normalised, is `impl core::iter::traits::double_ended::DoubleEndedIterator<Item = &T::Array>`. It then applies 0101's checks: the citation; `ChunkedArray<T: PolarsDataType>` with no where-clauses; `&self` with no generics; and pairs from the fixed tables, each owner with its own kind, without duplicates. A double listing is refused.

**A gap found while building.** The first version compared `Ty::render()` strings. `render` prints only an `impl` type's trait paths and drops the arguments and the item. So an inventory whose item changed from `&T::Array` to `T::Array` still passed, and the drift control caught it by emitting 14 bindings where it expected 0. The gate now uses two checks:
- the entry check compares the exact text
- `World::ret` inspects the parsed bound: exactly one `DoubleEndedIterator` with no arguments, and an item that is a shared borrow of exactly the pair's array

Earlier rules are not affected, because none compared an `impl` type through `render`.

In `emit_instantiations`, a malformed entry or an unlisted pair is refused by name, and a listed pair sets `World.iter_snapshot`. The binding calls `<Owner>::downcast_iter(&this.0)` once, then `support::iter_snapshot::<N, _>` or `support::iter_snapshot_{bool,str,binview,binary_offset}` with `this.0.chunks()`, the iterator and the method name. The emitter refuses a routed binding and any other conversion.

The copiers:

1. **Preflight.** They preflight the same immutable receiver's chunks with no allocation proportional to the input. Each chunk is downcast to the expected array; a mismatch is `ConversionError: … a chunk is not a <Array>`. Cells, and bytes for payload kinds, are summed through `payload_count_step`, and chunks plus cells plus bytes are bounded. This uses `numeric_preflight`, or `payload_preflight`, which was split out of 0100's copier so both share it.
2. **Copy.** They drive the iterator once, re-counting every chunk, cell and byte against the preflight total.
3. **Count and shape checks.** They refuse if the iterator yields more or fewer chunks than were counted, or if any yielded chunk's length differs from the preflight chunk at that position (round 1). They also refuse if the slots copied end below the preflight total. Nothing partial is returned.

The 0101 and 0102 single-array budgets are unchanged.

Controls:

- **`iter-snapshot self-test`.** The cited shape passes. Nine malformed shapes fail with the fault named: a blank citation, an owned receiver, a parameter, an owned `Item = T::Array`, a plain `Iterator`, an optional return, a Boolean-as-`u8` pair, List and a duplicate pair. A double listing fails. In the emitter, the `i8` and `bool` pairs call `downcast_iter(&this.0)` once and hand the copier `this.0.chunks()`. Another array's item, an owned item, an unscoped return and a Boolean iterator under an `i8` scope are unsupported with no text.
- **`iter_snapshot_drift_is_refused_by_name`, on the real generator.** Four runs:
  - A blank citation refuses all 16 pairs by name with no text.
  - A Binary pair given `str` does the same.
  - Without the Int64 pair, 13 bind and Int64 is named.
  - An inventory whose item becomes an owned `T::Array` refuses all 16 by name with no text. This is the control that exposed the `render` gap.
- **Support unit test `iterator_snapshots_preflight_then_copy_in_order`.**
  - `[a, empty, a]` is copied in forward order with the middle empty chunk kept.
  - Each chunk costs 3 alone but 7 together: refused at 6 with `m: 3 chunks and 4 cells (7 slots), more than the bound of 6`, accepted at 7.
  - 50 empty chunks are refused at 49 by their outer slots alone.
  - A string chunk read as numeric or Boolean is a typed error from the preflight, before any item is read.
  - An iterator yielding 2 of 3 counted chunks gives `m: the iterator yielded 2 chunks, 3 were counted`. One yielding 4 gives `… more than the 3 chunks counted`.
  - The same count with a shorter last chunk gives `m: chunk 2 has 1 cells, 2 were counted`. Swapped chunks give `m: chunk 0 has 0 cells, 2 were counted`. The same count and lengths with fewer payload bytes gives `m: 3 slots copied, 5 were counted` (round 1).
  - A string payload costing 5 slots is refused at 4 and copied at 5. Boolean values and nulls are kept.

## Gate 3: values, ownership and bounds

`tests/iter_snapshots.rs` compares the script with direct Rust `downcast_iter` for all fourteen owners. Each is checked on a two-chunk receiver, on its empty receiver, on a result kept after its receiver is dropped, and on the receiver iterated again. The receivers are:
- numeric and Boolean: `[1, 2, 3] ++ [1, null, 3]` cast to the type
- String: `series_str_mixed`
- Binary and BinaryOffset: the mixed binary fixtures

Three rows are spelled out in the test:

| Owner | Result |
|---|---|
| Int64 | `2[1,2,3\|1,n,3] 1[] …` |
| String | `2["é日本",""\|n,"z"] 1[] …` |
| BinaryOffset | `2[<195 40>,<>\|n,<0 0 255>] 1[] …` |

Extremes:
- Int8 gives `[-128, 127, 0]`.
- Float32 gives `[0.10000000149011612, -0.0]`.
- Float64 gives `[-0.0, NaN, inf, -inf]`.
- `series_u64_boundary` gives `ConversionError: downcast_iter: 9223372036854775808 does not fit a script integer` with no partial result. The receiver's `get(0)` then returns `i64::MAX`.

**Whole-iterator bound.**

| Receiver | Cost | Bound | Result |
|---|---:|---:|---|
| two-chunk Int64 (two chunks of 4 slots) | 8 | 7 | refused: `downcast_iter: 2 chunks and 6 cells (8 slots), more than the bound of 7` |
| same receiver, `downcast_get(1)` only | 4 | 7 | copied |
| two-chunk Int64 | 8 | 8 | copied |
| strings | 2 + 4 + 9 = 15 | 14 | refused |
| strings | 15 | 15 | copied |

## Gate 4: reconcile and cost

**Oracle.** The Rust side frames a `downcast_iter` result as the owned nested vectors, `__r.map(|a| a.iter()….collect()).collect()`, formatted as `Vec<Vec<Option<_>>>`. There are 14 new cases, one per binding, all matching on the typed fixtures. No old status moved. The oracle has 2,397 cases, all verified. Tally of the committed debug-profile results file: 2,280 match, 98 both_error, 17 both_panic, 2 row_order_differs.

| Measure | 0102 | 0103 |
|---|---:|---:|
| Available to a script | 2,132 | 2,133 |
| Value-tested | 1,376 | 1,377 |
| Unsupported | 1,911 | 1,910 |
| Bindings (new) | | +14 |
| Oracle cases | 2,383 | 2,397 |
| Refused proven pairs | 126 | 112 |
| Unresolved pairs | 564 | 564 |

**Verification.** All of these passed:

- The generator self-test: 26 suites.
- The drift check (`--check`).
- The debug suite with `generated,test-support`: 142 tests plus 1 ignored.
- Both release configurations from the README, with fail-fast disabled: 9 tests with default features and 141 plus 1 ignored with `test-support`.
- `git diff --check`.

The release build has no warnings.

**Launch.** The 0103 binary's SHA-256, rebuilt after round 1, is `6eff4543390e7c37b56da7b258f57ddd0515128d244411c02a5c14817983f296`. Cold launch was measured on it in three interleaved 60-run sets against a budget of +5 ms:

| Set | 0102 median | 0103 median | Difference | Raw samples |
|---|---:|---:|---:|---|
| 1 | 17.50 ms | 17.40 ms | −0.10 ms | `probes/0103/launch-results-1.json` |
| 2 | 18.17 ms | 17.85 ms | −0.32 ms | `probes/0103/launch-results-2.json` |
| 3 | 15.73 ms | 15.74 ms | +0.01 ms | `probes/0103/launch-results-3.json` |

## Round 1

Codex found that `iter_copy` checked only the chunk count and over-budget copies. An iterator with the same count but shorter or swapped chunks could therefore return success despite the preflight. The copy now checks two more things:
- each yielded chunk's length equals the preflight chunk at the same position
- the final copied slot count equals the preflight total

The unit test adds a shorter chunk, swapped chunks, and fewer payload bytes than counted, each with its exact message.

Generated output is unchanged: the drift check passes. The three suites were rerun with the same counts. The oracle tally is unchanged; only the recorded row order of the unordered `LazyFrame::unique` detail differs. The binary was rebuilt, rehashed and remeasured above.

## Remaining

`downcast_into_iter`, `downcast_chunks`, List and Struct typed arrays, and the other Arrow-return methods keep their dispositions. The refused proven pool is 112 and the unresolved pool is 564. `Ty::render`'s treatment of `impl` types is a latent hazard for any future rule that compares an `impl` return. Such a rule should compare exact text or the parsed bound, as 0103 does.
