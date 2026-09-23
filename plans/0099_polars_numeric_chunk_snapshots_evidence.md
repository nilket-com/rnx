# 0099 owned numeric chunk snapshots: evidence

Plan `4d94ca5`. `ChunkedArray::chunks` binds on the ten numeric wrappers and returns an owned `Vec<Vec<Option<script value>>>`: one inner vector per Polars chunk, values and nulls in order. The ten pairs leave the refused proven-pair pool, which falls from 168 to 158. The six nonnumeric pairs remain refused, and all ten new oracle cases match. The unresolved pool stays at 564.

## Gate 1: source and pairs

The inventory records `pub fn chunks(&self) -> &Vec<ArrayRef>` on `impl<T: PolarsDataType> ChunkedArray<T>`, with no where-clauses, parameters or method generics. The source, polars-core 0.55.2 `chunked_array/mod.rs:478-481`, is `&self.chunks`: a borrow with no allocation. For the numeric types, `impl_polars_num_datatype` (`datatypes/mod.rs:181-227`) sets `T::Array = PrimitiveArray<T::Native>`.

`probes/0099/pairs.json` lists the 16 pairs, each previously `refused: unsupported: foreign type: return (Box<dyn polars_arrow::array::Array>)`. The ten numeric pairs are emitted: Float32, Float64, Int8 through Int64, UInt8 and UInt16, `IdxCa` for UInt32, and UInt64. BinaryOffset, Binary, Boolean, List, String and Struct stay refused, now as ``chunk snapshot: `…` is not a listed pair``.

**Boundary probe** (direct Rust):
- `[1, 2, 3] ++ [1, null, 3]` has chunks `[3, 3]`.
- `Int64Chunked::from_vec("x", [])` is one empty chunk `[0]`.
- Polars's `append` drops an empty chunk: `[]` then `[5]` gives `[1]`. So a script cannot place an empty chunk mid-array, and the support unit test covers that case directly.

The retained 0098 binary's SHA-256 is `b2ef6c8fb68040f8b52a3788ddf496991193400ef207fa7298c5a15f2aa39a50`.

## Gate 2: only this Arrow return

The release file has one `[[chunk_snapshots]]` entry: key `polars_core:3681`, the path, the ten `[type, native]` pairs and a citation. `ChunkSnapshot::check` requires all of the following, and names the fault otherwise:
- `ChunkedArray<T>` with impl bounds exactly `T: PolarsDataType` and no where-clauses
- `&self` with no parameters or generics
- a return of exactly `&alloc::vec::Vec<polars_arrow::array::ArrayRef>`
- pairs from `NUMERIC_NATIVES`, without duplicates

`chunk_snapshot_entry` refuses a double listing. In `emit_instantiations`, a malformed entry or an unlisted pair is refused by name. A listed pair sets `World.chunk_snapshot` to (method, native) around `emit_method`.

With the scope set, `World::ret` accepts only a top-level shared reference to `Vec` of `ArrayRef`, or of the resolved `Box<dyn Array>`. It maps that to `support::chunk_snapshot::<N, _>(__r, "chunks", |__r| Ok::<_, Error>(<scalar rule>))?`, with `u64` checked by 0093. The emitter refuses the binding if it is routed, if the conversion is not the chunk snapshot, or if the receiver is not `&self`. The bindings are unrouted, so the borrow of the chunk vector ends inside the call.

`support::chunk_snapshot`:
1. Sums the chunk lengths with `checked_add`.
2. Calls `snapshot_budget(chunks, cells)`. That is one inclusive bound on chunk slots plus cells, checked with `checked_add`. An overflow or an excess is `MaterializeLimit`, naming both counts and the bound, before any allocation.
3. Downcasts each chunk with `as_any().downcast_ref::<PrimitiveArray<N>>()`. A mismatch is a `ConversionError` naming `chunks` and the expected array.
4. Re-counts slots cumulatively while copying and refuses on any excess.
5. Returns nothing partial on error.

Controls:

- **`chunk-snapshot self-test`.** The cited shape passes, and `native_for` resolves only listed types. Twelve malformed shapes fail with the fault named: a blank citation, another owner bound, a where-clause, an owned receiver, a parameter, `&mut Vec<ArrayRef>`, an owned `Vec<ArrayRef>`, `&ArrayRef`, no pair, a nonnumeric pair, a mispaired native and a duplicate pair. A double listing fails. In the emitter, the `i8` and `u64` scopes produce the snapshot conversion with the scalar rule, the latter checked. An owned vector, a nested `Option<&Vec<..>>`, a single `&Box<dyn Array>` and an unscoped chunk list are all unsupported with no text.
- **`chunk_snapshot_drift_is_refused_by_name`, on the real generator.** It makes three runs:
  - Without the Int64 pair, nine bind and Int64 is named.
  - With a blank citation, all 16 pairs read `chunk snapshot: no citation` and no text is emitted.
  - An inventory whose `chunks` returns an owned `Vec<ArrayRef>` refuses all 16 by name with no text.
- **Support unit tests.** `the_snapshot_budget_counts_chunks_and_cells`: at a bound of 7, 2+5 passes, 3+5 is refused with the exact message, 8 empty chunks are refused and 7 pass. `usize::MAX + 1` overflows: `m: … chunks and 1 cells overflow the slot count, more than the bound of 7`. With the override reset to 0, the production bound applies. `chunk_snapshots_keep_boundaries_and_refuse_the_wrong_array`: `[a, empty, a]` gives `[[1, n], [], [1, n]]`. An `i32` chunk read as `i64` is a `ConversionError`. At a bound of 5, 2 chunks with 4 cells are refused; at 6 they pass.

## Gate 3: boundaries, values and ownership

`tests/chunk_snapshots.rs` renders chunks as `[a,b|c,d]` and compares the script with direct Rust `chunks()` inspection:

- **All ten wrappers.** Each is built from `[1, 2, 3] ++ [1, null, 3]` cast to the type, giving two chunks with the null in the second. Each is also checked as one chunk, as an empty receiver (one empty chunk), as a result that outlives its dropped receiver, and as the receiver read again. Int64 gives `[1,2,3|1,n,3] [1,2,3] [] [1,2,3] [1,2,3|1,n,3]`, and Float64 gives the same shape with `1.0` formatting.
- **Extremes.**

  | Input | Result |
  |---|---|
  | Int8 `[-128, 127, 0]` | `[-128,127,0]` |
  | UInt64 `[0, i64::MAX]` | the same values |
  | Float32 `[0.1, -0.0]` | `[0.10000000149011612,-0.0]` |
  | Float64 `[-0.0, NaN, inf, -inf]` | the same values |
  | `series_u64_boundary` | `ConversionError: chunks: 9223372036854775808 does not fit a script integer` |
  | `series_u64_extremes` | `ConversionError: chunks: 18446744073709551615 does not fit a script integer` |

  After the UInt64 failures the receiver's `get(0)` still returns `i64::MAX`, and a second `chunks()` gives the same error.
- **Bound.** Two chunks with six cells cost 8 slots. At 7: `MaterializeLimit: chunks: 2 chunks and 6 cells (8 slots), more than the bound of 7`. At 8 the call succeeds. An empty receiver, 1 slot, passes at 1. The production bound is restored afterwards.

## Gate 4: reconcile and cost

**Oracle.** The Rust side now frames a listed chunk list as the same nested options: it downcasts each chunk to `PrimitiveArray<N>` and formats the result as `Vec<Vec<Option<N>>>`. There are ten new cases, one per binding, all matching on one-chunk fixtures. The direct tests cover multi-chunk, empty, null and overflow cases. The oracle has 2,351 cases, all verified, and no old status moved. Tally of the committed debug-profile results file: 2,234 match, 98 both_error, 17 both_panic, 2 row_order_differs.

| Measure | 0098 | 0099 |
|---|---:|---:|
| Available to a script | 2,129 | 2,130 |
| Value-tested | 1,373 | 1,374 |
| Unsupported | 1,914 | 1,913 |
| Generated operations | 2,118 | 2,119 |
| Bindings (new) | | +10 |
| Oracle cases | 2,341 | 2,351 |
| Refused proven pairs | 168 | 158 |
| Unresolved pairs | 564 | 564 |

**Verification.** All of these passed:

- The generator self-test: 23 suites.
- The drift check (`--check`).
- The debug suite with `generated,test-support`: 123 tests plus 1 ignored.
- Both release configurations from the README, with fail-fast disabled: 9 tests with default features and 122 plus 1 ignored with `test-support`.
- `git diff --check`.

**Launch.** The 0099 binary's SHA-256, rebuilt after round 1, is `81aa89a4b481d8d19116a2f585a23792ba1058cd27df4a44653bc4af4f84d2b1`. Cold launch was measured again on that binary in three interleaved 60-run sets against a budget of +5 ms:

| Set | 0098 median | 0099 median | Difference | Raw samples |
|---|---:|---:|---:|---|
| 1 | 17.77 ms | 17.41 ms | −0.36 ms | `probes/0099/launch-results-1.json` |
| 2 | 17.44 ms | 17.38 ms | −0.06 ms | `probes/0099/launch-results-2.json` |
| 3 | 19.44 ms | 19.37 ms | −0.07 ms | `probes/0099/launch-results-3.json` |

## Round 1

Codex found that the two `MaterializeLimit` overflow paths did not name the active bound, which plan line 11 promises. The checked-addition overflow in `snapshot_budget` and the cell-count overflow in `chunk_snapshot` now end with `more than the bound of <limit>`, as does the copy re-count refusal. The exact-message unit assertion now covers the bound. Generated output is unchanged: the drift check passes. The two support unit tests and `tests/chunk_snapshots.rs` pass. The binary was rebuilt, rehashed and remeasured above.

## Remaining

The six nonnumeric `chunks` pairs, the other Arrow arrays and buffers, bitmap-mutating methods, and the function-generic and callback pools keep their dispositions. The refused proven pool is 158 and the unresolved pool is 564.
