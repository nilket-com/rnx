# 0086 validated bitmap inputs: evidence

Plan `f60634b`. `ChunkedArray::set_validity`, `with_validity`, `from_vec_validity` and `BooleanChunked::from_bitmap` take a script `Vec<bool>` for their one `Bitmap` parameter. The binding checks the mask's length against the receiver or the values before Polars sees it; Polars itself asserts or panics. All 41 target pairs emitted and all 41 new oracle cases match Rust.

## Gate 1: freeze and audit

`probes/0086/pairs.json` holds 59 rows: the 41 targets, the 2 Struct pairs of `set_validity` and `with_validity`, and the 16 `with_validities` pairs, each with key, alias, substituted signature, disposition before and after, audit class and oracle outcome. `with_outer_validity` has no proven pair in the census and needs no row.

Source contracts, pinned Polars 0.55.2:

- **`set_validity`** (polars-core `chunked_array/mod.rs:618-634`) asserts the dtype is not a struct and `assert_eq!(self.len(), v.len())` for a `Some` mask. It then slices the mask across chunks and recomputes `null_count`.
- **`with_validity`** (`:636-639`) calls `set_validity` on the consumed receiver.
- **`from_vec_validity`** (`chunked_array/from.rs:186-194`) goes through `to_array` and `PrimitiveArray::new`, which unwraps `try_new`'s length check (polars-arrow `array/primitive/mod.rs:465-477`).
- **`from_bitmap`** (`from.rs:219-225`) makes the bitmap the Boolean values with no validity.

Classification: 41 candidates, namely `set_validity` and `with_validity` on 15 aliases each, `from_vec_validity` on the 10 numeric aliases, and `from_bitmap` on Boolean. The 2 Struct pairs are excluded with Polars's own struct assertion cited. The 16 `with_validities` pairs are still refused: that method replaces chunk masks without recomputing `null_count` (`chunked_array/ops/chunkops.rs:219-228`), so a length check alone would leave the receiver inconsistent. The plan's ceiling was 4 operations and 41 bindings, and both were reached.

## Gate 2: generator and controls

The release file gained `[[bitmap_inputs]]` entries (path, `length` = `receiver`, `values` or `none`, citation) and a Struct entry in `[[instantiation.exclude]]`. While a listed callable is emitted, `World.bitmap_input` holds its operation name and length rule. It is set and cleared by the same scoped guard as 0085's return flag, in both `emit_callable` and the method emitter. The argument mapping turns the `Bitmap` parameter into `support::bitmap_from_bools(&v, op, expect)?`, and `Option` composes through the existing arm, so `None` stays `None`. The expected length is taken in `pre` before any receiver borrow: `this.0.len()`, or `support::vec_len(&values, ...)` without copying.

`bitmap_from_bools` works in this order:

1. It reads the vector's length.
2. It reserves that many bits from the cumulative materialize bound under a `SliceBudget` guard.
3. It compares the length with the expected one and returns `ShapeMismatch` naming the operation and both lengths on a mismatch.
4. Only then does it copy the bools, in order, into `Bitmap::from_iter`.

A non-bool element is a `ConversionError`. The script's vector is only read.

Synthetic controls (`bitmap-input self-test`) cover an optional receiver-length mask with `None => None`, a values-length mask, a mask with no length rule, and refusal of an unlisted bitmap input, an unlisted bitmap output and an Arrow array; the scope is clear after emission. Support unit tests cover bit order and `unset_bits`, an explicit empty mask, one-short and one-long refusal with the exact message, the bound refused at 2 and admitted at 3, and a non-bool element. Both adapter configurations compile every emitted binding, and no unlisted callable changed disposition.

The oracle's mask fixtures are chosen by argument shape: `mask3` for receiver-length masks, `mask2` for the List receiver whose fixture has two rows, and `mask1` for the one-element values fixture. The first full run caught the List fixture's length: our binding refused a 3-bit mask with `ShapeMismatch` where Polars panicked on its assertion, which was the intended behaviour on the wrong fixture.

Counts, 0085 to 0086:

| Measure | 0085 | 0086 |
|---|---:|---:|
| Generated operations | 2,092 | 2,096 |
| Unsupported | 1,940 | 1,936 |
| Bindings | 3,961 | 4,002 |
| Refused proven pairs | 255 | 212 |
| Oracle cases | 2,105 | 2,146 |

The refused-pair count fell by 43 rather than the plan's projected 41, because the two Struct pairs moved from refused to excluded.

## Gate 3: behaviour and oracle

`tests/bitmap_inputs.rs` covers three areas:

- **`set_validity` and `with_validity`.** A mask `101` gives null count 1 and a null in row 1, and the script's vector is unchanged. A two-bit and a four-bit mask are refused with the exact message and leave the receiver's mask as it was. `None` clears the mask to null count 0, and an all-false mask gives null count 3. On a two-chunk array, a six-bit mask is split across both chunks (null count 1) while the source array keeps its mask.
- **Constructors.** `from_vec_validity` builds a masked array, accepts `None`, refuses a one-bit mask for three values, and accepts empty values with an empty mask. `from_bitmap` reads `[true, false, false, true]` as values with no nulls, and `[]` as an empty array.
- **Bound.** A bound of 2 refuses a three-bit mask with `3 mask bits with 0 already copied`, a bound of 3 admits it, and the receiver is intact after the refusal.

Oracle: 2,146 cases, all verified. The 41 new cases all match: 15 `set_validity`, 15 `with_validity`, 10 `from_vec_validity` and 1 `from_bitmap`. Among old cases only `LazyFrame::unique_generic` changed, to row_order_differs, the known permutation. Tally: 2,029 match, 98 both_error, 17 both_panic, 2 row_order_differs.

Scoreboard, 0085 to 0086:

| Measure | 0085 | 0086 |
|---|---:|---:|
| Available to a script | 2,103 | 2,107 |
| Value-tested | 1,330 | 1,334 |
| Unsupported | 1,940 | 1,936 |

## Gate 4: cost

Generator self-test, drift and accounting, both adapter suites with fail-fast disabled, and the full oracle pass. The retained 0085 binary's SHA-256 is `a77ea4cf7bfb200468a8407c903b527b3014fc5b10b929e5a6eec259e476831c`. The 0086 binary's is `9b9f48c818ba9b5735d3d519b617a87d41a1254141164b263cbd51c0695d2d92`, and it is unchanged by the test-only fixture fix. Cold launch was measured in three interleaved 60-run sets against a budget of +5 ms:

| Set | 0085 median | 0086 median | Difference | Raw samples |
|---|---:|---:|---:|---|
| 1 | 15.08 ms | 15.63 ms | +0.55 ms | `probes/0086/launch-results-1.json` |
| 2 | 17.87 ms | 17.73 ms | −0.14 ms | `probes/0086/launch-results-2.json` |
| 3 | 18.41 ms | 17.38 ms | −1.03 ms | `probes/0086/launch-results-3.json` |

## Remaining

212 refused proven pairs remain in the Arrow pool. `with_validities` needs a `null_count` refresh contract. `StructChunked::with_outer_validity` needs its field and null-propagation audit. `Column` and `SeriesTrait` validity methods need their own audit. The 0083 table's decisions are unchanged.
