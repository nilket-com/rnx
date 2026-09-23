# 0106 tagged, owned snapshots from layout: evidence

Plan `f0e7aea`. `ChunkedArray::layout()` binds on the fourteen numeric, Boolean, String, Binary and BinaryOffset wrappers. It returns an owned `(variant name, Vec<Vec<Option<script value>>>)`. The name is exactly the Polars variant returned: `SingleNoNull`, `Single`, `MultiNoNull` or `Multi`. The payload is that variant's one array or all of its chunks, copied under a bound checked before the call. There are 14 bindings and 14 new oracle cases, all matching. List and Struct stay refused. The refused proven-pair pool falls from 84 to 70. The operation adds one distinct available and value-tested operation. The 564 unresolved pairs are unchanged.

## Gate 1: pairs, source and variants

The inventory records `pub fn layout(&self) -> ChunkedArrayLayout<'_, T>`: `&self`, no parameters or method generics, `impl_head = ChunkedArray<T>`, `impl_bounds = [T: PolarsDataType]` and `impl_where = [T: PolarsDataType]`. Its canonical return is `polars_core::chunked_array::ChunkedArrayLayout<T>`.

The source is polars-core 0.55.2:
- **The enum** (`chunked_array/mod.rs:919-924`): `ChunkedArrayLayout` has four variants: `SingleNoNull(&T::Array)`, `Single(&T::Array)`, `MultiNoNull(&ChunkedArray<T>)` and `Multi(&ChunkedArray<T>)`.
- **The method** (`mod.rs:926-946`): with one chunk it returns `SingleNoNull` if that chunk's null count is 0, else `Single`. Otherwise it returns `MultiNoNull` if every chunk has null count 0, else `Multi`.

`probes/0106/pairs.json` lists all 16 pairs, each previously `refused: unsupported: generic type: return (ChunkedArrayLayout<…Type>)`. For each it records the alias, identity, concrete `T::Array`, disposition before and after, decision and oracle status. 14 pairs are emitted. List and Struct stay refused as not listed.

Variants observed through direct Rust and the script alike:

| Receiver | Variant |
|---|---|
| one chunk, no nulls | `SingleNoNull` |
| one chunk with a null | `Single` |
| two chunks, no nulls | `MultiNoNull` |
| two chunks, null in the later one | `Multi` |
| empty receiver (one empty chunk) | `SingleNoNull`, `[]` |
| zero chunks, from the safe `ChunkedArray::from_chunk_iter` with no arrays | `MultiNoNull`, `[]` (support unit test) |

The retained 0105 binary's SHA-256 is `b831bcc5e98b3d2f41c1d18dc47022ed177f09ef364821bd02a6c492e91d5ccd`.

## Gate 2: this enum only

The release file has one `[[layout_snapshots]]` entry: key `polars_core:3706`, the path, fourteen pairs from the 0099 and 0100 tables, and a citation. `LayoutSnapshot::check` is standalone, because 0101's check demands no where-clause. It requires all of the following, and names the fault otherwise:
- the citation
- `ChunkedArray<T>` with impl bounds exactly `[T: PolarsDataType]` and `impl_where` exactly `[T: PolarsDataType]`
- `&self`, with no parameters or method generics
- a return whose exact text is `ChunkedArrayLayout<T>`
- pairs from the fixed tables, each owner with its own kind, without duplicates

A double listing is refused. In `emit_instantiations`, a malformed entry or an unlisted pair is refused by name. A listed pair sets `World.layout` to (method, kind, owner type). With it set, `World::ret` accepts only a top-level `ChunkedArrayLayout<X>` where the parsed `X` equals the pair's owner type (for example `Int8Type`, not `ChunkedArray<Int8Type>`). The emitter refuses a routed call and any other conversion. It inserts the kind's preflight before the call:

```
let __total = support::preflight_*(this.0.chunks(), "layout")?;
let __r = <Owner>::layout(&this.0);
Ok(support::layout_snapshot*(this.0.chunks(), __total, __r, "layout", …)?)
```

`layout_snapshot*` matches the real variant through `layout_parts`. A `Single*` variant yields its one array; a `Multi*` variant yields `downcast_iter` over its chunked array. Either is copied through `iter_copy` against the preflight: each chunk's length must match its preflight chunk, the chunk count must match, every slot is re-counted, and the final total must match. So a `Single*` variant must correspond to exactly one preflight chunk, and a `Multi*` variant to the whole sequence. The tag `String` is allocated only after the copy succeeds. Nothing partial is returned, and the 0099 to 0105 budgets are unchanged.

**Oracle script-side framing.** The generated oracle compared no tuple returns before this record. For a listed layout pair only, it now frames the script's `(tag, chunks)` exactly like the Rust tuple formatter, as `(len:tag, len:chunks)`. Every other tuple return is still uncompared, which is why no other operation gained a case.

Controls:

- **`layout self-test`.** The cited shape passes, and `pair_for` returns the owner type and kind. Ten malformed shapes fail with the fault named: a blank citation, no where-clause, an extra where-clause, an owned receiver, a parameter, an `Option<ChunkedArrayLayout<T>>` return, `ChunkedArrayLayout<T::Array>`, an `Int8Type`-as-`i16` pair, Struct and a duplicate pair. A double listing fails. In the emitter, the `i8` and `str` pairs place the preflight before the single `layout(&this.0)` call and the call before the copier, and return `(String, Vec<Vec<Option<…>>>)`. Another owner type, an `Option`-wrapped layout, `ChunkedArrayLayout<ChunkedArray<Int8Type>>`, an unscoped layout, and a String layout under an `Int8Type` scope are unsupported with no text.
- **`layout_drift_is_refused_by_name`, on the real generator.** Five runs:
  - A blank citation refuses all 16 pairs by name with no text.
  - An Int16 pair given `i32` does the same.
  - Without the Int64 pair, 13 bind and Int64 is named.
  - An inventory with no where-clause refuses all 16 by name with no text.
  - An inventory whose return is `Option<ChunkedArrayLayout<T>>` refuses all 16 by name with no text.
- **Support unit test `layout_snapshots_report_the_variant_and_check_its_shape`.** A one-chunk array gives `("SingleNoNull", [[1, 2]])`. A two-chunk array with a later null gives `("Multi", [[1], [None, 3]])`. A `Single` variant against a two-chunk preflight is refused as `m: chunk 0 has 2 cells, 1 were counted`, and a `Multi` variant of another array against a one-chunk preflight is refused. Zero chunks give `("MultiNoNull", [])`. The preflight refuses 5 slots at 4.

## Gate 3: tag, values, ownership and bounds

`tests/layout_snapshots.rs` builds four receivers per owner, one for each variant, and compares the script's `(tag, chunks)` with a direct Rust `match` on the real `layout()` for all fourteen owners. It also checks an empty receiver, a result kept after its receiver is dropped, and a re-read. The receivers are:
- **Numeric and Boolean:** Int64 fixtures cast to the type.
- **String, Binary and BinaryOffset:** the typed fixture as `SingleNoNull`, the mixed fixture rechunked as `Single`, the typed fixture appended to itself as `MultiNoNull`, and the mixed fixture as `Multi`.

Spelled out:

| Owner | Result |
|---|---|
| Int64 | `SingleNoNull 1[1,2,3] / Single 1[1,n,3] / MultiNoNull 2[1,2,3\|1,2,3] / Multi 2[1,2,3\|1,n,3] / SingleNoNull 1[] / Multi … / Multi …` |
| String | `SingleNoNull 1["a","bb","ccc"] / Single 1["é日本","",n,"z"] / MultiNoNull 2[…] / Multi 2["é日本",""\|n,"z"] / SingleNoNull 1[] …` |
| BinaryOffset | begins `SingleNoNull 1[<97 98>,<0 255>,<>] / Single 1[<195 40>,<>,n,<0 0 255>] / MultiNoNull 2[` |

Extremes:
- Int8 gives `SingleNoNull 1[-128,127,0]`.
- Float32 gives `[0.10000000149011612,-0.0]`.
- Float64 gives `[-0.0,NaN,inf,-inf]`.
- `series_u64_boundary` gives `ConversionError: layout: 9223372036854775808 does not fit a script integer` with no partial result. The receiver then reads `i64::MAX` and still has 1 chunk.

Bounds, all refused by the preflight before the call:

| Receiver | Cost | Refused at | Copied at |
|---|---:|---:|---:|
| `Multi` Int64, two chunks that each fit at 4 | 8 | 7 (`layout: 2 chunks and 6 cells (8 slots), more than the bound of 7`) | 8 |
| `Single` Int64 | 1 + 3 = 4 | 3 | 4 |
| `Multi` strings | 2 + 4 + 9 = 15 | 14 | 15 |

## Gate 4: reconcile and cost

**Oracle.** The Rust side matches all four variants of the real `layout()` into `(name, nested options)` and formats the tuple. There are 14 new cases, one per binding, all matching. The standard one-chunk fixtures give `SingleNoNull`; the direct tests cover the other three variants. No old status moved. The oracle has 2,439 cases, all verified. Tally of the committed debug-profile results file: 2,322 match, 98 both_error, 17 both_panic, 2 row_order_differs.

| Measure | 0105 | 0106 |
|---|---:|---:|
| Available to a script | 2,135 | 2,136 |
| Value-tested | 1,379 | 1,380 |
| Unsupported | 1,908 | 1,907 |
| Bindings (new) | | +14 |
| Oracle cases | 2,425 | 2,439 |
| Refused proven pairs | 84 | 70 |
| Unresolved pairs | 564 | 564 |

**Verification.** All of these passed:

- The generator self-test: 29 suites.
- The drift check (`--check`).
- The debug suite with `generated,test-support`: 157 tests plus 1 ignored.
- Both release configurations from the README, with fail-fast disabled: 9 tests with default features and 156 plus 1 ignored with `test-support`.
- `git diff --check`.

The release build has no warnings.

**Launch.** The 0106 binary's SHA-256 is `80cac0835d3bc4fce77aa1ed4d911571034731c2adc29494a8054302cd6feef9`. Cold launch was measured in three interleaved 60-run sets against a budget of +5 ms:

| Set | 0105 median | 0106 median | Difference | Raw samples |
|---|---:|---:|---:|---|
| 1 | 16.64 ms | 16.24 ms | −0.40 ms | `probes/0106/launch-results-1.json` |
| 2 | 18.40 ms | 17.43 ms | −0.97 ms | `probes/0106/launch-results-2.json` |
| 3 | 18.75 ms | 17.62 ms | −1.13 ms | `probes/0106/launch-results-3.json` |

## Remaining

The Arrow-array input methods, List and Struct nested arrays, and the function-generic pool keep their dispositions. The refused proven pool is 70 and the unresolved pool is 564. With 0106 the snapshot family covers `chunks`, `downcast_get`, `downcast_as_array`, `downcast_iter`, `downcast_chunks`, `downcast_into_iter` and `layout` on the fourteen scalar owners.
