# 0090 numeric peaks with explicit boundaries: evidence

Plan `90fdbbb`. `peak_max_with_start_end` and `peak_min_with_start_end`, two generic free functions from `polars_ops`, are static functions on the ten numeric wrappers. `T` is fixed by the array argument, and `T::Native` resolves to each type's native scalar. All 20 bindings emitted, and all 20 new oracle cases match Rust. The 178-pair refused proven-receiver pool is unchanged.

## Gate 1: freeze and prove

`probes/0090/targets.json` holds the 20 rows: the keys `polars_ops:77` and `polars_ops:81`, signature, where-bound (`ChunkedArray<T>: ChunkCompareIneq<&ChunkedArray<T>, Item = BooleanChunked>`), prior surface reason (`unsupported: generic type: ca`), concrete `T` and native, wrapper, boundary conversion, audit class, emitted binding and oracle outcome.

Source, pinned Polars 0.55.2:

- **`peaks.rs`** (polars-ops `chunked_array/peaks.rs:36-60`) shifts the array by `1` and by `-1` with `shift_and_fill`, filling the vacated end with the optional boundary. It then combines strict `lt` (for max) or `gt` (for min) comparisons with `&`.
- **`shift_and_fill`** (polars-core `chunked_array/ops/shift.rs:5-39`) builds new arrays and leaves the input unchanged.
- **Comparisons** reach the `tot_lt_kernel` and `tot_gt_kernel` total-order kernels (polars-core `chunked_array/comparison/mod.rs:136-188`). NaN therefore orders consistently rather than comparing false. Nulls propagate through the comparisons and `&`.

No sorted flag, unsafe code or unchecked index is on this path. The public callee is `polars::prelude::peaks::*`. The generated adapter compiles all 20 instantiations against the pinned build, which proves the where-bound for each type, and `tests/peaks.rs` calls all ten. All 20 rows are candidates.

## Gate 2: constrained generation

0089's `[[free_instantiations]]` entries gained `natives`, the native scalar of each listed type in order. In `emit_free_instantiations`, both substitutions go through `replace_token`: `ChunkedArray<T>` becomes the wrapper, and `T::Native` becomes the native. A replacement happens only where the spelling stands as a whole token, not preceded by an identifier character or `:` and not followed by an identifier character. A remaining `T` or `::Native` in any parameter makes that type a named exception.

The substituted scalars go through the ordinary argument rules:

- `i8`, `i16`, `i32`, `u8`, `u16`, `u32` and `u64` use checked `support::narrow`, which makes the binding fallible.
- `i64` and `f64` pass directly.
- `f32` uses the adapter's existing `as f32` from a script float.

The Int64, Float32 and Float64 bindings are therefore infallible and return the `BooleanChunked` directly, while the other seven return a result. There is no engine routing, and the array is borrowed.

Synthetic controls (`native-substitution self-test`):

- `replace_token` substitutes whole tokens and leaves `my::XT::Native`, `T::NativeExt` and `a::T::Native` alone.
- A listed peak-shaped function over `Int8`, `UInt32` (as `IdxCa`) and `Float32` emits three bindings with `narrow::<i8>`, `narrow::<u32>` and `as f32` through `Option`, and no generic remains.
- A listed function whose parameter keeps `T::Physical` is refused, with each type named as a residual exception.

Both adapter configurations compile, and only the two target entries changed status.

Counts, 0089 to 0090:

| Measure | 0089 | 0090 |
|---|---:|---:|
| Generated operations | 2,102 | 2,104 |
| Unsupported | 1,930 | 1,928 |
| Bindings | 4,052 | 4,072 |
| Oracle cases | 2,196 | 2,216 |

Refused proven pairs stay at 178.

## Gate 3: behaviour and oracle

`tests/peaks.rs` compares Rune with direct Rust:

- **All ten types, nine scenarios each.** Duplicate peaks `[1, 3, 2, 3, 1]` for max and for min, with no boundaries and with boundaries that change the first and last results. A singleton with and without boundaries. An empty array. A nullable array, with and without a boundary. The input array keeps its length, values and null count.
- **Multi-chunk.** A two-chunk Int64 array, built from an array plus a nullable array, matches Rust with and without boundaries.
- **Refused boundaries.** 256 for `UInt8`, −1 for `IdxCa` and 128 for `Int8` each return `ConversionError`, and the input stays usable.
- **Float boundaries.** NaN, +∞ and −∞ as `Float32` boundaries give the same masks as Rust.

Oracle: 2,216 cases, all verified. The 20 new cases all match, and no old case changed status. Tally of the committed results file: 2,100 match, 98 both_error, 17 both_panic, 1 row_order_differs.

Scoreboard, 0089 to 0090:

| Measure | 0089 | 0090 |
|---|---:|---:|
| Available to a script | 2,113 | 2,115 |
| Value-tested | 1,340 | 1,342 |
| Unsupported | 1,930 | 1,928 |

## Gate 4: cost

Generator self-test, drift and accounting, both adapter suites with fail-fast disabled, and the full oracle pass on the first full run. The retained 0089 binary's SHA-256 is `6e7459968dfbfcc5d25a9a92c429b03c85e490ec2b63313db6505e4c08840e86` and the 0090 binary's is `4c890f01eb2d0561201833da7de08abe80e134bec55c0496837b51d18a44ebc9`. Cold launch was measured in three interleaved 60-run sets against a budget of +5 ms:

| Set | 0089 median | 0090 median | Difference | Raw samples |
|---|---:|---:|---:|---|
| 1 | 20.38 ms | 20.60 ms | +0.22 ms | `probes/0090/launch-results-1.json` |
| 2 | 20.01 ms | 18.80 ms | −1.21 ms | `probes/0090/launch-results-2.json` |
| 3 | 21.49 ms | 19.96 ms | −1.53 ms | `probes/0090/launch-results-3.json` |

## Remaining

The other 0083 function-generic candidates, the six numeric-scalar callables, the callback audits, float and categorical extrema, the 178-pair Arrow pool, `with_validities` and the `usize as i64` audit are unchanged.
