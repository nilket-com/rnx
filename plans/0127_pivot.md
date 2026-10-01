# rnx 0127: pivot, with an `Arc<DataFrame>` argument and higher-arity binding

Status: plan. Record 0126 is closed on `origin/main` at `28dcf0f`. The user chose pivot as one record covering both of its blockers. Raising the argument limit alone wouldn't complete W6.

**The measure (the user's):** W6 computes, matches Rust and displays. It is measured at v2, where the twins are.

**The user's condition:** this plan settles the `Arc<DataFrame>` ownership conversion and higher-arity binding together.

**Out of scope (the user's):** the byte-based `JsonReader` stays deferred; `read_json` already makes W1 usable.

## Where it stands

W6 ("reshape") pivots a left join and unpivots the result. Its pivot is Polars' (identical at both pins):

```rust
pub fn pivot(self, on: Selector, on_columns: Arc<DataFrame>, index: Selector, values: Selector,
             agg: Expr, maintain_order: bool, separator: PlSmallStr, column_naming: PivotColumnNaming) -> LazyFrame
```

(`polars-lazy frame/mod.rs:1770` at v2, `:1795` at 0.55.2.) Two blockers:
- **H:** `on_columns: Arc<DataFrame>` is a "foreign type". The generator has no rule for an `Arc` around a wrapped type.
- **I:** the receiver plus 8 arguments is 9. The generator refuses any method with more than 5 (`METHOD_ARITY`).

**Why 5.** Rune 0.14.2 implements its typed `Function` trait only up to arity 5. `function/macros.rs`'s `permute!` generates every by-value, `&` and `&mut` combination, 243 of them at arity 5, and stops there. Instance functions are implemented through `Function` (`function/mod.rs:122-140`). So a 9-argument method cannot be a typed Rune function at all.

## 1. `Arc<DataFrame>`: a shared, owned snapshot (H)

**What Polars wants.** `on_columns` is the frame of distinct values of the `on` columns. Polars only reads it: it validates it and turns each row into a literal filter (`dsl_to_ir/mod.rs:942-1010`). It takes shared ownership through `Arc`, so the plan keeps it after the call.

**The conversion.** The argument accepts the script's `DataFrame`. The binding passes `Arc::new(frame.clone())`:
- a `DataFrame` clone shares its column buffers and copies no data;
- the `Arc` owns that clone;
- the script keeps its frame, unchanged and usable (0125's borrowed-binding rule).

Polars never mutates through the `Arc`, so the plan and the script hold the same values with no aliasing hazard.

**Safety of arbitrary script frames.** Polars validates `on_columns` itself before use (`:942-951`):
- `on` is non-empty;
- `on.len() == on_columns.width()`;
- with several `on` columns, the names match;
- `values` is non-empty.

Every row index stays below `on_columns.height()`. So a wrong frame is Polars' own error, never a panic. Controls exercise each refusal.

**The rule: a closed, cited table (`[[arc_arguments]]`):**
- a row names a callable, its parameter, and the `Arc<T>` whose `T` is the wrapped source (`DataFrame`), with a citation;
- validated fail-closed: the parameter is `Arc<T>` by value at depth 0, `T` is wrapped and `Clone`, and the callable exists exactly once;
- an unused row is refused;
- only `LazyFrame::pivot`'s `on_columns` is listed. At v2 the only other `Arc<DataFrame>`-by-value parameter is `DslBuilder::pivot`'s, an internal plan builder, which stays refused.

## 2. Higher arity: a raw shim over the generated function (I)

**The route.** For a listed callable whose arity exceeds Rune's typed limit, the generator emits:
- **the generated Rust function exactly as today.** The signature, argument conversions, `vm_check`s, engine routing and return conversion are unchanged; a Rust function has no arity limit;
- **a raw shim registered in its place,** with `Module::raw_function(name, shim).build_associated::<Receiver>()` for a method (`module_raw_function_builder.rs`) or `.build()` for a free function.

**What the shim does: Rune's typed calling convention, step for step** (review of the plan; `function/mod.rs` `access_memory!` and `impl_function_traits!` `fn_call`, rune 0.14.2):
1. **The count first.** If the count is wrong: `VmError::bad_argument_count(actual, expected)`, as `access_memory!` returns.
2. **The slots taken, not cloned.** Through `slice_at_mut`, each slot is replaced with `Value::empty()`, as the typed path takes its arguments.
3. **Converted in order,** receiver first, at index 0:
   - `&W` → `borrow_ref::<W>()`;
   - `&mut W` → `borrow_mut::<W>()`;
   - `&str` → `borrow_string_ref()`;
   - `rune::Value` → the value;
   - anything else → `FromValue::from_value`.

   Each borrow guard lives through the generated call, as the typed path's guards do.
4. **The call.** The guards are dropped before the result is converted.
5. **The result converted with `ToReturn::to_return`,** not `ToValue`, so a `VmResult` return stays a VM error, then `out.store`.

**One difference that cannot be removed: the argument index.** The typed path attaches `VmErrorKind::BadArgument { arg: index }` to a conversion failure through `VmError::with_error`, which is `pub(crate)` in rune 0.14.2 (`runtime/vm_error.rs:299`). `VmErrorKind` is crate-private too. So the shim reports the conversion's own error, with the same kind and message, but without Rune's "Bad argument #n" layer. The control below records both texts side by side. This is reported, not hidden; an upstream request for a public way to tag an argument error is a follow-up.

**The typed metadata is not emitted** for a wide binding (review of the plan). `#[rune::function]` builds its `FunctionMetaKind` through the `Function` bounds, so for arity 9 it fails to compile even if it is never installed. The generated Rust function, its body and its argument conversions are unchanged; only the attribute is replaced by the shim's registration.

**Pivot's receiver.** `LazyFrame` is `Clone`, so pivot's generated receiver is the borrowed wrapper plus a cloned Rust receiver (0109). The script's `LazyFrame` stays usable after the call, and this is asserted.

**A typed-versus-raw control at a lower arity** (review of the plan: it proves the equivalence directly, rather than comparing against a typed arity-9 function that cannot exist). Under `test-support`:
- one control function (a `&DataFrame` receiver, a `&str`, an `i64`, and an owned wrapper argument) is registered twice: through `#[rune::function]`, and through a shim the generator's own emitter produces;
- a `VmResult`-returning twin is registered the same two ways.

The same scripts call both registrations and compare:
- a wrong type in each slot, receiver included: the error kinds and texts, with the index layer recorded as above;
- a wrong count;
- string and wrapper argument reuse after the call;
- an owned argument consumed;
- a borrow conflict: the same frame passed as receiver and as an owned argument;
- a `Result` error, and a `VmResult` error.

**The rule: a closed, cited table (`[[wide_bindings]]`):**
- a row names a callable and its arity, with a citation;
- validated fail-closed: the callable exists once, its arity exceeds 5, and every parameter's mapped Rust type has a shim conversion above, otherwise it is refused by name;
- an unused row is refused;
- **only `LazyFrame::pivot` is listed.** The 12 other callables refused only by arity at v2 (`Expr::qcut`, `datetime_range`, and others) are reported as candidates for a later record, not bound here.

**Settled in implementation:** `pivot`'s binding is infallible (no argument conversion can fail), so it returns a `LazyFrame`, not a `Result`. The probe's W6 steps, written in 0122 before the binding existed, drop the `?` after `.pivot(…)`, in the before and the after runs alike.

## The proof

- **W6 (the measure), `probes/0127`, from 0126's probe:** `w6.pivot` and the composed W6 compute, match their Rust twins and display both ways. W6 is a pivot of a left join, then an unpivot. Usable workflows at v2 go from W1–W5, W7, W8 to **all eight**. Before and after are replayable.
- **Equivalence (`tests/pivot.rs`, v2):**
  - the script's pivot equals Rust's `pivot(…, Arc::new(on_columns), …)` over the same frames, for `maintain_order` true and false and for both `PivotColumnNaming` values;
  - several `values` with a custom separator;
  - a multi-column `on`.
- **Ownership:** after the call, the script's `on_columns` frame and every other argument binding are still readable and unchanged; the receiver behaves as 0109's rule says for a by-value `LazyFrame`. The plan keeps its own `Arc` after the script frame is dropped or replaced.
- **Polars' own refusals, at the same point as in Rust:** a width mismatch, mismatched `on` names, empty `on`, empty `values`, and duplicate `on_columns` values (a duplicate output name).
- **The shim:**
  - the lower-arity typed-versus-raw control above;
  - for pivot itself, 8, 10 and 0 arguments are refused with `BadArgumentCount`;
  - a generator self-test covers the table's refusals (an arity of 5 or less, a free function, an unsupported parameter type, an unused row) and the emitted shim.
- **At 0.55.2:** the shipped build has no Polars `pivot` feature, so `pivot` isn't in its inventory. Adding it would be an inventory-configuration change like 0125's `json`, and is reported, not done here. Both new tables are empty at 0.55.2, and nothing there changes.

## Gates

- **Freeze:** every 0126 binding is unchanged at both pins; `LazyFrame::pivot` moves from `unsupported` to `generated` at v2. The frozen lists are rebuilt from 0126's surfaces.
- **Oracle:** no mismatches. A pivot oracle case is added only if its fixtures reach it; otherwise `tests/pivot.rs` carries it.
- **Suites and launch:** the usual suites, debug last. Launch is measured against 0126.

## Stop rules

- A parameter's conversion in the shim would differ from Rune's typed path for that type: refuse that callable by name.
- Polars would mutate through the `Arc`, or keep a borrow into the script's frame: stop. The conversion would need another model.
- Any Polars validation of `on_columns` turns out to be a panic, not an error: guard it, or refuse.
