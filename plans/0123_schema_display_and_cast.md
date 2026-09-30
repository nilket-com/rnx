# rnx 0123: schema display, `cast` with a `DataType`, and a display check in the probe

Status: plan. Record 0122 is closed on `origin/main` at `e16b445`. The user chose this pair from 0122's ranking. Each has a clear user-facing result, and together they are small enough to review as one record.

**The success measure changes** (the user's correction). 0122's "3 of 8 workflows run end to end" established the computation: each result matched Rust through the test-support `oracle_repr`. It did not establish a usable notebook result, because `preview` still refuses Date and count columns that those workflows now produce. This record adds a real display check to the probe and ranks the display gap from that evidence.

## 1. Schema display (family E, completes W2)

**The gap.** `df.schema()` returns `polars::SchemaRef` (the alias `Arc<Schema<DataType, ()>>`). It has no `DISPLAY_FMT` and no `DEBUG_FMT`, so `${df.schema()}` and `println!("{}", df.schema())` fail. 0113 refused the `Debug` impl because it sits on the generic head `Schema<Field, Metadata>` ("value protocol on a generic or lifetime-bearing head").

**What Polars offers.** `Schema` derives `Debug` and has no `Display` impl at the pin (`polars-schema schema.rs:10-13`, da47b74). So the only text Polars itself gives a schema is its derived `Debug`.

**The rule.**
- **`DEBUG_FMT`:** the wrapped `SchemaRef` instantiation gets its `Debug` protocol, proven for that instantiation by the applicability proof (`DataType: Debug`, `(): Debug`), as 0113 does for other instantiated protocols.
- **`DISPLAY_FMT`:** the same Polars `Debug` text, for this one listed type, because Rust has none of its own. The display text is Polars', not invented. The rule is a closed, cited row, and any other type is refused.

**Proof.** A script prints `${df.schema()}` and `{:?}` of it, and both equal Rust's `format!("{:?}", df.schema())`. W2's step and its composed workflow flip to works.

## 2. `cast` with a `DataType` (family F, completes W3)

**The gap.** `Expr::cast(dtype: impl Into<DataTypeExpr>)`. The generator maps an `impl Into<T>` parameter to `T` alone, so the natural `col("qty").cast(polars::DataType::Float64())` is refused ("Expected DataTypeExpr but found DataType").

**What Polars offers.** A recorded `From<DataType> for DataTypeExpr`, already generated as a conversion binding.

**The rule: `impl Into<T>` accepts its recorded `From` sources.** A parameter of type `impl Into<T>` accepts a `T`, or a value of any source `S` for which the inventory records `From<S> for T` and `S` is wrapped. The value is converted with Polars' own `<T as From<S>>::from`. Anything else is refused before any Polars call, never guessed and never a process panic. The refusal is a **VM argument-type error** (Rune's `VmError`, reported like today's "Expected type `DataTypeExpr` but found …"), naming the operation, the argument, both accepted types and the type found. It is not a script-catchable `ConversionError`: that would make all 12 bindings fallible, and every `cast` would need `?`, the fallibility change the user said must not be made casually. So `cast` keeps returning `Expr`, as it does today (review of the implementation: this boundary is the contract, and it is asserted).

The admitted targets are a closed, cited release table (`[[into_arguments]]`: target, sources, citation), validated fail-closed against the recorded `From` impls. This record lists one target: `DataTypeExpr` with source `DataType`. That covers exactly 12 parameters:
- `Expr::cast`, `strict_cast`, `cast_with_options`;
- `LazyFrame::cast_all`;
- `BinaryNameSpace::reinterpret`, `StringNameSpace::json_decode`, `StringNameSpace::strptime`;
- `CategoricalNameSpace::to`, `ExtensionNameSpace::to`;
- the free `cast`, `int_range` and `int_ranges`.

**The freeze.** These 12 bindings keep their paths and ids. Their catalogue contract widens from `DataTypeExpr` to `DataTypeExpr or DataType`. The freeze refuses any contract change, so each widening is listed in a closed exception table in the release: the exact old summary, the exact new summary and the citation. Generation refuses any other change, or an unlisted one.

**Proof.**
- **A valid dtype:** `cast(DataType::Float64())` equals Rust's `cast(DataType::Float64)`, and so does the existing `DataTypeExpr` form. W3's step and composed workflow flip to works.
- **Refused conversions:**
  - a non-dtype argument, for example a string, is a VM argument-type error naming the accepted types, before any Polars call; the call does not return a `Result`, and the binding stays infallible;
  - a data conversion Polars itself refuses (`strict_cast` of an unparseable string to Int64) surfaces as Polars' error, and is not swallowed.
- **The oracle:** the 12 bindings are exercised with both argument kinds where fixtures allow.

## 3. A display check in the probe

For every composed workflow's final result, and for each step whose result is a frame, the probe also records whether a user can see it. The two real routes are run and recorded separately:
- **`preview`,** the adapter's bounded text;
- **presentation,** printing the value (`println!("{}", df)`, the hand-written `DISPLAY_FMT`) and the session presenters (0068) for a bare frame.

Each outcome is `displayed` (the text contains the result's values) or `refused` (with the error). A refusal is attributed to its cause, for example the dtype that `preview` does not show.

**Reporting.** Computation and display are reported apart. A workflow counts as usable end to end only if it computes (matching its twin) and its final result displays.

**Ranking.** The display gaps are ranked by the same rule as 0122 (marginal steps and workflows made usable, then risk). The top display family is the candidate for 0124; it is not fixed here.

**Out of scope:** the pervasive `?` friction. It gets its own design pass, because changing fallibility casually could hide errors (the user).

## Gates

- **Freeze:** every 0122 binding is unchanged, except the 12 listed widenings, each exact. The frozen lists are rebuilt from 0122's surface.
- **Probe:** W2's and W3's blocked steps and composed workflows flip to works, and match their twins. The before and after tables are replayable, as in 0122. The display table is committed at both pins.
- **Oracle:** no mismatches; old movements only within the approved policies.
- **Suites and launch:** the usual suites, debug last; launch measured against 0122.

## Stop rules

- A binding outside the 12 widens, or any other contract changes: stop.
- A `From` source that is not recorded, or that needs an ownership rule this record does not have: refuse it by name.
- Displaying the schema would need text Polars does not produce: stop and report, rather than invent a format.
