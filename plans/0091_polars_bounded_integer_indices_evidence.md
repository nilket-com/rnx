# 0091 bounded integer index conversion: evidence

Plan `91570f5`. `convert_and_bound_idx_ca` is a static function on the eight integer wrappers. Its `target_len` argument is checked below `IdxSize::MAX` before Polars runs. All eight bindings emitted, and all eight new oracle cases match Rust. The 178-pair refused proven-receiver pool is unchanged.

## Gate 1: freeze and prove

`probes/0091/targets.json` holds the eight rows: key `polars_ops:1057`, canonical path, public callee, signature, bounds (`T: PolarsIntegerType`, `T::Native: ToPrimitive`), prior surface reason, concrete `T` and native, wrapper, source branch, audit class, emitted binding and oracle outcome. The public callee `polars::prelude::convert_and_bound_idx_ca` is the one the adapter compiles against, and the direct-Rust tests import it by that path.

Source, pinned polars-ops 0.55.2 `series/ops/index.rs:21-82`:

- **The assertion.** The function asserts `target_len < IdxSize::MAX as usize` (`:32`) after reserving a vector and a validity builder of `ca.len()`.
- **Unsigned branch** (`:35-50`). Each value goes through `to_u64()`, the vector gets `v_u64 as IdxSize`, and validity gets `v_u64 < target_len`.
- **Signed branch** (`:51-68`). Each value goes through `to_i64()`. A negative value is shifted by `target_len`, the vector gets `shifted as IdxSize`, and validity gets `-target_len <= v < target_len`.
- **The combined result.** Validity is ANDed with the input's own (`:70-74`). Without `null_on_oob`, any added null becomes an `OutOfBounds` error (`:76-80`).

Why this is safe for the admitted types:

- **Conversions are total.** Both conversions return `None` only for values outside `u64` or `i64`, and every admitted native (`i8`, `i16`, `i32`, `i64`, `u8`, `u16`, `u32`, `u64`) fits. The branch that pushes a validity bit without a value is therefore unreachable, and the two unchecked pushes stay one per input element.
- **Truncation never becomes valid.** A value truncated by `as IdxSize` (a `u64` at or above 2^32) is always out of range, because `target_len < IdxSize::MAX`, so its validity bit is false.
- **The signed shift cannot overflow.** `i64::MIN + target_len` stays in range for any `target_len` below 2^32.

The only precondition a script could violate is the `target_len` assertion. All eight rows are candidates.

## Gate 2: narrow guard

The `[[free_instantiations]]` entry names `guard_param = "target_len"` and `guard = "below_idx_max"`. While that entry is emitted, `World.arg_guard` holds the operation, parameter and check. The argument mapper then converts that one `usize` parameter in the binding's `pre` statements, before the call: `support::narrow::<usize>` refuses a negative value with `ConversionError`, and `support::below_idx_max` refuses `>= IdxSize::MAX` with `OutOfBounds`, naming the operation, parameter and bound. The scope is cleared by a drop guard after each instantiation. The binding stays fallible, and the result keeps its null mask.

The guard fails closed. Before anything is emitted, `guard_shape` requires both fields or neither, the known check `below_idx_max`, and a `guard_param` that names one of the callable's parameters whose canonical type is `usize`. Any other shape refuses the whole function as `unsupported: free instantiation guard`, with no binding text. Codex's review found that the first version would have emitted an unguarded binding for a missing or misnamed guard.

Synthetic controls (in `native-substitution self-test`): the guarded function emits the `pre` check before `polars::m::bound(&ca.0, __guarded_target_len, flag)`, another listed function with a `usize` parameter gets no guard, and the scope is clear afterwards. Five malformed guards are refused with no binding text: a guard without a parameter, a parameter without a guard, a parameter that does not exist, a parameter that is not `usize` (the `bool` flag), and an unknown check. A typed fixture `series_u64_extremes`, holding `[u64::MAX, 2^32 + 1, 1]`, feeds no producer. It exists so a script can reach values that script integers cannot spell. Both adapter configurations compile, and only the target entry changed status.

Counts, 0090 to 0091:

| Measure | 0090 | 0091 |
|---|---:|---:|
| Generated operations | 2,104 | 2,105 |
| Unsupported | 1,928 | 1,927 |
| Bindings | 4,072 | 4,080 |
| Oracle cases | 2,216 | 2,224 |

Refused proven pairs stay at 178.

## Gate 3: behaviour and oracle

`tests/index_bounds.rs` compares Rune with direct Rust, values and validity both:

- **All eight wrappers.** In-range and out-of-range positive values, negative indices for the signed types, `target_len = 3` with and without `null_on_oob`, `target_len = 0`, and a nullable input with and without `null_on_oob`. The input keeps its length and null count.
- **The shape the comparison relies on**, asserted from Rust: `0,2,n,2,0,n E n,n,n,n,n,n n,n E` for Int64.
- **The guard.** On an empty receiver, `target_len = -1` is a `ConversionError` and `target_len = IdxSize::MAX` is `OutOfBounds`; neither reaches Polars's assertion. `IdxSize::MAX - 1` is admitted and returns an empty array without allocating.
- **Extremes.** `u64::MAX` and 2^32 + 1 come back null under `target_len = 2`, in Rune and in Rust, while `1` stays valid.
- **Multi-chunk.** A four-chunk Int64 receiver with a null matches Rust.

Oracle: 2,224 cases, all verified. The 8 new cases all match. Among old cases, only `LazyFrame::unique` changed, from match to row_order_differs: the known run-to-run permutation of an unordered case, not part of the claimed delta. Tally of the committed results file: 2,107 match, 98 both_error, 17 both_panic, 2 row_order_differs.

Scoreboard, 0090 to 0091:

| Measure | 0090 | 0091 |
|---|---:|---:|
| Available to a script | 2,115 | 2,116 |
| Value-tested | 1,342 | 1,343 |
| Unsupported | 1,928 | 1,927 |

## Gate 4: cost

Generator self-test, drift and accounting, both adapter suites with fail-fast disabled, and the full oracle pass on the first full run. The retained 0090 binary's SHA-256 is `4c890f01eb2d0561201833da7de08abe80e134bec55c0496837b51d18a44ebc9` and the 0091 binary's is `5c2c8fb686a92a80fc43f427988e265755345b493647f6e8f97cfd211fe21582`. Cold launch was measured in three interleaved 60-run sets against a budget of +5 ms:

| Set | 0090 median | 0091 median | Difference | Raw samples |
|---|---:|---:|---:|---|
| 1 | 17.11 ms | 16.37 ms | −0.74 ms | `probes/0091/launch-results-1.json` |
| 2 | 18.11 ms | 17.84 ms | −0.27 ms | `probes/0091/launch-results-2.json` |
| 3 | 18.02 ms | 17.28 ms | −0.74 ms | `probes/0091/launch-results-3.json` |

## Remaining

The other 0083 function-generic candidates, the numeric-scalar policy, the callback audits, the 178-pair Arrow pool, `with_validities` and the generator-wide `usize as i64` audit are unchanged.
