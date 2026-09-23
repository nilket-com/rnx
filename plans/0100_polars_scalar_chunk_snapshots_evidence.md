# 0100 owned Boolean, string and binary chunk snapshots: evidence

Plan `566755b`. `ChunkedArray::chunks` now also binds on `BooleanChunked`, `StringChunked`, `BinaryChunked` and `BinaryOffsetChunked`: four bindings and four new oracle cases, all matching. List and Struct stay refused. The refused proven-pair pool falls from 158 to 154. The distinct operation counts do not change, because `chunks` was already available and value-tested.

## Gate 1: pairs and array mappings

`probes/0100/pairs.json` records the six remaining `chunks` pairs. For each it gives the alias, identity, Arrow array, disposition before and after, decision and oracle status. polars-core 0.55.2 `datatypes/mod.rs:229-232` maps `StringType` to `Utf8ViewArray`, `BinaryType` to `BinaryViewArray`, `BinaryOffsetType` to `BinaryArray<i64>` and `BooleanType` to `BooleanArray`. Each array's `iter()` yields `Option` values with its validity.

List (`ListArray<i64>`) and Struct stay refused as not listed: each cell is itself an array or a row of fields, and needs its own nested-value policy. Both boundary facts from 0099 hold here. An empty receiver is exactly one empty chunk, shown as `1[]`. Polars's `append` drops an empty chunk, so a mid-array empty chunk is covered by the support unit test.

The retained 0099 binary's SHA-256 is `81aa89a4b481d8d19116a2f585a23792ba1058cd27df4a44653bc4af4f84d2b1`.

## Gate 2: only the four cited owners

The 0099 `[[chunk_snapshots]]` entry now also lists `[BooleanType, "bool"]`, `[StringType, "str"]`, `[BinaryType, "binary"]` and `[BinaryOffsetType, "binary_offset"]`, and cites the mapping. `ChunkSnapshot::check` accepts a pair only from `NUMERIC_NATIVES` or from the fixed `SCALAR_CHUNKS` table, where an owner must carry its own kind. The exact `&self -> &Vec<ArrayRef>` and `ChunkedArray<T: PolarsDataType>` checks from 0099 are unchanged, and no general `ArrayRef` rule was added. Each kind maps to its own copier and script type:

| Kind | Copier | Script type |
|---|---|---|
| `bool` | `chunk_snapshot_bool` | `Vec<Vec<Option<bool>>>` |
| `str` | `chunk_snapshot_str` | `Vec<Vec<Option<String>>>` |
| `binary` | `chunk_snapshot_binview` | `Vec<Vec<Option<Vec<i64>>>>` |
| `binary_offset` | `chunk_snapshot_binary_offset` | `Vec<Vec<Option<Vec<i64>>>>` |

Binary cells use the adapter's existing byte-as-integer mapping. The emitter still refuses a routed binding.

The four copiers share `payload_snapshot`:
1. Run a preflight over the borrowed chunk slice, allocating nothing proportional to the input. It downcasts each chunk (the wrong array is `ConversionError: chunks: a chunk is not a <Array>`).
2. In the same preflight, sum the cells and the payload bytes of the non-null values through `payload_count_step`, a checked addition. An overflow is `MaterializeLimit`, naming the chunk count, the cells and bytes counted so far, what was being added, and the bound.
3. Call `payload_snapshot_budget(chunks, cells, bytes)`: one inclusive, checked bound on chunk slots plus cells plus bytes, with Boolean contributing 0 bytes, checked before any allocation. The message names all three counts, the total and the bound.
4. Copy, downcasting each chunk again and counting every chunk, cell and byte again, refusing any excess.
5. Return nothing partial on error.

Bytes are UTF-8 bytes, not characters.

Controls:

- **`chunk-snapshot self-test`, extended.** The four scalar owners with their kinds pass. String paired as `binary`, Boolean paired as `u8`, List, and an unlisted Struct each fail, with the fault named. Each kind emits its own copier and element type.
- **`chunk_snapshot_drift_is_refused_by_name`, extended on the real generator.** A String pair given the `binary` kind refuses all 16 pairs by name, with no text. Without the Boolean pair, 13 bind and Boolean is named. The 0099 runs still pass: the uncited entry, the owned `Vec` in the inventory, and Int64 removed, which now leaves 13 bindings.
- **Support unit test `payload_snapshots_count_chunks_cells_and_bytes`.**
  - `[s, empty, s]` keeps the explicit empty chunk.
  - 3 chunks + 6 cells + 8 bytes (`ab` and `é`, twice) is 17 slots: refused at 16 with `m: 3 chunks, 6 cells and 8 bytes (17 slots), more than the bound of 16`, accepted at 17.
  - Binary views keep `[0, 255]`, the empty value and null. Offset binary keeps the non-UTF-8 `[0xc3, 0x28]`. Boolean keeps `[true, null, false]`.
  - The wrong array is a typed error for each kind.
  - `usize::MAX + 1` and `cells + bytes` overflows are named.
  - `payload_count_step` messages are checked exactly for both sums. The cell sum gives `m: 2 chunks, <usize::MAX> cells and 5 bytes counted; adding 1 cells overflows, more than the bound of 7`, and the byte sum gives the same form.
  - 10,000 empty chunks under a bound of 3 are refused as `m: 10000 chunks, 0 cells and 0 bytes (10000 slots), more than the bound of 3`, before anything is allocated for the result.
  - A wrong array is still a typed error from the preflight.

## Gate 3: values, validity, ownership and bounds

Three new two-chunk fixtures feed no producer:
- `series_str_mixed`: `["é日本", ""] ++ [null, "z"]`
- `series_binary_mixed`: `[c3 28, <empty>] ++ [null, 00 00 ff]`
- `series_binary_offset_mixed`: the same bytes as offset binary

`tests/scalar_chunk_snapshots.rs` renders `<chunks>[a,b|c,d]` and compares the script with direct Rust inspection of `chunks()`. For each owner it checks a two-chunk receiver, a one-chunk receiver, an empty receiver, a result kept after its receiver is dropped, and the receiver read again:

| Owner | Two-chunk result |
|---|---|
| Boolean | `2[true,false,true\|true,n,true]` (`series_bool` plus a Boolean cast of `[1, null, 3]`) |
| String | `2["é日本",""\|n,"z"]` (multibyte, empty and null) |
| Binary | `2[<195 40>,<>\|n,<0 0 255>]` (non-UTF-8, empty, zero bytes and null) |
| BinaryOffset | the same |

An empty receiver gives `1[]`.

Bounds (`polars::set_materialize_limit`):
- **String.** The string receiver costs 2 + 4 + 9 = 15 slots (8 bytes for `é日本`, 0 for `""`, 1 for `z`). At 14 it is refused with `MaterializeLimit: chunks: 2 chunks, 4 cells and 9 bytes (15 slots), more than the bound of 14`. At 15 it is copied.
- **Binary.** The binary receiver costs 2 + 4 + 5 = 11 slots, although no single value exceeds 3 bytes. At 10 it is refused, and at 11 it is copied.

The receivers are reused after each refusal, and the production bound is restored afterwards.

## Gate 4: reconcile and cost

**Oracle.** The Rust side frames a scalar owner's chunks by downcasting each chunk to its array. Strings are made owned and bytes become `Vec<u8>`, which format like the script's integers. The formatted type is the nested `Vec<Vec<Option<_>>>`. There are four new cases, one per binding, all matching. No old status moved against 0099. `LazyFrame::unique_generic` matched in the first run and differed in row order in the round-1 rerun, the known permutation. The oracle has 2,355 cases, all verified. Tally of the committed debug-profile results file from the round-1 rerun: 2,238 match, 98 both_error, 17 both_panic, 2 row_order_differs.

| Measure | 0099 | 0100 |
|---|---:|---:|
| Available to a script | 2,130 | 2,130 |
| Value-tested | 1,374 | 1,374 |
| Unsupported | 1,913 | 1,913 |
| `chunks` bindings | 10 | 14 |
| Oracle cases | 2,351 | 2,355 |
| Refused proven pairs | 158 | 154 |
| Unresolved pairs | 564 | 564 |

**Verification.** All of these passed:

- The generator self-test: 23 suites.
- The drift check (`--check`).
- The debug suite with `generated,test-support`: 126 tests plus 1 ignored.
- Both release configurations from the README, with fail-fast disabled: 9 tests with default features and 125 plus 1 ignored with `test-support`.
- `git diff --check`.

**Launch.** The 0100 binary's SHA-256, rebuilt after round 1, is `e0d2d7ecc3303d02a30e2059b58c62ad2dd36ce2ece69e57d8fc156292c02884`. Cold launch was measured on it in three interleaved 60-run sets against a budget of +5 ms:

| Set | 0099 median | 0100 median | Difference | Raw samples |
|---|---:|---:|---:|---|
| 1 | 19.18 ms | 17.66 ms | −1.52 ms | `probes/0100/launch-results-1.json` |
| 2 | 19.52 ms | 18.23 ms | −1.29 ms | `probes/0100/launch-results-2.json` |
| 3 | 16.97 ms | 17.63 ms | +0.66 ms | `probes/0100/launch-results-3.json` |

## Round 1

Codex found two faults in `payload_snapshot`:

1. **Allocation before the bound.** It collected every downcast chunk into a `Vec<&A>` before checking the bound, so storage proportional to the chunk count was allocated even when the call had to refuse.
2. **Incomplete overflow messages.** The preflight overflow messages lacked the accumulated counts the plan requires.

The preflight now walks the borrowed slice without collecting it, and the copy downcasts again. `payload_count_step` makes each checked addition and names the chunks, the cells and bytes so far, what overflowed, and the bound. The unit test adds exact messages for both overflows, the 10,000-empty-chunk refusal under a bound of 3, and a preflight wrong-array error.

Generated output is unchanged: the drift check passes. The three suites were rerun with the same counts, and the binary was rebuilt, rehashed and remeasured above.

## Remaining

List and Struct chunk snapshots, the other Arrow array and buffer returns, and the function-generic and callback pools keep their dispositions. The refused proven pool is 154 and the unresolved pool is 564.
