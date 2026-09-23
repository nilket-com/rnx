# 0095 typed left division and remainder: evidence

Plan `9a440a8`. `ChunkedArray::lhs_div` (`x / ca`) and `lhs_rem` (`x % ca`) are instance methods on the ten numeric wrappers, with the scalar generic `N` bound to each wrapper's own native type by 0092's exact rule. Of the 32 family pairs, 20 were emitted and 12 were rejected on the owner bound; all 20 new oracle cases match Rust. The 178 refused proven pairs are unchanged. The unresolved pool falls from 656 to 624.

## Gate 1: freeze, source and probe

`probes/0095/targets.json` lists all 32 pairs. For each it records the key, the method signature (`fn lhs_div<N: Num + NumCast>(&self, lhs: N) -> Self`, likewise `lhs_rem`), the owner bound (`T: PolarsNumericType`), the alias, `T`, `T::Native` and the bound `N`. It also gives the disposition before and after, the decision and the oracle status. All 32 were `unresolved: function-level generics` before. The ten numeric wrappers per method were emitted. The six nonnumeric wrappers were rejected: `BinaryOffset`, `Binary`, `Boolean`, `List`, `String` and `Struct` have no `PolarsNumericType` impl.

Source, pinned 0.55.2:

- **The methods.** `lhs_div` and `lhs_rem` (polars-core `series/arithmetic/borrowed.rs:845-849`, `852-856`) convert with `NumCast::from(lhs).expect(..)`. They then call `legacy_div_scalar_lhs` or `wrapping_mod_scalar_lhs`. With `N = T::Native` that conversion is same-type and cannot fail.
- **Dispatch.** `legacy_div_scalar_lhs` (polars-compute `arithmetic/mod.rs:65-76`) sends floats to true division, `lhs / x` (`float.rs:117-119`). Integers go to `wrapping_floor_div_scalar_lhs`.
- **Signed integers** (`signed.rs:131-140`, `199-208`). The validity is the input's validity AND `x != 0`, so a zero divisor is null. `lhs == 0` fills zeros. Otherwise each physical value, including null and zero slots, goes through `FloorDivMod::wrapping_floor_div_mod` (polars-utils `floor_divmod.rs:39-68`):
  - `other == 0` returns `(0, 0)` before dividing.
  - `wrapping_div` and `wrapping_rem` make `MIN / -1` wrap to `MIN` with remainder 0.
  - The floor adjustment (`div -= 1`, `mod_ += other`) runs only for a nonzero remainder with operands of opposite sign. There the divisor's magnitude is at least 2, so the quotient's magnitude is at most `|MIN| / 2`. The remainder's magnitude is below the divisor's, and the two have opposite signs. Neither step can overflow, so no debug overflow panic is reachable.
- **Unsigned integers** (`unsigned.rs:97-106`, `127-136`). The same validity mask applies, and each value is computed as `if x != 0 { lhs / x } else { 0 }`, so nothing divides by zero.
- **Floats.** Remainder is `lhs - x * (lhs / x).floor()` (`float.rs:101-103`). Nothing is masked: IEEE results for zero, NaN and infinity pass through.

`probes/0095/kernel_probe.rs` calls both methods directly with `N = T::Native` on all ten types. Its inputs include:

- zero, ±1, 2, −3, `MIN` and `MAX`, and a null slot, against scalars 0, ±7, `MIN` and `MAX`
- ±0.0, 2, −3, NaN and ±inf for the floats
- an empty array and a two-chunk array

It ran in debug, where overflow checks are on, and in release. Both exited 0 with no panic. `kernel-probe-debug.txt` and `kernel-probe-release.txt` are byte-identical, 46 rows each. Observed behaviour:

- An integer zero divisor gives null.
- `-128 / -1` on Int8 gives −128 with remainder 0.
- Floor semantics: `-7 / 2` is −4, `7 % -3` is −2 and `-7 % 3` is 2.
- On floats, `7 / 0.0` is inf, `7 / -0.0` is −inf, `0 / 0` is NaN and `x % 0` is NaN. `7 % inf` is NaN because of the floor formula.

The retained 0094 binary's SHA-256 is `5ce1c4b57d115ff399eaa2d3cefdcf113f1f1425db2f8f4076508d062d58fb54`.

## Gate 2: the existing exact policy

The release file gained two `[[method_scalar_generics]]` entries, `polars_core:3722` and `polars_core:3723`. Each lists the ten types and natives with 0092's table and cites the kernels above. No generator rule changed: `MethodScalarGeneric::check`, `NUMERIC_NATIVES` and the positional binding of `N` are 0092's. The bindings follow the ordinary scalar rules:
- `support::narrow` for `i8` through `u64`, which makes those bindings fallible.
- Direct `i64` and `f64`.
- `as f32` for Float32.

Controls:

- **`method-scalar-generic self-test`, extended.** For both names the shipped ten-type entry passes `check`. `native_for` gives `i8`, `u32` (`IdxCa`), `i64`, `u64`, `f32` and `f64` for those scalar classes, and nothing for `BooleanType`. A swapped `Int8Type`/`i64` pair fails closed. 0092's ten malformed shapes still fail.
- **`a_mispaired_scalar_native_emits_no_binding`.** Now a shared helper runs the real generator on a modified copy of the shipped release file. With `Int8Type` paired with `i64` on `lhs_div`, `lhs_div` is unsupported and emits no text, while `lhs_rem` still emits all ten bindings.
- **`an_unlisted_scalar_type_is_refused_by_name`.** Removing `Int64Type` from the `lhs_rem` entry leaves nine `lhs_rem` bindings. The Int64 pair is then `refused: … method scalar generic: ChunkedArray<Int64Type> is not a listed type`.

Counts, 0094 to 0095:

| Measure | 0094 | 0095 |
|---|---:|---:|
| Generated operations | 2,106 | 2,108 |
| Unsupported | 1,926 | 1,924 |
| Bindings (new) | | +20 |
| Oracle cases | 2,251 | 2,271 |
| Unresolved pairs | 656 | 624 |
| Refused proven pairs | 178 | 178 |

## Gate 3: values and validity

`tests/lhs_div_rem.rs` compares Rune with direct Rust for all 20 bindings. Each binding is checked on four scalars against a six-value array, on the same array with its second slot null, on an empty array, and on the receiver unchanged afterwards. The inputs cover:

- a zero divisor
- ±1 and signed `MIN` and `MAX`
- `lhs` equal to 0, `MIN` and a negative value
- unsigned maxima (`UInt64` up to `i64::MAX`, the largest a script can spell)
- for Float32 and Float64: NaN, inf, −inf, 0.0, −0.0 and a `-0.0` scalar, compared in `f64` Debug text, which keeps the sign of zero

Integers read back through 0093's checked widening. The test spells out the Int8 and Float64 rows. For Int8, `7 / [0, 2, -3, -1, 127, -128]` is `n,3,-3,-7,0,-1`, and `-128 / -1` is −128. For Float64, `7 / [.., 0.0, -0.0, ..]` is `inf,-inf`.

A two-chunk Int64 receiver with a null matches Rust: `7 / ca` is `7,3,2,7,n,2` and `-7 % ca` is `0,1,2,0,n,2`. Scalars out of range are each a `ConversionError`, with every receiver unchanged: 128 and −129 on Int8, −1 and 256 on UInt8, 2^32 on `IdxCa`, and −1 on UInt64. `Int8.lhs_div(-128)` still works afterwards.

A `UInt64` result above `i64::MAX` cannot arise here. Unsigned floor division and remainder never exceed `lhs`, and a script's `lhs` is at most `i64::MAX`. So 0093's checked read-back never fires on these results.

## Gate 4: reconcile and cost

Oracle: 2,271 cases, all verified, and the 20 new cases all match. Among old cases only `LazyFrame::unique` moved, from row_order_differs to match, which is the known run-to-run permutation under its unordered policy. Tally of the committed debug-profile results file: 2,155 match, 98 both_error, 17 both_panic, 1 row_order_differs.

Scoreboard (`probes/0077/scoreboard.py`):

| Measure | 0094 | 0095 |
|---|---:|---:|
| Available to a script | 2,117 | 2,119 |
| Value-tested | 1,361 | 1,363 |
| Unsupported | 1,926 | 1,924 |

All of these passed:

- The generator self-test: 19 suites.
- The drift check (`--check`).
- The debug suite with `generated,test-support`: 104 tests plus 1 ignored.
- Both release configurations from the README, with fail-fast disabled: 9 tests with default features and 103 plus 1 ignored with `test-support`.
- `git diff --check`.

The 0095 binary's SHA-256 is `324f1a706390a7f1b257361b086dea5b0d681a8a63a2cf15b73cb5daf41c843b`. Cold launch was measured in three interleaved 60-run sets against a budget of +5 ms:

| Set | 0094 median | 0095 median | Difference | Raw samples |
|---|---:|---:|---:|---|
| 1 | 18.78 ms | 19.41 ms | +0.63 ms | `probes/0095/launch-results-1.json` |
| 2 | 19.47 ms | 18.62 ms | −0.85 ms | `probes/0095/launch-results-2.json` |
| 3 | 17.06 ms | 15.43 ms | −1.63 ms | `probes/0095/launch-results-3.json` |

## Remaining

The other numeric-scalar candidates, the family-generic and callback audits, and the 178-pair Arrow pool are unchanged. The refusal reason for an unlisted scalar type repeats its `refused:` prefix, a cosmetic issue carried over from 0092.
