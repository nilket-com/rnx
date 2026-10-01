# rnx 0127 evidence: pivot, with an `Arc<DataFrame>` argument and higher-arity binding

**The measure is met.** At v2, `w6.pivot` and the composed W6 (a left join, pivoted wide, then unpivoted) compute, match their Rust twins and display both ways.
- **All eight composed workflows** are now usable at v2: W1 through W8.
- **Every computed frame displays:** 42 of 42.
- **The only remaining v2 gap in the probe** is C2, the byte-based `JsonReader` step, deferred by the user.

**The two blockers, settled together:**
- **`on_columns: Arc<DataFrame>`** takes the script's `DataFrame` as a shared snapshot: `Arc::new(frame.clone())`.
- **The method's 9 arguments,** receiver included, are bound through a raw shim that follows Rune's typed calling convention step by step.

**Replay:** `probes/0127/replay.sh` runs:
- the twins;
- the v2 pivot test;
- the before state (0126's tree, `28dcf0f`) and the after state, at both pins.

An init run and a non-init check both pass, byte for byte.

## 1. `Arc<DataFrame>` (H): `[[arc_arguments]]`

**The family** (`tools/polars-gen/src/families/arc_arguments.rs`, callable scope, `ARG_TOP`): a listed callable's by-value `Arc<T>` parameter accepts the script's wrapped `T`. The argument is `&W`, borrowed, so the script keeps its value. The conversion is `std::sync::Arc::new(x.0.clone())`, a shallow clone the `Arc` owns.

**Validated fail-closed:**
- each row cited and unique;
- the target wrapped and `Clone`;
- the callable present exactly once, with the named parameter taking `Arc<target>` by value;
- an unused row is refused after generation.

**The only row (v2):** `LazyFrame::pivot`'s `on_columns`. Polars only reads it and validates it as errors (`dsl_to_ir/mod.rs:942-1010`).
- At v2 the only other `Arc<DataFrame>`-by-value parameter is `DslBuilder::pivot`'s, an internal plan builder, which stays refused.
- At 0.55.2 the table is empty: the shipped build has no Polars `pivot` feature.

**Self-test `arc_arguments_self_test`** covers:
- the refusals: no citation, an unwrapped target, a target that isn't `Clone`, a borrowed `&Arc`, a missing parameter, a missing callable, an unused row;
- the argument's shape and conversion.

## 2. Higher arity (I): `[[wide_bindings]]` and the raw shim

**Why.** Rune 0.14.2 implements its typed `Function`, and so `InstanceFunction`, only up to arity 5 (`function/macros.rs`, `permute!`: 243 combinations at arity 5).

**The family** (`families/wide_bindings.rs`):
- **What changes:** a listed method above the typed limit is generated as the same Rust function as always (signature, conversions, engine routing, return), but the `#[rune::function]` attribute is replaced. That attribute builds its metadata through the typed `Function` bounds, so for arity 9 it doesn't compile.
- **The registration:** `m.raw_function("pivot", s_…).build_associated::<LazyFrame>()?`, an instance function, so `lf.pivot(…)` works.
- **The shim follows rune's typed `fn_call`** (`function/mod.rs` `access_memory!`, `impl_function_traits!`):
  1. the count first: `RuntimeError::bad_argument_count`, the same `BadArgumentCount` kind;
  2. every slot taken through `slice_at_mut`, replaced with `Value::empty()`;
  3. each slot converted in order, receiver at index 0: `&W` with `borrow_ref`, `&mut W` with `borrow_mut`, `&str` with `borrow_string_ref`, `rune::Value` as is, anything else with `FromValue`;
  4. the call, with every guard alive through it and dropped before the result is converted;
  5. `ToReturn::to_return`, so a `VmResult` stays a VM error, then `out.store`.
- **Validated fail-closed:**
  - each row cited and unique;
  - the callable present once, and a method;
  - exactly the listed arity, above 5 and at most 16;
  - every parameter type has a slot conversion, otherwise refused by name;
  - an unused row is refused.

  The only row (v2) is `LazyFrame::pivot`, arity 9. The 12 other v2 callables refused only by arity (`Expr::qcut`, `datetime_range`, and others) are later candidates.

**The one documented difference.** The typed path tags a conversion failure with `VmErrorKind::BadArgument { arg }` through `VmError::with_error`, which is `pub(crate)` in rune 0.14.2 (`runtime/vm_error.rs:299`); `VmErrorKind` is crate-private too. The shim reports the conversion's own error. Accepted in plan review.

**The typed-versus-raw control** (`tests/wide_shim.rs`, 5 of 5 at both pins). One function, `support::wide_control(&DataFrame, &str, i64, DataFrame) -> Result<i64, Error>`, and its `VmResult` twin are each registered twice:
- typed, by the adapter;
- raw, through shims the generator's **own emitter** produces (`wide_bindings::control()`, under `test-support`).

The same scripts call both, and **every outcome and every error text is identical:**
- success and both error returns. Polars' error is a script `Err`; a `VmResult` error stays a VM error ("wide control: negative");
- a wrong type in each slot: "Expected type `::std::string::String` but found `::std::i64`", "Expected number type, but found `::std::string::String`", "Expected type `::polars::DataFrame` but found `::std::i64`". The tests assert typed and raw are equal, because rune 0.14.2 renders the typed error without its index layer;
- a wrong count: "Wrong number of arguments 3, expected 4";
- borrowed arguments readable afterwards (the receiver and the string), and an owned frame taken (reading it is "Cannot read, value is M-000000");
- a borrow conflict, the same frame as receiver and as owned argument: "Cannot take, value is --000001".

**Self-test `wide_bindings_self_test`** covers:
- the refusals: no citation, arity 4, a free function, arity 21, a wrong listed arity, a missing callable, a duplicate, an unused row;
- each slot conversion, and a slice type with none;
- the shim's exact order: count, take, convert, call, `ToReturn`, store.

## 3. Pivot itself (`probes/0127/pivot.rs`, run at v2 by the replay; 4 of 4)

**Equivalence** with Rust's `pivot(…, Arc::new(on_columns), …)` over the same frame:
- `maintain_order` true, compared exactly, and false, compared as a multiset of rows;
- `PivotColumnNaming::Auto` and `Combine`;
- two `values` with the separator `-`;
- a two-column `on`.

**Ownership:**
- after the call, the script's `on_columns` (height 3) and its receiver `lf` (a borrowed wrapper plus a Rust clone, 0109's rule for a `Clone` receiver) are readable;
- with `on_columns` reassigned afterwards, collecting the plan still gives Rust's result. The plan keeps its own snapshot.

**Polars' own refusals, as in Rust:** a width mismatch, mismatched names with a two-column `on`, empty `values`, duplicate `on_columns` values, and an empty `on` (round 1). Each is an error at `collect`, as in Rust, and none is a panic.

**The shim's count:** 8 and 10 arguments are refused with "Wrong number of arguments N, expected 9".

**Found in implementation.** `pivot`'s binding is infallible. No argument conversion can fail, so the binding returns a `LazyFrame`, not a `Result`. The probe's W6 steps (written in 0122, before the binding existed) used `?` after `.pivot(…)`, which is a Rune error on a non-`Result`. They now call `.pivot(…).collect()?`, in the before and the after runs alike. The Rust twins are unchanged.

## Review round 1 (Codex)

**Finding: test reliability.** Under Cargo's default parallel runner, the v2 pivot test failed 2 of 4 with `NoData("empty CSV")`. Every test's `fixture()` truncated and rewrote one per-process `sales.csv` while another test read it. The replay's `--test-threads=1` had hidden the race.
- **The fix:** each fixture call now writes its own file (a per-process counter), in `probes/0127/pivot.rs` and in `tests/wide_shim.rs`, which has the same pattern. The same fix goes into 0122's `tests/collect_dtypes.rs`, whose two tests both rewrote one `sales.csv`.
- **Checked:** the other temporary fixtures already use one name per test (`group_by.rs`, `loading.rs`).
- **The replay** now runs the pivot test under the default parallel runner.
- **Results:** at v2, pivot 4 of 4 and `wide_shim` 5 of 5 in each of three parallel runs; at 0.55.2, `wide_shim` 5 of 5 and `collect_dtypes` 2 of 2.

**The missing controls, added:**
- **`wide_shim`:** a wrong type in the **receiver** slot (index 0), reached through the qualified form `polars::DataFrame::wide_control_typed(5, "a", 2, other)` and its raw twin. Both report the same error: expected `DataFrame`, found `i64`.
- **The pivot refusals:** an empty `on`. Rust refuses it ("`pivot` called without `on` columns"), and so does the script, at `collect`.

## 4. Surface, freeze, oracle

**v2:**
- `LazyFrame::pivot` moves from `unsupported` to `generated`: generated 4,115 → 4,116, unsupported 1,896 → 1,895;
- no other entry moves.

**0.55.2:**
- no entry moves;
- the generated output gains only the `test-support` control shims.

**Freeze:**
- the frozen lists are rebuilt from 0126's surfaces (5,145 at 0.55.2, 6,781 at v2);
- both pins report "none moved, no contract changed".

**Oracle:**
- unchanged at 0.55.2: 3,997 cases, and only a standing unordered flip moved, which was restored;
- the v2 build is at its standing state (`cases_failed`, `oracle_controls` ok, `no_default` ok).

## Suites and launch

- **0.55.2 suites:** release default (33), release test-support (256) and debug generated plus test-support (257) all pass.
- **v2:** `tests/wide_shim.rs` 5 of 5, `tests/group_by.rs` 7 of 7, and the pivot test 4 of 4.
- **Launch against 0126 (`28dcf0f`),** default release builds at 0.55.2, three rounds of 60 interleaved launches (`launch-results-*.json`): median deltas of −0.69, +0.17 and +0.12 ms, so no change. The default build gains nothing; the control shims are `test-support` only.
