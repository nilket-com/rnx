# 0092 typed left subtraction: evidence

Plan `a7b1159`. `ChunkedArray::lhs_sub` (`x - ca`) is an instance method on the ten numeric wrappers, with its scalar generic `N` bound to each wrapper's own native type. Of the 16 family-census pairs, 10 emitted and 6 were rejected on the owner bound; all 10 new oracle cases match Rust. The 178 refused proven pairs are unchanged; the unresolved pool falls from 672 to 656.

## Gate 1: freeze and prove

`probes/0092/targets.json` holds all 16 pairs for `polars_core:3719`: method signature, owner bound, alias, identity, native and bound `N`, disposition before (all `unresolved: function-level generics`) and after, decision and oracle outcome. The 10 numeric pairs emitted. The 6 non-numeric pairs (`BinaryOffset`, `Binary`, `Boolean`, `List`, `String`, `Struct`) are rejected by the applicability engine: there is no impl of `PolarsNumericType`.

Source, pinned Polars 0.55.2:

- **`lhs_sub`** (polars-core `series/arithmetic/borrowed.rs:836-843`) converts `lhs` with `NumCast::from(lhs).expect("could not cast")`, then calls `ArithmeticChunked::wrapping_sub_scalar_lhs`.
- **The kernels** are `lhs.wrapping_sub(x)` for signed and unsigned integers (polars-compute `arithmetic/signed.rs:80-82`, `unsigned.rs:68-70`) and ordinary subtraction for floats (`float.rs:59-65`). Nulls propagate through the arity helpers, and empty arrays stay empty.

Binding `N` to the pair's exact `T::Native` makes that `NumCast` a same-type conversion that cannot fail, so no script value can reach the `expect`. The generated adapter compiles `<Alias>::lhs_sub(&this.0, native)` for all ten.

## Gate 2: narrow generation

The release file gained `[[method_scalar_generics]]` entries: inventory `key`, `path`, function `generic`, owner `types` and their `natives`, and a citation. For a listed key whose entry passes `MethodScalarGeneric::check`, the family census stops marking the pairs unresolved for that one generic, and the normal applicability proof decides them. In `emit_instantiations`, each proven pair's parameters that are exactly `N` in the original signature are set to that pair's native, by position. The family substitution has already replaced `N` with its bound text, so a replacement by name would miss it.

What stays refused, by name:

- A proven pair whose type is not listed.
- A remaining `N` or `NumCast` after binding.
`check` fails closed. It requires exactly one function generic, used only as a whole parameter type and never in the return. Every `types`/`natives` pair must also match the fixed `T::Native` table (`NUMERIC_NATIVES`: `Int8Type` to `i8` through `Float64Type` to `f64`); a type outside the table is refused. Codex's review found that the first version paired the lists only by position. A mispairing such as `Int8Type` with `i64` would compile, since `N` is independent of `T`, and `lhs_sub(1000)` would then panic in Polars's `NumCast::from(..).expect(..)`. The same table now also checks 0090's `[[free_instantiations]]` natives.

The native then maps through the ordinary rules: checked `narrow` for `i8`, `i16`, `i32`, `u8`, `u16`, `u32` and `u64` (a fallible binding), direct for `i64` and `f64`, and `as f32` for `f32`.

Synthetic controls (`method-scalar-generic self-test`) cover a valid entry's natives for `Int8`, `UInt32` (as `IdxCa`) and `Float32`, and no native for an unlisted `Int64`. Ten malformed shapes fail the check: uneven lists, another generic name, two function generics, `N` in the return, `N` inside another parameter, no whole-`N` parameter, `Int8Type` paired with `i64`, `Float32Type` paired with `f64`, a type with no known native, and lists out of order. End to end, `tests/generated.rs` (`a_mispaired_scalar_native_emits_no_binding`) runs the real generator on a copy of the shipped release file with `Int8Type` paired with `i64`. It asserts that `lhs_sub` stays unsupported, all 16 pairs stay unresolved, and no binding text is emitted. Both adapter configurations compile, only `lhs_sub` changed status, and only its 16 pairs changed disposition.

Counts, 0091 to 0092:

| Measure | 0091 | 0092 |
|---|---:|---:|
| Generated operations | 2,105 | 2,106 |
| Unsupported | 1,927 | 1,926 |
| Bindings | 4,080 | 4,090 |
| Oracle cases | 2,224 | 2,234 |
| Unresolved pairs | 672 | 656 |

Refused proven pairs stay at 178.

## Gate 3: values

`tests/lhs_sub.rs` compares Rune with direct Rust:

- **All ten types.** Three scalars per type, including each integer type's extremes, so `-1 - (-128)`, `0 - 1` on `UInt8` (255) and similar cases exercise wraparound. Also a nullable array, an empty array, and the input unchanged afterwards.
- **Floats.** The float rows use NaN and +∞ scalars and `0.1` through `as f32` rounding, and compare in the script's `f64` `Debug` formatting.
- **Refused scalars.** 128 and −129 for `Int8`, −1 and 256 for `UInt8`, and −1 and 2^32 for `IdxCa` each return `ConversionError`, and the receivers read unchanged.
- **Multi-chunk and extremes.** A two-chunk Int64 with a null matches Rust. `5 - [u64::MAX, 2^32 + 1, 1]` on the `series_u64_extremes` fixture wraps as Polars computes it.

**Finding: `UInt64Chunked` read-back wraps.** `lhs_sub` is exact, but the generated `UInt64Chunked::get` converts `u64` to a script integer with `as i64`, so 18446744069414584324 reads back as −4294967292. The test states that read-back explicitly (`show_u64_as_script`) instead of hiding it. This is the unchecked `usize`/`u64 as i64` generator audit noted in 0087, which Codex deferred until a pinned path could reach that range. `lhs_sub` on UInt64 is such a path, as is any `UInt64Chunked` holding values above `i64::MAX`. The oracle cannot catch it, because its Rust side formats `u64` with the same `as i64`. The fix belongs to that audit, not to this record.

Oracle: 2,234 cases, all verified. The 10 new cases all match. Among old cases, only `LazyFrame::unique_generic` changed, from row_order_differs to match: the known run-to-run permutation. Tally of the committed results file: 2,118 match, 98 both_error, 17 both_panic, 1 row_order_differs.

Scoreboard, 0091 to 0092:

| Measure | 0091 | 0092 |
|---|---:|---:|
| Available to a script | 2,116 | 2,117 |
| Value-tested | 1,343 | 1,344 |
| Unsupported | 1,927 | 1,926 |

## Gate 4: cost

Generator self-test, drift and accounting, both adapter suites with fail-fast disabled, and the full oracle pass on the first full run. The retained 0091 binary's SHA-256 is `5c2c8fb686a92a80fc43f427988e265755345b493647f6e8f97cfd211fe21582` and the 0092 binary's is `566f03bfe580dc4e5dd888b6fa06885581a464b563000ccb93cc3f2479eb4803`. Cold launch was measured in three interleaved 60-run sets against a budget of +5 ms:

| Set | 0091 median | 0092 median | Difference | Raw samples |
|---|---:|---:|---:|---|
| 1 | 19.57 ms | 18.53 ms | −1.04 ms | `probes/0092/launch-results-1.json` |
| 2 | 19.06 ms | 20.19 ms | +1.13 ms | `probes/0092/launch-results-2.json` |
| 3 | 18.53 ms | 18.79 ms | +0.26 ms | `probes/0092/launch-results-3.json` |

## Remaining

`lhs_div` and `lhs_rem` need their zero and overflow audits before using this rule. The unchecked `u64`/`usize as i64` read-back now has a demonstrated reachable path and is proposed as the next audit. The other numeric-scalar callables, the function-generic candidates, the callback audits, the 178-pair Arrow pool and `with_validities` are unchanged.
