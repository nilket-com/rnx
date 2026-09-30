# rnx 0126: the eager group-by, with an explicit ownership model for Polars' borrowed `GroupBy`

Status: plan. Record 0125 is closed on `origin/main` at `3286e49`. The user chose the eager group-by, family G, the top of 0125's computation ranking.

**The measure (the user's):** W5 computes, matches Rust, and displays its result. It is measured at v2, where the Rust twins are.

**The user's condition:** settle the borrowed `GroupBy`'s ownership contract first. Its frame must remain valid throughout aggregation. If that needs a new lifetime model, it is an explicit part of the plan. **It does, and section 1 is that model.**

## Where it stands

`df.group_by(["region"])?.count()?` fails: "Missing instance function … for `::polars::DataFrame`". Every `GroupBy` row is refused:
- **0.55.2:** 17 rows as "lifetime owner", and `apply`, `par_apply` and `apply_sliced` by the callback audit;
- **v2:** 9 as "lifetime owner", and 2 by the callback audit.

The reason is Polars' type (`polars-core frame/group_by/mod.rs`, both pins):

```rust
pub struct GroupBy<'a> {
    pub df: &'a DataFrame,
    pub(crate) selected_keys: Vec<Column>,
    groups: GroupPositions,
    pub(crate) selected_agg: Option<Vec<PlSmallStr>>,
}
```

A script value can't hold `&'a DataFrame`: nothing ties the script's frame to the borrow, and the script could drop or replace it while the `GroupBy` lives.

## 1. The ownership contract: a rebuilt borrow (a new model, `[[rebuilt_borrows]]`)

**The contract.** A script `polars::GroupBy` **owns** everything Polars' `GroupBy<'a>` borrows or keeps privately:

| part | what it is | how it is captured |
|---|---|---|
| `frame` | an owned `DataFrame` snapshot | the receiver's frame, cloned (a `DataFrame` clone shares its column buffers; it copies no data) |
| `keys` | the selected key columns, full length | the same public call Polars makes |
| `groups` | `GroupPositions`, computed **once** | Polars' own grouping, over the snapshot |
| `selection` | the `select`ed aggregation columns | recorded as `select` sets them |

**Each operation rebuilds a real `GroupBy` for the length of the call and drops it before returning:**

```rust
fn view(&self) -> GroupBy<'_> {
    GroupBy::new(&self.frame, self.keys.clone(), self.groups.clone(), self.selection.clone())
}
```

`GroupBy::new` is public at both pins (`mod.rs:196` at 0.55.2, `:193` at v2). Every clone is shallow: `Column` shares its buffers, and `GroupPositions` shares an `Arc` of its groups.

**Why the frame stays valid throughout aggregation:**
- The borrow `&self.frame` exists only inside one call, while the snapshot that owns the frame is itself borrowed. No borrow outlives a call.
- Every `&self` method returns owned values (`DataFrame`, `Vec<Column>`, `GroupPositions`), so no reference escapes.
- There is no `unsafe`, and no lifetime is extended.

**One grouping, as in Rust.** The groups are computed once, when the `GroupBy` is created, and every later call reuses them. So `keys()` and `count()` from one `GroupBy` stay row-aligned, as they are in Rust, even for the non-stable (hash-ordered) `group_by`.

**Snapshot semantics.** In Rust the borrow checker forbids changing the frame while a `GroupBy` exists. Here the `GroupBy` holds its own snapshot, so replacing or dropping the script's frame afterwards doesn't affect it. That is what any Rust program that compiles would observe.

**Exactness: every piece of private state comes from Polars' own public calls, in Polars' own order.** Nothing re-implements its internals.
- **`DataFrame::group_by(by)` and `group_by_stable(by)`:** Polars' bodies are `let selected_keys = self.select_to_vec(by)?; self.group_by_with_series(selected_keys, true, sorted)` (`frame/group_by/mod.rs:115-133`, `:112-130` at v2). The snapshot runs those same two calls on the frame snapshot. It keeps `select_to_vec`'s columns as `keys` and `get_groups().clone()` as `groups`.
  - `group_by_with_series` broadcasts the keys to the frame height (`:43-53`), which is the identity for columns selected from that frame.
- **`GroupBy::select(self, selection)`:** Polars' body is `self.selected_agg = Some(selection.into_iter().map(|s| s.into()).collect())` (`:216` / `:213`). The snapshot records the same value.
- **`GroupBy::sliced(self, slice)`: refused by name.** It is reproducible exactly: its body is `groups = groups.slice(offset, length)`, then `selected_keys = self.keys_sliced(slice)` (`:840` / `:461`). But the state it leaves is unsound in Polars itself:
  - the keys become one row per sliced group, while `groups` still holds **frame row indices**;
  - every later aggregation runs `prepare_agg`, then `keys()`, then `keys_sliced(None)` (`:302-307`), which calls `take_slice_unchecked(groups.first())` on those short keys;
  - so a row index past the group count reads out of bounds, unchecked.

  A script could reach that with `sliced(...)` and then `count()`, so `sliced` stays unsupported, with this reason. (Found while planning; Polars uses `sliced` internally only on paths that don't aggregate afterwards.)
- **`keys_sliced(slice)` on the snapshot's unsliced state is sound.** The keys are full length, and `GroupPositions::slice` clamps any offset and length into range through `slice_offsets` (`utils/mod.rs:340-357` at 0.55.2, `:353-370` at v2), whether negative, overflowing or past the end. The bounds controls exercise exactly that.
- **`GroupBy::into_groups(self)`:** the snapshot's `groups`.

**Out of scope, refused by name:**
- `group_by_with_series` with script-supplied key columns. Its broadcast of short keys would have to be reproduced, and W5 doesn't need it.
- `GroupBy::new`, which takes a borrowed frame.
- `apply`, `par_apply` and `apply_sliced` stay refused by the callback audit, unchanged.

**The rule in the generator.** A new family, `rebuilt_borrows`, with a closed, cited release table:
- the owner (`polars_core::frame::group_by::GroupBy`);
- its snapshot type (`support::GroupBySnapshot`, hand-written, holding the four parts);
- each constructor and consuming method, with its capture recipe (`group_by`, `group_by_stable`, `select`, `into_groups`), and `sliced` refused with its reason.

The rest is ordinary generation:
- Every other `&self` method of the owner is generated with the receiver `&this.0.view()` instead of `&this.0`.
- A method returning the owner by value (`select`) returns a new snapshot.

Validated fail-closed:
- the owner's only lifetime is the one borrowed field, and the release names that field;
- every constructor and consuming method is listed, and an unlisted way of creating the owner is refused;
- a listed row unused is refused.

**Settled in implementation:**
- **Deprecated methods are refused.** At 0.55.2, Polars marks 11 eager aggregations `#[deprecated]` ("use polars.lazy aggregations"): `first`, `last`, `max`, `mean`, `median`, `min`, `n_unique`, `quantile`, `std`, `sum` and `var`, plus `par_apply`. v2 removes them. The family refuses a deprecated method of its owner by name. This is scoped to the family: no generated callable elsewhere is deprecated, and no general rule changes.
- **The snapshot's frame field is named `df`,** Polars' one public field, so the generator's field getter (`this.0.df.clone()`) reads it.
- **`get_groups` is a recipe.** It lends the snapshot's own groups, which live for the call, never a temporary view's, and the binding clones them before returning (review of the plan).
- **`CLONE` stays refused** by 0113's rule for a lifetime-bearing head. A `self` method (`select`) clones the snapshot, so the script value stays usable.
- **`DEBUG_FMT`** is Polars' own `Debug` of the rebuilt view.

**The surface this reaches:**
- **both pins, the same 10 rows:** `DataFrame::group_by` and `group_by_stable`, and `GroupBy`'s `count`, `groups`, `keys`, `keys_sliced`, `get_groups` (a shallow clone), `into_groups`, `select` and `DEBUG_FMT`. At 0.55.2 the deprecated aggregations are refused, as above.
- The exact set is established by generation. Rows move from `unsupported` to `generated`, and nothing already bound changes.

## Proof

- **Equivalence (both pins).** For every generated method, the script's result equals Rust's direct call on a real borrowed `GroupBy` with the same groups:
  - `group_by_stable` results compared exactly;
  - `group_by` results compared as unordered, the probe's rule;
  - after `select`.
- **One grouping.** Two aggregations and `keys()` from one script `GroupBy` over a non-stable `group_by` are row-aligned, as in Rust.
- **The frame outlives nothing it must not.** After `let gb = df.group_by([...])?`, the script drops or reassigns `df`, builds and drops other frames, and then aggregates: the result equals Rust's on the original frame. Receiver and argument bindings are reused after every call (0125's borrowed-binding control).
- **Bounds.** `keys_sliced` with out-of-range, negative and overflowing offsets and lengths matches Rust's clamped result and never panics (`slice_offsets`, cited above). `sliced` is refused by name.
- **The generator:** a self-test covers the table's refusals (an unlisted constructor, an unused row, an owner with a second lifetime-bearing field) and the view receiver.
- **The probe (`probes/0126`, from 0125's):** `w5.eager_group_by` and the composed W5 compute, match their twins and display. Usable workflows at v2 go from W1, W2, W3, W4, W7, W8 to **W1 through W5, W7 and W8**. Before and after are replayable at both pins.

## Gates

- **Freeze:** every 0125 binding is unchanged. The frozen lists are rebuilt from 0125's surface.
- **Oracle:** no mismatches. New `GroupBy` cases get fixtures over a small frame, if the oracle's fixture rules reach the owner; otherwise the equivalence test above carries them.
- **Suites and launch:** the usual suites, debug last. Launch is measured against 0125.

## Stop rules

- A captured part cannot be obtained through Polars' public calls exactly as Polars obtains it: refuse that constructor or method by name.
- Any rebuilt `GroupBy` would need to outlive a call, or any `unsafe` would be needed: stop. That would be a different model, and a new plan.
- A `GroupBy` method panics on script input that Rust would reject with an error: guard it (as 0120's receiver guards do), or refuse it.
