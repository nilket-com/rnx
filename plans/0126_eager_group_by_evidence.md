# rnx 0126 evidence: the eager group-by, with Polars' borrowed `GroupBy` as a rebuilt borrow

**The measure is met.** At v2, `w5.eager_group_by` (`df.group_by(["region"])?.count()?`) and the composed W5 compute, match their Rust twins and display both ways. The usable workflows go from W1, W2, W3, W4, W7, W8 to **W1, W2, W3, W4, W5, W7, W8**, and every computed frame displays (40 of 40). At 0.55.2, W5 is usable too.

**The ownership contract (the user's condition), settled as planned.** A script `polars::GroupBy` owns everything Polars' `GroupBy<'a>` borrows or keeps privately:
- a snapshot of the frame;
- the full-length key columns;
- the groups, computed once;
- the selection.

Each call rebuilds a real `GroupBy` with Polars' public `GroupBy::new`, for that call only. The frame is owned by the snapshot, so it stays valid throughout every aggregation. No borrow outlives a call, and nothing is `unsafe`.

**Replay:** `probes/0126/replay.sh` builds the before state from 0125's tree (`3286e49`) and the after state at both pins, and byte-compares every table.

## 1. The model in the code

**`support::GroupBySnapshot`** (`adapters/polars/src/generated/support.rs`, hand-written) holds `df`, `keys`, `groups` and `selection`:
- **`view()`** is `p::GroupBy::new(&self.df, keys.clone(), groups.clone(), selection.clone())`. Each clone is shallow: `Column` shares its buffers, and `GroupPositions` shares an `Arc`.
- **`group_by` and `group_by_stable`** make Polars' own two calls, in Polars' order, over the snapshot: `select_to_vec(by)`, then `group_by_with_series(keys, true, sorted)` (`mod.rs:115-133` at 0.55.2, `:112-130` at v2). They keep the keys and `get_groups().clone()`.
- **`select`** records the selection exactly as Polars' body does (`:216` / `:213`).
- **`into_groups`** returns the groups.
- **`get_groups`** lends the **snapshot's** groups. They live for the whole call, and the binding clones them before it returns (review of the plan).
- **`Debug`** is Polars' own `Debug` of the view.

**In the generated code, a view is `let __arg0 = &this.0.view();`.** Rust extends that temporary to the end of the binding function, and `engine::run` joins before the function returns. So the rebuilt borrow lives exactly through the Polars call.

**The generator family `rebuilt_borrows`** (`tools/polars-gen/src/families/rebuilt_borrows.rs`, callable scope, on the 0115 registry):
- **Its release table,** closed and cited at each pin, names:
  - the owner;
  - its one borrowed field (`df`);
  - the snapshot type;
  - the Polars spelling;
  - each recipe (`DataFrame::group_by`, `group_by_stable`, `GroupBy::select`, `into_groups`, `get_groups`).
- **Validated fail-closed:**
  - the owner is a struct with a lifetime and no type parameter, whose **only** public field is the listed borrow;
  - each recipe's callable is present exactly once, and creates, consumes or lends the owner;
  - a recipe that is not generated is refused after generation.
- **Routing:**
  - a recipe calls the snapshot's function;
  - a `&self` method of the owner that neither takes, returns nor lends the owner gets the receiver `&this.0.view()` and calls Polars' real method (`<p::GroupBy>::count`);
  - every other callable that takes or returns the owner is refused by name.
- **The owner's wrapper** holds the snapshot, under the Rune name `polars::GroupBy`.

**Refused by name:**

| callable | why |
|---|---|
| `GroupBy::sliced` | release `[[refused]]`: it leaves per-group keys beside frame row indices, and every later aggregation (`prepare_agg`, then `keys`, then `keys_sliced(None)`) calls `take_slice_unchecked` with those indices, an unchecked out-of-bounds read in Polars itself (0.55.2 `mod.rs:840`, `:302`, `:306-307`, `:264`; v2 `:461`, `:287`, `:291-292`, `:249`) |
| `GroupBy::new` | "not a listed use": it takes a borrowed frame |
| `DataFrame::group_by_with_series` | "takes or returns GroupBy, and is not a listed recipe": script keys would need Polars' broadcast reproduced |
| 11 deprecated aggregations and `par_apply` (0.55.2) | Polars marks them `#[deprecated]` ("use polars.lazy aggregations") and removes them by v2. The family refuses its owner's deprecated methods (found in implementation; no other generated callable is deprecated) |
| `apply`, `apply_sliced` | the callback audit, unchanged |
| `GroupBy` `CLONE` | 0113's lifetime-head rule, unchanged. `select` clones the snapshot, so the script value stays usable |
| `get_groups_mut` (0.55.2) | the inventory classifier's `unsupported` bucket (a `&mut` borrowed return), unchanged |

**The surface: the same 10 rows move at both pins,** from `unsupported` to `generated`:
- `DataFrame::group_by` and `group_by_stable`;
- `GroupBy::count`, `groups`, `keys`, `keys_sliced`, `get_groups`, `into_groups` and `select` (as `select_`: `select` is a Rune keyword);
- `GroupBy` `DEBUG_FMT`.

| | 0125 | 0126 |
|---|---|---|
| 0.55.2 generated / unsupported | 2,884 / 1,448 | 2,894 / 1,438 |
| v2 generated / unsupported | 4,105 / 1,906 | 4,115 / 1,896 |

No other entry changed status, and none is new or gone.

**Freeze:**
- The frozen lists are rebuilt from 0125's surfaces (`probes/0126/frozen-*.json`: 5,135 at 0.55.2, 6,771 at v2).
- 0125's widenings are removed (2 at 0.55.2, 4 at v2), because the rebuilt lists already hold those contracts.
- Both pins report "none moved, no contract changed", with no widening listed.

**The oracle.** `DataFrame::group_by` and `group_by_stable` are `[[excluded_oracle]]` at both pins, with a cited reason: the oracle's Rust side cannot hold Polars' borrowed `GroupBy` as the snapshot wrapper. It had generated cases for them, calling the crate-private recipe, which the out-of-crate oracle test cannot reach (found by the suite). `tests/group_by.rs` carries them. No other `GroupBy` method had an oracle fixture. At 0.55.2 the oracle is unchanged: 3,997 cases, no status change, and the two standing unordered flips were restored.

## 2. Proof

**`tests/group_by.rs`: 7 of 7 at both pins,** each comparing the script with Rust's **real borrowed** `GroupBy` over the same frame:
- **every operation** on one stable `GroupBy`:
  - `count`, `groups`, `keys`, and `select(["qty"]).count()`;
  - `keys_sliced` at `(1, 1)`, `(-100, 999999)`, `(5, 2)` (past the end), `(i64::MAX, i64::MAX)` and `(-i64::MAX, 3)`. It matches Rust's clamped result exactly and never panics (`slice_offsets`, `utils/mod.rs:340-357` / `:353-370`);
  - `get_groups` and `into_groups`, compared by Polars' own `Debug` of the groups;
- **the hash-ordered `group_by`, multiple keys, null keys and an empty frame** (review of the plan): the null region is a group of its own, as in Rust. Stable results are compared exactly; hash-ordered ones as a multiset of rows;
- **one grouping:** over 20 hash-ordered `group_by`s of two keys, `count()`, `keys()` and a second `count()` from one `GroupBy` are row-aligned every time, as in Rust;
- **the frame stays valid whatever the script does to its binding** (review of the plan):
  - after `gb = df.group_by_stable(by)`, the script mutates `df` in place (`drop_in_place("qty")`, `rename("region", "r")`), then replaces `df`, then aggregates from a `GroupBy` whose frame was a temporary in an inner block;
  - every `count()` equals Rust's on the original frame;
  - the key list `by` is reused afterwards (0125's borrowed-binding rule);
- **an invalid selection** (review of the plan): `select_(["nope"]).count()` is Polars' error, as Rust's is, and the same `GroupBy` and the selection list stay usable;
- **`Debug`** equals Rust's `format!("{:?}", gb)`;
- **the refusals:** `sliced`, `new` and `group_by_with_series` are absent from the catalogue, and `count` and `group_by` are present.

**Investigated in implementation, and not a defect.** A smoke run seemed to show two `count()` calls in different orders. It was two separate process runs, whose hash orders legitimately differ. Within one run, `count`, `keys` and a second `count` always agree (20 of 20 above). Polars' own `GroupBy` agrees too (200 of 200, probed directly).

**The generator:** 53 of 53. Self-test `rebuilt_borrows_self_test` covers:
- the table's refusals: no citation, an owner without a lifetime, a second public field, an owned field, a missing recipe callable, a recipe that neither creates, consumes nor lends, a duplicate;
- routing: a recipe, a view, a deprecated method, `sliced`, `new`, an unrelated callable taking the owner, and a callable that never mentions it;
- the receiver and callee each route produces;
- the unused-recipe refusal.

0115's `hook_orders_are_pinned` names the family.

## 3. The probe (`probes/0126`, from 0125's)

G is fixed; the steps, twins and data are unchanged.

| pin, state | steps (works / blocked, + presented) | workflows compute | frames displayed both ways | usable workflows |
|---|---|---|---|---|
| v2, before (0125) | 36 / 3, +1 | 6 of 8 | 38 of 38 | W1, W2, W3, W4, W7, W8 |
| v2, after | **37 / 2, +1** | **7 of 8** | **40 of 40** | **W1, W2, W3, W4, W5, W7, W8** |
| 0.55.2, before | 27 / 12, +1 | 2 of 8 | 26 of 26 | W2, W3 |
| 0.55.2, after | 28 / 11, +1 | 3 of 8 | 28 of 28 | W2, W3, W5 |

At 0.55.2 there are no Rust twins, so "works" means the step ran.

**The remaining v2 ranking:** C2 (`w1.json_bytes`, the `JsonReader` lifetime model), then H and I (pivot, W6).

## Suites and launch

- **Production suites (0.55.2):**
  - release default (33), release test-support (251) and debug generated plus test-support (252) all pass;
  - the first test-support run failed to compile `generated_oracle.rs` on the recipe cases, fixed by the oracle exclusion above.
- **v2 build (`probes/0108/build.sh`):** every stage ok; `oracle: cases_failed` is the standing state; `no_default` ok. The first build's oracle controls failed on the same recipe cases, and passed after the exclusion. `tests/group_by.rs` and `tests/loading.rs` pass 7 of 7 each at v2.
- **Launch against 0125 (`3286e49`),** default release builds, three rounds of 60 interleaved launches (`launch-results-*.json`):
  - median deltas of −0.15, −0.22 and +0.08 ms, so no change;
  - binary 175.33 → 175.19 MB (−0.14 MB). This is not attributed: the old binary was built from a worktree (`target/0126/launch-old-src/…`), whose longer source paths are embedded in its panic locations, so a difference this size is within build-path noise.

**The probe's display text for unordered steps (found by the replay).** Now that `w5.eager_group_by` displays, its display text showed the hash-ordered rows in a per-run order, so a second replay differed from the first. For an `UNORDERED` step, the probe now sorts the data rows before cutting the text to 200 characters. The title, any omission line and the column header keep their places. This is the same normalisation the computation comparison already used. The committed tables are written with it, and a second, non-init replay reproduces them byte for byte.
