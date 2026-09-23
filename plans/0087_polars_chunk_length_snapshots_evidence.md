# 0087 chunk-length snapshots: evidence

Plan `3880625`. `ChunkedArray::chunk_lengths` returns an owned vector of integers, one per chunk in order. All 16 proven pairs emitted and all 16 new oracle cases match Rust. Neither the iterator nor any Arrow array crosses the binding boundary.

## Gate 1: freeze and verify

`probes/0087/pairs.json` holds the 16 rows (key `polars_core:3680`, alias, substituted signature, disposition before and after, oracle outcome). All 16 were candidates.

Source, pinned polars-core 0.55.2:

- `ChunkLenIter<'a>` is `Map<Iter<'a, ArrayRef>, fn(&ArrayRef) -> usize>` (`chunked_array/mod.rs:61`).
- `chunk_lengths` is `self.chunks.iter().map(|chunk| chunk.len())` (`:472-475`). It borrows the receiver, yields one `usize` per chunk in order, and is exact-size because a slice iterator's `Map` keeps `ExactSizeIterator`.

The inventory records the return as the alias path, which the mapper resolves one level down. `chunks()` (`polars_core:3681`, `&Vec<ArrayRef>`) stays refused. The ceiling was 1 operation and 16 bindings, and both were reached.

## Gate 2: narrow generation

The release file gained `[[iterator_returns]]` with `path`, integer `item` and `cite`. While a listed callable is emitted, `World.iter_return` holds its item type. It is set and cleared by the scoped guard shared with 0085 and 0086, in both `emit_callable` and the method emitter. In the return mapper, a `core::iter::adapters::map::Map` under that scope becomes an exact-size materialization: `support::materialize_exact` refuses an over-bound count from the iterator's length before consuming anything, and each item goes through `support::widen::<usize>`. That is a `TryInto<i64>` that returns a `ConversionError` naming the operation instead of wrapping. The iterator is created and drained inside the call while the receiver borrow is live, so only `Vec<i64>` leaves the call. The oracle frames the Rust result by the listed return type through `World.iter_return_items`.

Existing `usize` returns elsewhere in the adapter still convert with `as i64`, which would wrap above `i64::MAX`. That is outside this record and is reported to review as an observation.

Synthetic controls (`iterator-return self-test`):

- The listed `chunk_lengths` return, spelled through a `ChunkLenIter` alias, emits `Result<Vec<i64>, Error>` with the exact-size helper and `widen`, and the iterator is consumed inside the call.
- An unlisted method returning the same alias, a different `Map`, and a `chunks()`-style Arrow array return are refused.
- The scope is clear after emission.

A support unit test checks `widen` at 7 and at `i64::MAX`, and refuses `i64::MAX + 1` with its message. Both adapter configurations compile all 16 pairs, and no other callable changed disposition.

Counts, 0086 to 0087:

| Measure | 0086 | 0087 |
|---|---:|---:|
| Generated operations | 2,096 | 2,097 |
| Unsupported | 1,936 | 1,935 |
| Bindings | 4,002 | 4,018 |
| Refused proven pairs | 212 | 196 |
| Oracle cases | 2,146 | 2,162 |

## Gate 3: behaviour and oracle

`tests/chunk_lengths.rs` covers two areas:

- **Chunk shapes.** A single chunk gives `[3]`. A three-chunk array built from a nullable, a plain and a one-row slice gives `[3, 3, 1]`: its sum equals the receiver's length and its count equals `n_chunks()`. A rechunked array gives `[7]`, and that vector survives its receiver going out of scope. An empty slice gives one zero-length chunk, which confirms the plan's warning not to derive lengths from the total.
- **Bound.** A bound equal to the chunk count admits the call. One below it refuses with `chunk_lengths: 2 items, more than the bound of 1`. The receiver answers normally afterwards.

Oracle: 2,162 cases, all verified. The 16 new cases all match, and no old case changed status. Tally: 2,045 match, 98 both_error, 17 both_panic, 2 row_order_differs.

Scoreboard, 0086 to 0087:

| Measure | 0086 | 0087 |
|---|---:|---:|
| Available to a script | 2,107 | 2,108 |
| Value-tested | 1,334 | 1,335 |
| Unsupported | 1,936 | 1,935 |

## Gate 4: cost

Generator self-test, drift and accounting, both adapter suites with fail-fast disabled, and the full oracle pass. The retained 0086 binary's SHA-256 is `9b9f48c818ba9b5735d3d519b617a87d41a1254141164b263cbd51c0695d2d92` and the 0087 binary's is `7345d2cc28469ba04733e8a90d43407f8b2ce64da40f23a9e5c183759da67027`. Cold launch was measured in three interleaved 60-run sets against a budget of +5 ms:

| Set | 0086 median | 0087 median | Difference | Raw samples |
|---|---:|---:|---:|---|
| 1 | 16.92 ms | 17.00 ms | +0.08 ms | `probes/0087/launch-results-1.json` |
| 2 | 21.39 ms | 20.74 ms | −0.65 ms | `probes/0087/launch-results-2.json` |
| 3 | 18.16 ms | 18.45 ms | +0.29 ms | `probes/0087/launch-results-3.json` |

## Remaining

196 refused proven pairs remain, mostly Arrow array returns, `with_validities` and unreachable types. The unchecked `usize as i64` conversion on other returns is a candidate for a small follow-up. The 0083 table's decisions are unchanged.
