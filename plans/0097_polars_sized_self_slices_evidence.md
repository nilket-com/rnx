# 0097 concrete ChunkedArray head, limit and tail: evidence

Plan `d88ed2e`. `limit`, `head` and `tail` bind on all 16 `ChunkedArray` families: 48 bindings across three new operations, with 48 new oracle cases, all matching. The unresolved function-generic pool falls from 624 to 576. The 168 refused proven pairs are unchanged. Each binding checks that the receiver length fits `i64` before calling Polars.

## Gate 1: pairs and source

`probes/0097/pairs.json` lists the 48 pairs: 30 numeric, 9 string and binary, 6 list and struct, 3 boolean. For each it records the key, method, alias, identity, family, parameters and `Self` return, the disposition before and after, the decision and the oracle status. All 48 were `unresolved: function-level generics are out of this record's scope` before. All 48 are now proven by the ordinary `T: PolarsDataType` applicability and emitted. No pair needed a refusal.

The inventory records each method with `receiver: &self`, `generics_canonical: [("Self", "core::marker::Sized")]` and `ret_canonical: Self`. The parameters are `num_elements: usize` for `limit` and `length: Option<usize>` for `head` and `tail`. The impl head is `ChunkedArray<T>`, bounded `T: PolarsDataType`.

Source, polars-core 0.55.2:

- **`limit`** (`chunked_array/ops/chunkops.rs:278-283`) is `self.slice(0, num_elements)`.
- **`head`** (`:287-295`) is `slice(0, min(len, self.len()))`, where `None` means 10.
- **`tail`** (`:299-308`) is `slice(-(len as i64), len)`, with the same `len`.
- **`ChunkedArray::slice`** (`:252`) goes through `slice` (`:52-59`) to `slice_offsets` (`utils/mod.rs:340-357`). There `array_len.try_into().expect("array length larger than i64::MAX")` panics on a receiver longer than `i64::MAX`.

Such a receiver is constructible with shared-buffer appends (record 0093). Each call returns a new owned `ChunkedArray<T>`, which becomes the pair's existing wrapper.

The retained 0096 binary's SHA-256 is `933bcee2e1cec0a004210e60f3bdea1426cb449bc06d32890693330d50827709`.

## Gate 2: the cited `Self: Sized` shape

The release file gained three `[[sized_self_methods]]` entries, each with inventory key, canonical path, the exact parameter types and a citation. `SizedSelfMethod::check` requires all of the following, and names the fault otherwise:
- the `ChunkedArray` owner
- a `&self` receiver
- method generics exactly `[Self: Sized]`
- the listed parameter types
- a return of exactly `Self`

`sized_self_entry` also refuses a key and path listed twice. The rule is applied at three points:

1. **Census.** A well-formed listed method is decided by applicability. A malformed one leaves its pairs `unresolved: sized-self method: <fault>`.
2. **Instantiation.** The entry is re-checked, a `Sized` left in any substituted parameter is refused, the return is set to the pair's alias, and the per-pair scope `World.sized_self` is set and cleared around `emit_method`.
3. **The method emitter.** It requires a `&self` receiver whose return is the owner. It inserts `support::signed_len(this.0.len(), "<method>")?;` before the Polars call, which makes the binding fallible.

An unlisted `Self: Sized` method stays in the generic census. The integer parameters use the existing checked `i64` to `usize` rule.

Controls:

- **`sized-self self-test`.** The cited shape passes. Ten malformed shapes fail with the fault named: a blank citation, the `Logical` owner, a by-value receiver, an extra generic, a `Clone` bound, no generic, a `PolarsResult<Self>` return, a `&Self` return, an altered parameter and an extra parameter. Beyond those:
  - an unlisted method has no entry
  - the same path under another key is not listed
  - a double listing fails
  - in the emitter, the guard precedes the call and makes the binding fallible
  - a return that is not the receiver is unsupported with no binding text
  - the scope is clear afterwards
- **`sized_self_drift_is_refused_by_name`, on the real generator.** It uses a copy of the inventory where `head` returns `polars_error::PolarsResult<Self>`, and a copy of the release file where `tail` has a blank citation. All 16 `head` pairs read `sized-self method: return polars_error::PolarsResult<Self> is not Self`, and all 16 `tail` pairs read `sized-self method: no citation`. Neither emits any text, while `limit` still binds on all 16.
- **Support unit test `the_signed_length_guard_is_exact`.** Lengths 0 and `i64::MAX` pass. `i64::MAX + 1` and `usize::MAX` give `ConversionError`, with the exact message `m: receiver length 9223372036854775808 is beyond i64::MAX, the range of Polars's slice offsets`. No receiver above the bound is built: that would take about 36 GiB even with shared buffers.

## Gate 3: values, refusals and ownership

`tests/sized_slices.rs` compares the script with direct Polars through the same text: the fixtures' show helper on the Rune side, and `oracle::series_repr(&ca.into_series())` on the Rust side. On each receiver it applies twenty operations:
- `limit` at 0, 1, `n` and `n + 5`
- `head` and `tail` with `None`, `Some(0)`, `Some(1)` and `Some(n + 5)`
- on an empty result, `head(Some(2))`, `tail(None)` and `limit(3)`
- `tail(Some(2))` of a receiver dropped before the result is shown
- `limit(-1)`, `head(Some(-1))` and `tail(Some(-5))`, each a `ConversionError`
- `head(Some(n - 1))`

Receivers:

- **All 16 families.** Each receiver is two chunks: the typed fixture appended to itself, asserted as 2 chunks on the Rust side. The families cover Boolean, String, Binary, BinaryOffset, the eight integer types including `IdxCa`, Float32 and Float64, List and Struct payloads.
- **A null across the chunk boundary.** `[1, 2, 3] ++ [1, null, 3]`, where `tail(None)` keeps the null.
- **Int8 extremes.** `[-128, 127, 0, -1]`.
- **Float64.** `[-0.0, NaN, inf, -inf]`.
- **Float32.** `[-0.0, NaN, inf, 0.1 as f32]`.
- **`series_u64_boundary`.** Slicing keeps `18446744073709551615` as a Polars value, since nothing reads it back.

A separate test checks that `limit(-1)`, `head(Some(-1))` and `tail(Some(i64::MIN))` are each a `ConversionError`, and that the receiver's `limit(3)` then equals the untouched fixture.

## Gate 4: reconcile and cost

Oracle: 48 new cases, one per binding, all matching. The cases use the typed fixtures with the standard scalar argument. Among old cases only `LazyFrame::unique` moved, from match to row_order_differs, which is the known run-to-run permutation under its unordered policy. There are 2,329 cases, all verified. Tally of the committed debug-profile results file: 2,212 match, 98 both_error, 17 both_panic, 2 row_order_differs.

| Measure | 0096 | 0097 |
|---|---:|---:|
| Available to a script | 2,120 | 2,123 |
| Value-tested | 1,364 | 1,367 |
| Unsupported | 1,923 | 1,920 |
| Generated operations | 2,109 | 2,112 |
| Bindings (new) | | +48 |
| Oracle cases | 2,281 | 2,329 |
| Unresolved pairs | 624 | 576 |
| Refused proven pairs | 168 | 168 |

All of these passed:

- The generator self-test: 21 suites.
- The drift check (`--check`).
- The debug suite with `generated,test-support`: 114 tests plus 1 ignored.
- Both release configurations from the README, with fail-fast disabled: 9 tests with default features and 113 plus 1 ignored with `test-support`.
- `git diff --check`.

The 0097 binary's SHA-256 is `1cc4c452688d71dcb545e2d1dc78bc4929897d284a48f54da625b6c347122aec`. Cold launch was measured in three interleaved 60-run sets against a budget of +5 ms:

| Set | 0096 median | 0097 median | Difference | Raw samples |
|---|---:|---:|---:|---|
| 1 | 19.10 ms | 18.76 ms | −0.34 ms | `probes/0097/launch-results-1.json` |
| 2 | 18.18 ms | 18.22 ms | +0.04 ms | `probes/0097/launch-results-2.json` |
| 3 | 17.47 ms | 16.39 ms | −1.08 ms | `probes/0097/launch-results-3.json` |

## Remaining

The Arrow array and buffer returns, bitmap-mutating methods, return-only generics, numeric-scalar decisions and the callback-audit pool keep their dispositions. The unresolved pool is 576 and the refused proven pool is 168.
