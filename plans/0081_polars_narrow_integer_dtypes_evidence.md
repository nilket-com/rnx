# 0081 narrow integer dtypes: evidence

Plan `9b5307b`. The adapter's Polars feature set is now `lazy`, `csv`, `parquet`, `dtype-i8`, `dtype-i16`, `dtype-u8` and `dtype-u16`; its bindings, fixtures, oracle and inventory describe that one build. All 156 `fixture_failed` cases of record 0080 execute; none is an unapproved mismatch.

## Gate 1: feature and fixture control

The 156 setup failures were reproduced first on the 0080 tree (commit `99ba4bb`, inventory `0.55.2-adapter`): 39 cases each on `Int8Chunked`, `Int16Chunked`, `UInt8Chunked` and `UInt16Chunked`, both sides failing at setup with `cannot create series from <dtype>`. With the four features the generated fixture recipes (`Series::new(...).cast(&DataType::Int8)` and the three others) construct, and the script control `tests/narrow_dtypes.rs` reads each narrow alias back: each dtype reports as an integer of the right signedness, the values and lengths are those of the fixture, and each alias refuses the other widths, so the four remain distinct script types. The adapter builds and its suites pass with all features and with `--no-default-features`.

## Gate 2: inventory and emission

The documentation runner gained the `adapter-narrow` configuration (`probes/0072/inventory/doc.sh 0.55.2 adapter-narrow`, its own lock `0.55.2-adapter-narrow.lock` and output directory). Under the pinned `nightly-2026-09-20` the run documented all 24 activated Polars crates with none failed; `pins.json` records 13 resolved Polars features, the 0080 configuration's 9 plus exactly the four dtype features. The extractor now copies that resolved set into the inventory's provenance as `features`, beside `release`, `rev` and `cfg`.

Callable identities, matched on canonical path, name, impl head, parameters, return and receiver:

| Inventory | Callables |
|---|---:|
| `0.55.2-adapter` (0080 baseline, kept) | 6,349 |
| `0.55.2-adapter-narrow` | 6,365 |

16 callables are added and none removed; no shared callable changed bucket. The 16 are the 12 `FromIterator` impls of `Series` over `i8`, `i16`, `u8`, `u16`, their `Option` and reference forms (generic bucket, unsupported under the existing rules) and the 4 `From<i8|i16|u8|u16>` conversions of `Expr` (conversion bucket, generated). Generated operations 2,052 to 2,056; unsupported 1,966 to 1,978; adapted 327 and out of scope 710 unchanged; wrappers 270 unchanged; oracle cases 2,003 to 2,007.

The shipped release file `0.55.2-joins.toml` names `cfg = "adapter-narrow"` and pins the complete 13-feature resolved set under `[provenance]`; the generator compares the inventory's recorded set exactly and refuses another configuration, a missing base feature or an extra feature. `tests/generated.rs` proves the configuration refusal against the retained `0.55.2-adapter` inventory (`the_previous_feature_configuration_is_refused`) and the feature-set refusal against the narrow inventory with `lazy` removed and with `dtype-i128` added (`a_wrong_feature_set_under_the_right_configuration_is_refused`); the drift and accounting tests run against the new inventory. The generator self-test passes; the 0079 callback audit and 0080 post-emission census are unchanged.

## Gate 3: oracle

The 2,007-case oracle passes. All 156 baseline IDs exist and every one left `fixture_failed`:

| Alias | match | both_error |
|---|---:|---:|
| `Int8Chunked` | 38 | 1 |
| `Int16Chunked` | 38 | 1 |
| `UInt8Chunked` | 38 | 1 |
| `UInt16Chunked` | 38 | 1 |

The four `both_error` cases are `ChunkedArray::unpack_series_matching_type`, where Rune and Rust agree on `SchemaMismatch`. Ten other cases changed status: `ChunkedArray::from_vec` and `new_vec` on the four narrow aliases went from `both_panic` to `match` (their panic was the same missing dtype support), `BinaryChunked::bin_get` went from `both_panic` to `match`, and `LazyFrame::unique` went from `row_order_differs` to `match`, a run-to-run permutation of an unordered case that is not part of the claimed delta. The four new `Expr::from` cases match. Full tally: 1,891 match, 98 both_error, 17 both_panic, 1 row_order_differs (`LazyFrame::unique_generic`, same permutation class), 0 fixture_failed; 2,007 verified cases against 1,847.

Scoreboard (`probes/0077/scoreboard.py`), 0080 to 0081: available to a script 2,061 to 2,065 (the four `Expr::from` conversions); value-tested on at least one receiver 1,299 to 1,304 (those four plus `bin_get`). Fixture repair alone added no value-tested operation, as the plan predicted: all 39 repaired operations already matched on another family. Verified binding cases rose by 160: the 156 repaired plus the 4 new.

## Gate 4: cost

The 0080 release binary was retained before the rebuild (SHA-256 `94ba9258fe5c77cced89569eea17e2633a284027e2daccacd7f1d2e8d9391be0`, the binary 0080 measured); the 0081 binary is `00e4eb844b2387baa30d6a8697e852ffe3b5a1f9adc03b2ab00bdae3b123bed1`. Cold launch with `probes/0073/launch.py`, three interleaved sets of 60 empty-script runs, budget +5 ms per set:

| Set | 0080 median | 0081 median | Difference | Raw samples |
|---|---:|---:|---:|---|
| 1 | 18.88 ms | 19.33 ms | +0.45 ms | `probes/0081/launch-results-1.json` |
| 2 | 19.00 ms | 17.84 ms | −1.16 ms | `probes/0081/launch-results-2.json` |
| 3 | 15.14 ms | 14.39 ms | −0.75 ms | `probes/0081/launch-results-3.json` |

## Remaining

Adjacent 0.54.4 and rc2 release files are unchanged and name no configuration, so their inventories remain accepted as before. The 12 narrow `FromIterator` impls stay unsupported under the generic-bucket rule; `BinaryOffsetChunked` and the other feature-gated dtypes are outside this record. Running the oracle still permutes the two `LazyFrame::unique` rows between runs.
