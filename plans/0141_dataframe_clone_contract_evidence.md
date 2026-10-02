# rnx 0141 evidence: `clone()` for the Polars value types, and the consumption contract

**The result:**
- `df.clone()`, `lf.clone()`, `series.clone()` and `expr.clone()` exist. Each returns a **distinct Rune value** over Polars' own `Clone`: an independent wrapper whose data is shared copy-on-write. **This isn't deep independence:** the buffers are shared until one side writes.
- **The contract** is stated in the adapter's module documentation and the catalogue:
  - the four types are never consumed;
  - names alias one value, so an in-place method changes every name;
  - the 131 by-value methods (builders, readers, writers, namespaces) are each marked "consumes the receiver".

**A correction to the plan's premise:** an independent copy wasn't strictly impossible before. Record 0111 registered Polars' `Clone` as Rune's `CLONE` protocol for all four types, and that's reachable as `std::clone::clone(df)`. What was missing was the **method** form, `df.clone()`, which is what 0139 (and anyone coming from Rust or Python) typed. A test now shows that the two agree (`the_clone_protocol_and_the_method_agree`).

## 1. The change

**`adapters/polars/src/lib.rs`, the hand-written layer:**
- **The bindings:** `clone_frame`, `clone_plan` and `clone_expr`, each `#[rune::function(instance, path = clone)]`. `clone_series` is `cfg(feature = "generated")`, because `Series` is a generated type.
- **Registration:** they're registered before the generated install, and the generator binds no `clone` method (the test checks this).
- **The catalogue:** gains three entries, plus `polars::Series::clone` with `generated`.
- **The marking:** a generated catalogue entry whose path is in the new `CONSUMING` table takes that table's marked text, ending " (consumes the receiver: a later use of it is an access error)". The lookup is a binary search over the sorted, generated `'static` table: nothing is formatted at launch (see Launch for why).
- **The contract,** in the module's documentation.

**`tools/polars-gen`, the generator:**
- `Emitted` gains `consuming`. `emit_callable` records each binding whose receiver signature is by value (`this: W`, a non-Clone owner moved out of the Rune value, record 0109's rule). The generic-traits family carries it from its scratch emission.
- `catalogue.rs` gains `pub const CONSUMING: &[(&str, &str)]`: each path, sorted, with its marked catalogue text.
- **No summary changes,** so the freeze is untouched.
- **Regenerating** (README command, 0.55.2-joins release) changes only `generated/catalogue.rs`, by appending the 131-path list. A regeneration before the generator change was byte-identical to the committed tree.

**U4's comment** (`probes/0139/workflow-u4.rn`): it said "a DataFrame's lazy and sorting methods take it by value in Rune, and it has no clone". That's corrected, with a pointer to this record, and the script is unchanged.

## 2. Controls

**The census** (`tests/clone_contract.rs`, configuration: default features plus `test-support`):
- **What it counts:** method bindings only (`#[rune::function(instance, path = …)]`). Protocol handlers (`CLONE`, `DEBUG_FMT`, …) and free functions are excluded, and the four hand-written clones aren't generated bindings.
- **Per type** (by reference / by mutable reference / by value): `DataFrame` 83 / 22 / **0**, `LazyFrame` 75 / 1 / **0**, `Series` 163 / 8 / **0**, `Expr` 118 / 0 / **0**.
- **All types:** 3,394 / 468 / 131.
- **`CONSUMING`** has 131 paths, sorted, the same methods as the by-value bindings; each marked text is its catalogue summary plus the marker.
- **The marking:** every one of them has a catalogue entry and is marked in `build()`'s catalogue, and **nothing else is marked**.
- **`SeriesBuilder::freeze_reset`** is `&mut`, so it's mutated, never consumed, and stays unmarked.

**Distinct values** (`clone_identity`, a unit test that needs the private wrappers):
- **A clone:** for each type, a clone and its original can be exclusively borrowed at the same time. So they're distinct Rune values.
- **An alias:** for `let b = a;`, the second exclusive borrow fails, so it's the same value.
- **Where it runs:** with `generated` (four types) and **with `--no-default-features`** (`DataFrame`, `LazyFrame`, `Expr`). The catalogue test checks the clone entries in both configurations.

**Data-changing mutations, alias versus clone:**
- **`DataFrame::sort_in_place`, descending by `x`:**
  - sorting the clone leaves the original's values exactly as before;
  - sorting the original leaves an earlier clone alone;
  - sorting an alias changes both names, to the same value as the clone's sort.
- **`Series::append`:**
  - an append to the clone leaves the original at 3 rows, with unchanged values;
  - an append through an alias is seen by both names.

**`LazyFrame` and `Expr`,** with no invented mutations:
- one plan and one expression serve the same two-column view twice, through a clone, and again afterwards, all four results equal;
- the plan's collected value is unchanged;
- `collect_schema`, `LazyFrame`'s one `&mut` method, run on the clone, gives the original's schema.

**A consumed builder:**
- `fx::lf().join_builder()` then `j.with(…)` twice fails the second time with Rune's own access error, `Cannot take, value is M-000000`. It's shown, not changed.
- The catalogue's marker says "access error", the same wording record 0109 used.

## 3. Measures

**Clone against copying** (`clone_cost`, ignored unit test, release; 10 `i64` columns; median of 15 runs, 7 at 10⁶ rows; bytes are live bytes left while the step's result is held; `probes/0141/out/clone-cost.txt`):

| rows | `clone()` | bytes | buffers shared | `rename` (metadata) | bytes | `sort_in_place` (data) | bytes | original intact | deep copy | bytes |
|---:|---:|---:|---|---:|---:|---:|---:|---|---:|---:|
| 1 | 2.0 µs | 1,672 | true | 102.8 µs | 272 | 186 µs | 768 | true | 11 µs | 4,880 |
| 100,000 | 2.4 µs | 1,672 | true | 74.8 µs | 272 | 1,253 µs | 8,045,584 | true | 2,652 µs | 8,004,800 |
| 1,000,000 | 6.0 µs | 1,672 | true | 306.2 µs | 272 | 10,246 µs | 80,045,584 | true | 32,179 µs | 80,004,800 |

- **`clone()` is O(columns):**
  - the same 1,672 bytes and a few µs at every size;
  - every clone's first column shares the original's buffer (pointer equal).
- **The metadata mutation** (`rename("c0", "r0")` on the clone) leaves 272 bytes, and no buffer is copied. Its time is the engine thread's dispatch (`rename` is routed), not data work.
- **The data-changing mutation** (`sort_in_place` by `r0`, descending, on the clone):
  - it allocates a full new frame (≈ 8 bytes × rows × 10), because Polars' sort gathers every column into new buffers;
  - the original's buffers and values are intact in every run.
  - **Copy-on-write saves the copy at clone time, not this rewrite.** An in-place method that rewrites every column pays for it either way.
- **The deep-copy baseline** (Rust, each column's slice copied into a new `Series`) costs what the data costs: 8 MB / 80 MB at 10⁵ / 10⁶ rows.

**Many views from one frame** (`probes/0141/views.py`, runner0134 session; `out/views.txt` is the clean transcript):
- **The five views:** a lazy group-by (sum of hours per team), a descending sort, a sort by team, `head(Some(2))` and a lazy filter. Each displays its result, then `df` is shown.
- **The frame is shown 7 times, and is identical every time.**
- **`let copy = df.clone(); copy.sort_in_place(…)?; copy`:** shows the sorted copy, and `df` is unchanged.
- **`let same = df; same.sort_in_place(…)?; df`:** shows `df` sorted, the clone's value: an alias is the same frame.
- **Errors:** none.

## Gates

- **0139's U4,** replayed on this tree as a session paste with only its comment changed: it passes, and is **BIT-EQUAL** to its twin with the table validated (`out/u4-replay.txt`).
- **Suites:**

| suite | passed |
|---|---:|
| core, with `server-runtime` | 408 |
| Polars, default | 43 |
| Polars, `test-support` | 273 |
| Polars, `--no-default-features` | 22, plus **2 pre-existing failures** |
| Candle, `test-support` | 61 |
| project tool | 102 |
| polars-gen | 55 |

  - **The two failures** are in `tests/dense.rs`, which calls the generated `select_` but isn't gated on `generated`, so it fails without it ("Missing instance function … for `::polars::DataFrame`"). They fail identically on the base tree (stashed); 0142's maintenance can gate them.
  - **`oracle-results.json`** is rewritten by a release `test-support` run (Polars' `debug_assert`s don't fire), so it's restored to the committed copy.
  - **Clippy** reports nothing in the code this record touches. One `type_complexity` warning it raised in `generic_traits.rs` was fixed with a type alias.
  - **The generator:** regenerating changes only `generated/catalogue.rs`, by appending the `CONSUMING` table.

- **Launch** (`probes/0141/launch.py`, 0129's method: 60 interleaved launches of an empty script per round, `rnx-polars` at `13c7328` against this tree):
  - **The first build cost +2.31, +2.01 and +1.82 ms,** which is real. The cause, measured in-process: the first version formatted the 131 marked texts and built a map over the ~4,900 catalogue entries in `build()`, which runs at every launch. That took **2.1 ms**.
  - **The fix:** the generator now emits the marked texts (the `CONSUMING` table), and the adapter only binary-searches it.
  - **Six rounds after the fix:** +0.75, −0.15, +1.19, −0.16, −0.11 and −0.04 ms, a median of about −0.1 ms. **Within noise** (`out/launch-results-1…6.json`, all after the fix).
- **`:dep polars`, after push** (`out/dep/views.txt`):
  - **The setup:** a clean worktree binary at the pushed `bdad901` and a fresh `RNX_PROJECT_CACHE`; `views.py` ran under `RNX_DEP=1`, so the session ran `:dep polars` first and used the adapter fetched at that commit.
  - **The result:** the frame was shown 7 times and was identical every time. The clone's in-place sort left it unchanged, and the alias's sort changed it to the clone's sorted value. **No errors.**
