# 0096 null-aware numeric vectors: evidence

Plan `9a10a6a`. `ChunkedArray::to_vec_null_aware` binds on the ten numeric wrappers and returns one owned `Vec<Option<script value>>`. The ten pairs leave the refused proven-pair pool, which falls from 178 to 168. The six nonnumeric pairs stay rejected on the owner bound, and all ten new oracle cases match. The whole result is checked against the materialize bound before Polars is called.

## Gate 1: source, pairs and branches

Source, polars-core 0.55.2 `chunked_array/to_vec.rs:15-27`:

- **Left.** `to_vec_null_aware` returns `Either::Left(buf)`, built with `extend_from_slice(arr.values())` over every chunk, exactly when `self.null_count() == 0`.
- **Right.** Otherwise it returns `Either::Right(self.to_vec())` (`to_vec.rs:7-13`), a `Vec<Option<T::Native>>` built chunk by chunk.

Both branches are owned `Vec`s and hold no borrow of the receiver.

`probes/0096/pairs.json` lists all 16 family pairs. For each it records the alias, identity, return shape after substitution, disposition before and after, decision and oracle status. The ten numeric pairs were `refused: unsupported: foreign type: return (either::Either<alloc::vec::Vec<N>, alloc::vec::Vec<core::option::Option<N>>>)` and are now emitted. The six others (`BinaryOffset`, `Binary`, `Boolean`, `List`, `String`, `Struct`) remain rejected: there is no `PolarsNumericType` impl. The 624 unresolved function-generic pairs are unchanged.

**Branch proof.** The Rust side of `tests/null_aware.rs` calls `to_vec_null_aware` directly on every numeric wrapper. For each type it observes, in order:

- `L` for a no-null input
- `R` for the same values with a null
- `L` for an empty input
- `R` for a two-chunk receiver that contains a null

That is the tag string `LRLR` for all ten types, matching the source's `null_count` choice. `series_u64_extremes`, which has no nulls, is `L`. `series_u64_boundary`, which has a null, is `R`.

The retained 0095 binary's SHA-256 is `324f1a706390a7f1b257361b086dea5b0d681a8a63a2cf15b73cb5daf41c843b`.

## Gate 2: the exact return shape

**The release entry.** The release file has one `[[null_aware_returns]]` entry: key `polars_core:3658`, the canonical path, the ten numeric owner types and a citation. `NullAwareReturn::check` fails closed on a blank citation, an empty list, a nonnumeric type or a duplicate. In `emit_instantiations`, a malformed entry refuses every pair, naming the fault. A proven pair whose type is not listed is refused by name.

**The scope.** A listed pair sets `World.null_aware` to (method, the pair's native from `NUMERIC_NATIVES`) around its `emit_method` call and clears it afterwards. With the scope set, `World::ret` accepts an `either::Either` only if it is exactly `Either<Vec<N>, Vec<Option<N>>>` for that native. It maps each element through the existing scalar rule:
- `as i64` for narrow integers
- `support::widen::<u64>(..)?` for `u64`
- `as f64` for `f32`

It uses `Either`'s inherent `either`, so the adapter never names the `either` crate. The binding is always fallible.

**The bound.** The method emitter inserts `support::null_aware_bound(this.0.len(), "to_vec_null_aware")?;` before the Polars call. It refuses with `MaterializeLimit` when the length exceeds the inclusive bound. An `Either` outside the scope stays a refused foreign type. None of these ten bindings is routed, so the call and both vectors are owned in the binding itself.

**Codex round 1: the whole return is checked.** Codex found that the scope was checked only where an `Either` appeared. A listed pair whose substituted return was `Option<Either<..>>`, or a fallible return with no `Either` such as `PolarsResult<i64>`, could still emit. The generator now checks at three points:

1. `emit_instantiations` compares the pair's whole substituted return with the exact `Either<Vec<N>, Vec<Option<N>>>` (`null_aware_return_matches`) before setting the scope. A mismatch is refused by name: ``refused: null-aware return: `<type>` is not Either<Vec<N>, Vec<Option<N>>>``.
2. `World::ret` accepts the `Either` arm only at depth 0, so a nested one is unsupported.
3. The emitter adds the bound only when the return is that exact shape and its conversion is the null-aware one, not merely fallible.

The shipped output is byte-identical: the drift check passes, and the binary and results below are unchanged.

Controls:

- **`null-aware self-test`, from a synthetic inventory.** The entry's check refuses a missing citation, an empty list, a nonnumeric type and a duplicate. `native_for` resolves only listed types. For `i8`, `u64` and `f32` the binding is a fallible `Vec<Option<..>>`, the bound comes before the call, and both branches go through the scalar rule, with `u64` checked. Five cases are unsupported with no binding text:
  - an `i16` return in an `i8` scope
  - a Right branch that is not optional
  - mixed natives
  - a by-value receiver
  - an `Either` nested in `Option` (round 1)
  - a `PolarsResult<i64>` return in an `i64` scope (round 1)
  - an `Either` with no scope

  The pair-level check `null_aware_return_matches` accepts only the exact top-level shape. It rejects an `Either` nested in `Option` or in `PolarsResult`, a fallible return without `Either`, another native and a unit return. The scope is clear afterwards.
- **`null_aware_entries_fail_closed_and_refuse_unlisted_types`, on the real generator.** A copy of the shipped release file with a blank citation emits no `to_vec_null_aware` text, and all ten pairs name `null-aware return: no citation`. A copy without `Int64Type` emits nine bindings and refuses the Int64 pair as not a listed type.
- **Support unit test `the_null_aware_bound_is_inclusive`.** Under a bound of 3, 3 passes and 4 is refused with the exact message. With the override reset to 0, the production bound applies.

## Gate 3: values, errors and bounds

`tests/null_aware.rs` compares the script with direct Rust on every wrapper. Each type is checked on a no-null array, the same array with its second slot null, an empty array, and a two-chunk receiver built by `Series::append` then `cast` to the type (`[1, 2, 3] ++ [1, null, 3]`). A second read of the first array confirms the receiver is reusable. The values include:

- signed `MIN`, −1, 0 and `MAX`
- unsigned 0, 1, 2 and `MAX` (`UInt64` up to `i64::MAX`)
- Float32 −0.0, NaN, inf and `0.1 as f32`, which reads back as `0.10000000149011612`
- Float64 −0.0, NaN, −inf and 0.1

Floats are compared in `f64` Debug text, which keeps the sign of zero, NaN and infinity. Two rows are spelled out in the test:

| Type | Result |
|---|---|
| Int8 | `-128,-1,0,127 \| -128,n,0,127 \| (empty) \| 1,2,3,1,n,3 \| 2 chunks \| -128,-1,0,127` |
| Float32 | `-0.0,NaN,inf,0.10000000149011612 \| -0.0,n,inf,0.10000000149011612 \| … \| 1.0,2.0,3.0,1.0,n,3.0` |

**UInt64 beyond a script integer.** `series_u64_extremes` (Left) fails with `ConversionError: to_vec_null_aware: 18446744073709551615 does not fit a script integer`. `series_u64_boundary` (Right) fails the same way at `9223372036854775808`, both times. The call returns no partial vector: Rust holds `ConversionError,4294967297,1` and `9223372036854775807,ConversionError,ConversionError,n`. The receiver's `get(0)` still returns `i64::MAX` afterwards, and an in-range UInt64 converts to `1,2,3`.

**Bounds**, set with `polars::set_materialize_limit`:

| Input | Bound | Result |
|---|---:|---|
| Left, length 3 | 2 | `MaterializeLimit: to_vec_null_aware: 3 items, more than the bound of 2` |
| Left, length 3 | 3 | values |
| Right, length 4 | 3 | refused |
| Right, length 4 | 4 | `1.0,n,3.0,4.0` |
| two-chunk, length 6 | 5 | refused |
| two-chunk, length 6 | 6 | `1,2,3,1,n,3` |
| empty | production (reset with 0) | `[]` |

After the refusals, the first receiver still returns its values.

## Gate 4: reconcile and cost

**Oracle.** The oracle's Rust side frames a null-aware result as the merged vector of options, `either(|v| v.into_iter().map(Some).collect(), |v| v)`, then as `Vec<Option<N>>`. There are ten new cases, one per binding, and all match. The standard fixture has no nulls and so reaches only the Left branch; the direct tests above cover Right. The oracle has 2,281 cases, all verified, and no old status moved. Tally of the committed debug-profile results file: 2,165 match, 98 both_error, 17 both_panic, 1 row_order_differs (`LazyFrame::unique_generic`).

| Measure | 0095 | 0096 |
|---|---:|---:|
| Available to a script | 2,119 | 2,120 |
| Value-tested | 1,363 | 1,364 |
| Unsupported | 1,924 | 1,923 |
| Generated operations | 2,108 | 2,109 |
| Bindings (new) | | +10 |
| Oracle cases | 2,271 | 2,281 |
| Refused proven pairs | 178 | 168 |
| Unresolved pairs | 624 | 624 |

**Verification.** All of these passed:

- The generator self-test: 20 suites.
- The drift check (`--check`).
- The debug suite with `generated,test-support`: 109 tests plus 1 ignored.
- Both release configurations from the README, with fail-fast disabled: 9 tests with default features and 108 plus 1 ignored with `test-support`.
- `git diff --check`.

**Launch.** The 0096 binary's SHA-256 is `933bcee2e1cec0a004210e60f3bdea1426cb449bc06d32890693330d50827709`. Cold launch was measured in three interleaved 60-run sets against a budget of +5 ms:

| Set | 0095 median | 0096 median | Difference | Raw samples |
|---|---:|---:|---:|---|
| 1 | 18.19 ms | 16.89 ms | −1.30 ms | `probes/0096/launch-results-1.json` |
| 2 | 16.67 ms | 16.74 ms | +0.07 ms | `probes/0096/launch-results-2.json` |
| 3 | 15.43 ms | 15.01 ms | −0.42 ms | `probes/0096/launch-results-3.json` |

## Remaining

The other Arrow arrays, buffers, borrowed array views, `with_validities`, and the function-generic and callback pools keep their dispositions. The refused proven-pair pool is now 168.
