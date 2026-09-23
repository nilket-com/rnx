# 0080 Polars callback bindings: evidence

The 0.55.2 adapter now exposes all 41 callback operations that the 0079 audit marked feasible: 24 immediate, 9 stored, and 8 per-family operations. The latter produce 50 family bindings; altogether the operations add 83 bindings. The callback census records `generated` after emission for every feasible row. There is no feasible operation without a binding.

| Measure | 0079 | 0080 | Change |
|---|---:|---:|---:|
| Generated operations | 2,011 | 2,052 | +41 |
| Bindings | 2,661 | 2,744 | +83 |
| Oracle cases | 1,976 | 2,003 | +27 |
| Verified oracle cases | 1,820 | 1,847 | +27 |
| Script-available operations, scoreboard | 2,020 | 2,061 | +41 |
| Operations value-tested on at least one receiver | 1,278 | 1,299 | +21 |
| Unsupported operations | 2,007 | 1,966 | −41 |

All 27 new oracle cases are for callbacks: 26 match the Rust result, and one agrees on an error. The 26 matches cover 24 operations: 9 immediate, 9 stored, and 6 per-family. The other 17 emitted callback operations have no matching value case, chiefly because their closure signature has no callback-safe, non-identity recipe; they remain compiled and are not counted as value-tested. The overall oracle `match` tally is 1,725 versus 1,702 in 0079, a net increase of 23: three older cases that previously matched now report `both_panic` (`ScalarColumn::from_single_value_series`, `DataFrame::slice_par`, and `ColumnStats::from_column_literal`). Their generated cases use the same IDs; this is a changed oracle outcome, not a callback match. The full 0080 tally is 1,725 match, 94 both_error, 26 both_panic, 2 row_order_differs, and 156 fixture_failed.

The per-family applicability census has 50 emitted callback pairs, 102 rejected by existing family applicability, six refused at emission, and two excluded by the release file. The six refusals are: `apply_mut` on String and Binary (callback audit), `for_each` on Binary and BinaryOffset (bare-slice callback argument), `for_each` on List and `apply_into_string_amortized` on List (foreign Arrow array callback argument). `for_each` and `apply_into_string_amortized` on Struct are excluded by the documented `no_call_const` condition. The remaining callback census rows are outside the feasible set: 75 refused, 16 not eligible, and 31 internal-crate operations out of scope. These rows did not gain bindings.

The vector borrow correction is reported apart from the callback counts, and the plan's figure of 68 was an undercount. At the 0079 tip (baa71fc) the generated file had 112 signatures taking a script vector, 120 vector parameters in all: 106 signatures with a direct `Vec<…>`, 7 with an `Option<Vec<…>>`, one with both, and one taking two vectors. Signatures are not operations: those 112 signatures belong to 80 Polars operations by source path, or 91 by emitted method name, because a per-family operation emits one signature per receiver family. At this commit no generated signature takes a `Vec` or `Option<Vec>`: every one of those 120 positions is a `rune::Value` borrowed through `borrow_vec`, the optional ones through the same helper inside the `Option` conversion. The integration controls pass a named vector to `Int64Chunked::from_vec` and read it afterward, including after an element conversion refusal; a closure argument also uses the borrowed-vector path. The bridge has direct unit controls in `support.rs` (`callback_tests`): a nested callback refused before any budget is consulted, with the running callback's guard kept; and the guard and the outer allowance on the calling thread handed back exactly after success, a VM failure, exact-halt exhaustion, a native panic and a typed unwind, the last two under a nonzero inner budget. The harness in `tests/callbacks.rs` covers typed callback failures, wrong return types, capture refusals, budget exhaustion against a user panic that mentions `limited`, rollback of a sorted nullable receiver, routed re-entry through the infallible `Column::reverse` and through the newly routed fallible sink `DslPlan::compute_schema` (whose refusal returns to the callback as a `CallbackError` value, and fails the outer operation only if the callback lets it), and a stored callback failing at a later `collect` and at a later `compute_schema` from another unit. The `compute_schema` control found that Polars wraps the failure in a `Context` error on that path, so the error kind is now derived from the innermost Polars error and both sinks report `CallbackError`. The generator self-test exercises the audit gate and emission shapes. The adapter's full `--all-features` suite, its `--no-default-features` suite, the generated drift/accounting checks, and the 2,003-case oracle pass.

Adjacent release checks use their own inventories and release files. For 0.54.4, the callback census has 167 rows, 36 unresolved, zero feasible and zero emitted; 1,955 operations are generated overall. For rc2, it has 162 rows, 36 unresolved, zero feasible and zero emitted; 2,179 operations are generated overall. Neither release file contains callback audit tables, so the gate leaves callback operations unbound.

Cold launch against the retained 0079 release binary, measured with `probes/0073/launch.py` on the same host using three interleaved sets of 60 empty-script runs each:

The retained 0079 binary has SHA-256 `815950cdc11a7dcc08d1d91cc2fbb12bd6cb267063d3a33ce8074252ed6ac22b`; the measured 0080 binary has SHA-256 `94ba9258fe5c77cced89569eea17e2633a284027e2daccacd7f1d2e8d9391be0`.

| Set | 0079 median | 0080 median | Difference | Raw samples |
|---|---:|---:|---:|---|
| 1 | 18.28 ms | 18.14 ms | −0.14 ms | `probes/0080/launch-results-1.json` |
| 2 | 19.16 ms | 19.61 ms | +0.45 ms | `probes/0080/launch-results-2.json` |
| 3 | 19.34 ms | 18.48 ms | −0.86 ms | `probes/0080/launch-results-3.json` |

The plan's registration budget is +3 ms in each interleaved comparison. These measurements concern process launch and registration, not callback invocation cost.
