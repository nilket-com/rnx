# 0089 integer function-generic arg extrema: evidence

Plan `eaa540b`. `arg_min_numeric` and `arg_max_numeric`, two generic free functions over `&ChunkedArray<T>`, are static functions on the eight integer wrappers. The element type comes from the wrapper, so a script never names it. All 16 bindings emitted and all 16 new oracle cases match Rust. The 178-pair refused proven-receiver pool is unchanged, as the plan required.

## Gate 1: freeze and prove

`probes/0089/targets.json` holds the 16 rows: the two inventory keys (`polars_core:3153`, `polars_core:3155`), signature, where-bounds (`T: PolarsNumericType`, `&[T::Native]: ArgMinMax`), surface disposition before (`unsupported: generic type: ca`), concrete `T`, wrapper, audit class, emitted binding and oracle outcome.

Source, pinned polars-core 0.55.2 `chunked_array/arg_min_max.rs`:

- Both functions return `None` when `null_count == len`, which covers both empty and all-null input.
- Otherwise a contiguous array goes to `arg_*_numeric_slice` (`:163-283`). That helper answers from the ends under a sorted flag and uses `argmin`/`argmax` otherwise. `cont_slice` succeeds only without nulls, so the slice is non-empty there, and `len() - 1` cannot underflow.
- A chunked array goes to `arg_*_numeric_chunked`, which uses `first_non_null`/`last_non_null` under a sorted flag.
- No integer path reads out of range. On a nullable, sorted-flagged array, `arg_*_numeric_chunked` calls `first_non_null`/`last_non_null` (`chunked_array/mod.rs:314-377`). Those helpers make one `unsafe { self.get_unchecked(out) }` read, but only inside a `debug_assert!` that checks the sorted flag. The index is in range for any valid array with `0 < null_count < len`: `first_non_null` picks `0` or `null_count` (at most `len - 1`), and `last_non_null` picks `len - 1` or `len - null_count - 1`. A script-set sorted flag that is false for a nullable array may trip that assertion (`incorrect sorted flag`) in a debug build, when the chosen index lands on a null; otherwise, and always in a release build, it returns an index that may point at a null, exactly as Polars does. It never reads out of range. The contiguous slice path cannot see nulls, so a false flag there changes only the answer. (This paragraph was corrected after Codex's review, which found the unchecked read the first version missed.)
- `arg_max_numeric`'s `arg_max_float_sorted` branch (`:69-80`, via `float_sorted_arg_max.rs:45-71`) has an unsafe precondition and is reached only for float dtypes, so Float32 and Float64 are excluded.

The generated adapter compiles all 16 instantiations against the pinned build, which proves both trait bounds for each type, and `tests/arg_extrema.rs` calls all eight. All 16 rows are candidates.

## Gate 2: narrow generation

The release file gained `[[free_instantiations]]` entries: inventory `key`, `path`, public `callee`, the `generic` name, its concrete `types`, and a citation. A free function listed there is dispatched to `emit_free_instantiations` in place of `emit_free`. For each type, the emitter builds a synthetic callable:

- **Argument.** `ChunkedArray<T>` in the parameters becomes the wrapper that `by_identity` gives for that type, so `UInt32Type` becomes `IdxCa`.
- **Placement.** The receiver is `none` and the owner is that wrapper, so the binding is `#[rune::function(free, path = W::name)]`.
- **Call.** It calls the public path through the callee override, and Rust infers `T` from `&ca.0`. The argument is borrowed, never cloned or consumed.
- **Result.** While the scoped `World.widen_usize` flag is set, a `usize` result converts with 0087's `support::widen`. That makes the binding fallible, and `None` stays `None`.

The bindings carry their own route label, `free instantiation`. The first full run caught this: under the family label they had been counted as census pairs (909 bindings against 893 emitted pairs). A listed type that no wrapper holds, or a generic that survives substitution, is a named exception.

Synthetic controls (`free-instantiation self-test`):

- A listed function over `Int64Type` and `UInt32Type` emits two static bindings, with the second on its `IdxCa` alias, and no clone and no `as i64`.
- A listed `Float32Type` with no wrapper becomes an exception.
- An unlisted generic free function and a return-only generic stay refused.
- The flag is clear afterwards.

Both adapter configurations compile. Only the two target entries changed status; no float binding exists.

Counts, 0088 to 0089:

| Measure | 0088 | 0089 |
|---|---:|---:|
| Generated operations | 2,100 | 2,102 |
| Unsupported | 1,932 | 1,930 |
| Bindings | 4,036 | 4,052 |
| Oracle cases | 2,180 | 2,196 |

Refused proven pairs stay at 178.

## Gate 3: behaviour and oracle

`tests/arg_extrema.rs` runs seven scenarios in Rune and compares each with the same call made directly in Rust:

- **Covered shapes:** a nullable contiguous array, a two-chunk array, duplicate extrema (`[2, 5, 1, 5, 1]`), and ascending and descending sorted flags set on unsorted data.
- **Edge inputs:** an empty array, and an all-null array built through 0086's masks. Both give `none/none`.
- **Sorted flags:** the flags answer from the ends (`0/2` and `2/0`), as in Polars.
- **Every wrapper:** a second test calls both functions on all eight integer wrappers.

The input wrappers keep their length and null count afterwards. The range check at `i64::MAX + 1` is covered by 0087's `widen` unit test.

Oracle: 2,196 cases, all verified. The 16 new cases all match. Among old cases, only `LazyFrame::unique` changed, from row_order_differs to match: this is the known run-to-run permutation of an unordered case, not part of the claimed delta. Tally of the committed results file: 2,080 match, 98 both_error, 17 both_panic, 1 row_order_differs. The first version of this evidence quoted an earlier run's tally; Codex's review caught the discrepancy.

Scoreboard, 0088 to 0089:

| Measure | 0088 | 0089 |
|---|---:|---:|
| Available to a script | 2,111 | 2,113 |
| Value-tested | 1,338 | 1,340 |
| Unsupported | 1,932 | 1,930 |

## Gate 4: cost

Generator self-test, drift and accounting, both adapter suites with fail-fast disabled, and the full oracle pass. The retained 0088 binary's SHA-256 is `88581a1c6dda4a0dc6df8aa76a7c7b7f1e2f97e9e604ad3af5d13d0a670f453c` and the 0089 binary's is `6e7459968dfbfcc5d25a9a92c429b03c85e490ec2b63313db6505e4c08840e86`, unchanged by the route-label fix. Cold launch was measured in three interleaved 60-run sets against a budget of +5 ms:

| Set | 0088 median | 0089 median | Difference | Raw samples |
|---|---:|---:|---:|---|
| 1 | 18.57 ms | 17.47 ms | −1.10 ms | `probes/0089/launch-results-1.json` |
| 2 | 18.38 ms | 17.32 ms | −1.06 ms | `probes/0089/launch-results-2.json` |
| 3 | 16.62 ms | 16.48 ms | −0.14 ms | `probes/0089/launch-results-3.json` |

## Remaining

Float extrema need an audit of the sorted-float path, and categorical extrema belong to a different type family. The other 0083 function-generic candidates, the numeric-scalar policy and the callback audits await their own decisions. The 178-pair Arrow pool, `with_validities` and the `usize as i64` audit are unchanged.
