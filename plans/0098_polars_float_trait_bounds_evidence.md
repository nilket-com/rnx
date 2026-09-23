# 0098 cited float trait bounds: evidence

Plan `4a2914d`. Six float methods bind on `Float32Chunked` and `Float64Chunked`: 12 bindings and 12 new oracle cases, all matching. The pairs involved are the 12 that were unresolved only because the external `num_traits::float::Float` bound could not be proven. The unresolved pool falls from 576 to 564. The 84 nonfloat owner-bound rejections and the 168 refused proven pairs are unchanged.

## Gate 1: pairs and source

`probes/0098/pairs.json` lists all 96 pairs, six methods on 16 aliases. For each it records the key, method, alias, identity, native, the exact `impl_where`, return, disposition before and after, decision and oracle status. The 12 Float32 and Float64 pairs were `unresolved: trait num_traits::float::Float is outside the inventory` and are now emitted. The 84 others remain `rejected: no recorded impl of PolarsFloatType`.

Source, pinned 0.55.2:

- **The first five methods** (polars-core `chunked_array/float.rs:11-44`) sit under `impl<T> ChunkedArray<T> where T: PolarsFloatType, T::Native: Float`.
- **`is_nan` and `is_not_nan`** (`:16-28`) build a bitmap from the values and attach the input validity.
- **`is_finite` and `is_infinite`** (`:29-34`) use `unary_elementwise_values`, which keeps validity.
- **`none_to_nan`** (`:36-43`) applies `set_at_nulls(arr, T::Native::nan())` (polars-arrow `legacy/kernels/set.rs:16-38`). Valid values are copied unchanged, nulls are filled, and the validity is dropped.
- **`to_canonical`** (`:71-79`) sits under `T::Native: Float + Canonical` and maps `canonical_f32` and `canonical_f64` over the values (polars-utils `total_ord.rs:27-48`). Those add 0.0, which turns −0.0 into +0.0, and map any NaN to `0x7fc00000` or `0x7ff8000000000000`.
- **The trait impls.** `Canonical` is implemented for `f32` and `f64` at `float.rs:57-68`. `num_traits` implements `Float` for both.

The retained 0097 binary's SHA-256 is `1cc4c452688d71dcb545e2d1dc78bc4929897d284a48f54da625b6c347122aec`.

## Gate 2: only the cited external bound

The release file has six `[[external_bounds]]` entries. Each gives key, path, the exact `ret`, the complete exact `where` list, the `discharge` clause, the pairs (`Float32Type`/`f32`, `Float64Type`/`f64`) and a citation. `ExternalBound::check` requires all of the following, and names the fault otherwise:
- the `ChunkedArray<T>` head with impl bounds exactly `T: PolarsFloatType`
- `&self` with no parameters or method generics
- the listed return
- the listed where-clauses, exactly
- discharges that are listed `T::Native` clauses, never the owner bound
- pairs from the fixed `FLOAT_NATIVES` table, without duplicates

`external_bound_entry` refuses a double listing.

In `World::applicability`, a malformed entry leaves every pair `unresolved: external bound: <fault>`. For a well-formed entry, a listed clause counts as proven only on a listed pair, and only when the inventory resolves `T::Native` to that pair's native. Any other result stays unresolved and named. An unlisted type keeps the ordinary proof and is named if unresolved; a nonfloat is still rejected by `T: PolarsFloatType`. Every other clause goes through the ordinary predicate. The bindings are infallible instance methods that return the existing wrappers.

Controls:

- **`external-bound self-test`.** The cited shape passes. Sixteen malformed shapes fail with the fault named: a blank citation, another head, another owner bound, an owned receiver, a parameter, a method generic, a `PolarsResult<Self>` return, `Canonical` removed, `Canonical` replaced, an extra external bound, a discharge outside `where`, discharging the owner bound, no pair, a nonfloat pair, swapped natives and a duplicate pair. An unlisted method and the same path under another key have no entry, and a double listing fails.
- **`external_bound_drift_is_refused_by_name`, on the real generator.** Run A's inventory drops `Canonical` from `to_canonical` and adds `T::Native: core::fmt::LowerExp` to `is_nan`. Its release file swaps `is_finite`'s natives and lists only Float32 for `is_infinite`. Results:
  - Both `to_canonical` and both `is_nan` pairs read `external bound: where-clauses …`, with no binding text.
  - Both `is_finite` pairs read `… is not a float type and its native`, with no text.
  - `is_infinite` emits only on Float32. Float64 reads ``external bound: `ChunkedArray<Float64Type>` is not a listed pair``.
  - `is_not_nan` and `none_to_nan` still emit on both types.

  Run B's inventory replaces `Canonical` with another trait. Both `to_canonical` pairs are refused with no text, while the shipped entries still bind.

## Gate 3: values, validity and bits

Two new fixtures, `series_f64_specials` and `series_f32_specials`, feed no producer. Each is two chunks: `[1.5, -0.0, 0.0, NaN(+, payload 1), null]` and `[-inf, inf, NaN(-, payload 0x123), null, -2.5]`. The NaN bits are `0x7ff8000000000001` and `0xfff0000000000123` for f64, and `0x7fc00001` and `0xff800123` for f32.

A script cannot see a float's bits losslessly. `Column::bit_repr` is the only bit binding, and reading its `u64` bits back hits 0093's check for sign-bit NaNs. So `support::float_bits` (test-support) reads bits and booleans straight out of the wrapper each binding returned.

`tests/float_bounds.rs` compares seventeen results per float type with direct Polars: bools for masks, bits for floats. It runs all six methods on the specials receiver, and all six on an all-null receiver. On an empty receiver it runs `is_nan`, `none_to_nan` and `to_canonical`. It also checks a `to_canonical` that outlives its dropped receiver, and the receiver itself afterwards.

Spelled out for Float64, where null means None:

| Method | Result |
|---|---|
| `is_nan` | `F F F T null F F T null F` |
| `is_not_nan` | `T T T F null T T F null T` (true on infinities) |
| `is_finite` | `T T T F null F F F null T` |
| `is_infinite` | `F F F F null T T F null F` |
| `none_to_nan` | the input bits, each null replaced by a valid `0x7ff8000000000000`, both NaN payloads intact |
| `to_canonical` | −0.0 becomes `0`, both NaNs become `0x7ff8000000000000`, nulls kept |
| all-null receiver | masks `[null, null]`; `none_to_nan` gives two valid NaNs; `to_canonical` keeps both nulls |
| empty receiver | `[]` |
| receiver afterwards | unchanged bits, still two chunks |

Float32 gives the same masks and canonical NaN `0x7fc00000`. Its receiver carries both payloads, and `none_to_nan` keeps `0xff800123`.

## Gate 4: reconcile and cost

Oracle: 12 new cases, one per binding, all matching on the standard fixture. The direct tests above cover NaN, the infinities, nulls, signed zero and payloads. The oracle has 2,341 cases, all verified, and no old status moved. Tally of the committed debug-profile results file: 2,224 match, 98 both_error, 17 both_panic, 2 row_order_differs.

| Measure | 0097 | 0098 |
|---|---:|---:|
| Available to a script | 2,123 | 2,129 |
| Value-tested | 1,367 | 1,373 |
| Unsupported | 1,920 | 1,914 |
| Generated operations | 2,112 | 2,118 |
| Bindings (new) | | +12 |
| Oracle cases | 2,329 | 2,341 |
| Unresolved pairs | 576 | 564 |
| Refused proven pairs | 168 | 168 |

All of these passed:

- The generator self-test: 22 suites.
- The drift check (`--check`).
- The debug suite with `generated,test-support`: 117 tests plus 1 ignored.
- Both release configurations from the README, with fail-fast disabled: 9 tests with default features and 116 plus 1 ignored with `test-support`.
- `git diff --check`.

The 0098 binary's SHA-256 is `b2ef6c8fb68040f8b52a3788ddf496991193400ef207fa7298c5a15f2aa39a50`. Cold launch was measured in three interleaved 60-run sets against a budget of +5 ms:

| Set | 0097 median | 0098 median | Difference | Raw samples |
|---|---:|---:|---:|---|
| 1 | 16.85 ms | 16.61 ms | −0.24 ms | `probes/0098/launch-results-1.json` |
| 2 | 18.82 ms | 18.69 ms | −0.13 ms | `probes/0098/launch-results-2.json` |
| 3 | 18.06 ms | 17.72 ms | −0.34 ms | `probes/0098/launch-results-3.json` |

## Remaining

The random-distribution bounds, the remaining function-generic and callback pools, and the Arrow array and buffer returns keep their dispositions. The unresolved pool is 564 and the refused proven pool is 168.
