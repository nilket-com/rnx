# 0105 owned snapshots from downcast_into_iter: evidence

Plan `1ebdc20`. `ChunkedArray::downcast_into_iter()` binds on the fourteen numeric, Boolean, String, Binary and BinaryOffset wrappers. The binding consumes a clone of the Rune receiver and streams Polars's owned arrays into an owned `Vec<Vec<Option<script value>>>`. The whole-call bound is checked on the receiver before that clone. There are 14 bindings and 14 new oracle cases, all matching. List and Struct stay refused. The refused proven-pair pool falls from 98 to 84. The operation adds one distinct available and value-tested operation. The 564 unresolved pairs are unchanged.

## Gate 1: pairs, source and ownership

The inventory records `pub fn downcast_into_iter(mut self) -> impl DoubleEndedIterator<Item = T::Array>` on `impl<T: PolarsDataType> ChunkedArray<T>`, with a consuming `self` and no parameters, method generics or where-clauses. The source, polars-core 0.55.2 `chunked_array/ops/downcast.rs:53-61`, takes the chunk vector with `std::mem::take(&mut self.chunks)` and moves each `Box<dyn Array>` out as `T::Array` under Polars's datatype invariant. `ChunkedArray::clone` (`chunked_array/mod.rs:996-1008`) clones the field, the chunk vector, the flags, the length and the null count. Its cost grows with the number of chunks, which is why the bound is checked before it.

`probes/0105/pairs.json` lists all 16 pairs, each with its owned item array taken from the old refusal. It also records the disposition before and after, the decision and the oracle status. 14 pairs are emitted. List and Struct stay refused as not listed.

**Ownership.** The binding consumes `this.0.clone()`, per the adapter's established `self` convention. After every success and every refusal, the Rune receiver still holds its values and its chunk count. The test checks this after a failed `u64` read-back: `get(0)` returns `i64::MAX` and there is still 1 chunk. A result also survives dropping its receiver.

The retained 0104 binary's SHA-256 is `621d4853fa881ea504d48d40683989629317adcaff93ec9976c80d9754937ec5`.

## Gate 2: only this consuming iterator

The release file has one `[[owned_iter_snapshots]]` entry: key `polars_core:3504`, the path, fourteen pairs from the 0099 and 0100 tables, and a citation. `OwnedIterSnapshot::check` requires all of the following, and names the fault otherwise:
- a consuming `self` receiver
- no parameters
- a return whose exact canonical text, whitespace-normalised, is `impl core::iter::traits::double_ended::DoubleEndedIterator<Item = T::Array>`

The text comparison is needed because `Ty::render` drops an `impl` type's item, as found in 0103. The check then applies 0101's checks: the citation, the impl, no generics, and pairs from the fixed tables with each owner carrying its own kind, without duplicates. A double listing is refused.

In `emit_instantiations`, a malformed entry or an unlisted pair is refused by name, and a listed pair sets `World.owned_iter`. With it set, `World::ret` accepts only a parsed `impl` with exactly one `DoubleEndedIterator` bound, no arguments, and an item equal by `Ty` equality to the pair's array as an owned value. A borrowed item fails. The emitter refuses a routed call, a non-`self` receiver and any other conversion. It inserts the kind's preflight at the front of the binding's pre-call statements:

```
let __total = support::preflight_{numeric::<N>|bool|str|binview|binary_offset}(this.0.chunks(), "downcast_into_iter")?;
let __r = <Owner>::downcast_into_iter(this.0.clone());
Ok(support::owned_snapshot*(this.0.chunks(), __total, __r, "downcast_into_iter", …)?)
```

So the preflight runs before the clone and before the Polars call. It downcasts each chunk with a typed error, sums checked cells and bytes, applies one inclusive bound, and allocates nothing proportional to the input. `owned_snapshot*` then streams each owned array as it is yielded, with no intermediate collection. It uses `copy_chunk`, refactored out of 0103's loop and shared by the borrowed and owned copiers: each yielded array must match its preflight chunk's length, and every slot is re-counted. Then `finish_copy` requires the chunk count and the final slot total to equal the preflight. Nothing partial is returned. The 0099 to 0104 budgets are unchanged, and their tests pass on the shared loop.

Controls:

- **`owned-iter self-test`.** The cited shape passes. Eight malformed shapes fail with the fault named: a blank citation, a borrowing `&self`, a parameter, a borrowed `Item = &T::Array`, a plain `Iterator`, a String-as-binary pair, List and a duplicate pair. A double listing fails. In the emitter, for the `i8` and `str` pairs, the preflight text precedes both `this.0.clone()` and the `downcast_into_iter(` call, the call precedes the copier, and there is exactly one clone. A borrowed item, another array, a `&self` receiver, an unscoped return and a string iterator under an `i8` scope are unsupported with no text.
- **`owned_iter_drift_is_refused_by_name`, on the real generator.** Four runs:
  - A blank citation refuses all 16 pairs by name with no text.
  - A UInt8 pair given `i8` does the same.
  - Without the Int64 pair, 13 bind and Int64 is named.
  - An inventory whose item becomes `&T::Array` refuses all 16 by name with no text.
- **Support unit test `owned_snapshots_stream_after_the_preflight`.**
  - The preflight total for `[a, empty, a]` is 7, and the owned copy keeps the order and the middle empty chunk.
  - The preflight alone refuses 7 slots at 6, and 50 empty chunks at 49.
  - An owned iterator yielding 2 or 4 of 3 counted chunks, or a first chunk of the wrong length, is refused.
  - A string chunk as numeric is a typed preflight error.
  - Fewer payload bytes than counted gives `m: 3 slots copied, 5 were counted`.
  - A checked-add overflow is refused without a large array.

## Gate 3: values, receiver reuse and the whole-call bound

`tests/owned_iter_snapshots.rs` is 0104's suite with `downcast_into_iter`. Its Rust reference drives the real owned iterator, `clone().downcast_into_iter()`. It covers all fourteen owners on a two-chunk receiver, the empty receiver `1[]`, a result kept after its receiver is dropped, and the receiver read again after success. The three spelled-out rows equal 0103's and 0104's: Int64 `2[1,2,3|1,n,3]`, String `2["é日本",""|n,"z"]` and BinaryOffset `2[<195 40>,<>|n,<0 0 255>]`.

Extremes match as well: Int8, Float32 `0.1` rounding, and Float64 NaN, infinities and negative zero. `series_u64_boundary` gives `ConversionError: downcast_into_iter: 9223372036854775808 does not fit a script integer` with no partial result, and the receiver then reads `i64::MAX` with 1 chunk.

The two-chunk Int64 call costs 8 slots. At a bound of 7 it is refused with `downcast_into_iter: 2 chunks and 6 cells (8 slots), more than the bound of 7`, which comes from the preflight before any clone, while a single `downcast_get(1)` chunk still fits; at 8 it is copied. The string call costs 15 slots: refused at 14, copied at 15.

## Gate 4: reconcile and cost

**Oracle.** The Rust side drives the real owned iterator, `__r.map(|a| a.iter()….collect()).collect()`, and formats `Vec<Vec<Option<_>>>`. There are 14 new cases, one per binding, all matching. Among old cases only `LazyFrame::unique` moved, from match to row_order_differs, which is the known permutation. The oracle has 2,425 cases, all verified. Tally of the committed debug-profile results file: 2,308 match, 98 both_error, 17 both_panic, 2 row_order_differs.

| Measure | 0104 | 0105 |
|---|---:|---:|
| Available to a script | 2,134 | 2,135 |
| Value-tested | 1,378 | 1,379 |
| Unsupported | 1,909 | 1,908 |
| Bindings (new) | | +14 |
| Oracle cases | 2,411 | 2,425 |
| Refused proven pairs | 98 | 84 |
| Unresolved pairs | 564 | 564 |

**Verification.** All of these passed:

- The generator self-test: 28 suites.
- The drift check (`--check`).
- The debug suite with `generated,test-support`: 152 tests plus 1 ignored.
- Both release configurations from the README, with fail-fast disabled: 9 tests with default features and 151 plus 1 ignored with `test-support`.
- `git diff --check`.

The release build has no warnings.

**Launch.** The 0105 binary's SHA-256 is `b831bcc5e98b3d2f41c1d18dc47022ed177f09ef364821bd02a6c492e91d5ccd`. Cold launch was measured in three interleaved 60-run sets against a budget of +5 ms:

| Set | 0104 median | 0105 median | Difference | Raw samples |
|---|---:|---:|---:|---|
| 1 | 18.00 ms | 17.41 ms | −0.59 ms | `probes/0105/launch-results-1.json` |
| 2 | 18.15 ms | 17.71 ms | −0.44 ms | `probes/0105/launch-results-2.json` |
| 3 | 18.22 ms | 17.91 ms | −0.31 ms | `probes/0105/launch-results-3.json` |

## Remaining

`layout`, the Arrow-array input methods, List and Struct nested arrays, and the function-generic pool keep their dispositions. The refused proven pool is 84 and the unresolved pool is 564.
