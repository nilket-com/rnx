# rnx 0141: `clone()` for the Polars value types, and a measured consumption contract

Status: plan, the second of the order agreed with Codex after 0139 (0140, then 0141, then 0142).

**The finding (0139):** U4 hit "no method `clone` on `::polars::DataFrame`" (named since 0140). It then rebuilt its frame for every view, on the assumption that `lazy` and `sort` consume the frame.

**That assumption was wrong, measured before planning:**
- **In a session,** `df.lazy().collect()` twice, `df.sort(…)` twice and `df.head(…)` all leave `df` usable, and it still displays.
- **The generated bindings** (`adapters/polars/src/generated/functions.rs`), counting instance receivers:
  - **`DataFrame`:** 83 by reference, 22 by mutable reference, **0 by value**;
  - **`LazyFrame`:** 75, 1 and **0**;
  - **`Series`:** 163, 8 and **0**;
  - **`Expr`:** 118, 0 and **0**;
  - **all types:** 3,394 by reference, 468 by mutable reference, 131 by value. The by-value receivers are builders (`JoinBuilder`, `DslBuilder`), readers and writers, and expression namespaces (`list()`, `str()`, `name()`, `struct_()`, `binary()`).

**So the real gap isn't consumption. It's two things:**
1. **No way to make an independent copy.** Rune variables share one value. `let b = df;` is the same frame, and the 22 `DataFrame` methods (8 on `Series`) that mutate in place change it through every name. A script that wants to mutate a copy can't.
2. **No stated contract,** so a script author (and 0139's author) guesses.

## 1. The change

- **`clone()` for `DataFrame`, `LazyFrame`, `Series` and `Expr`:** Polars' own `Clone`. That's a shallow copy, its columns `Arc`-shared and copy-on-write when one side is mutated. It returns an independent value: mutating either side never changes the other.
  - **Registration:** as ordinary instance functions in the adapter's hand-written layer, not the generator (it's one binding per type, and the generator's freeze stays untouched).
  - **The catalogue:** gains each entry, so 0140's naming and the catalogue's search find them.
- **The contract, stated in the adapter's module documentation and the catalogue text:**
  - **These four types never consume:** every method borrows, so a value can be used for any number of views.
  - **The 22 + 1 + 8 in-place methods** mutate the value, and every name bound to it sees the change; `clone()` first gives an independent copy.
  - **Builders, readers, writers and namespaces** are consumed by their chaining methods, and the catalogue marks each one.
- **The counts above become a test,** recomputed from the generated source, so a regenerated binding set that changes them fails it.

## 2. Measures

- **The shallow clone against copying:** the time and allocation of `clone()` on frames of 1, 10⁵ and 10⁶ rows by 10 columns. Then, separately, the first in-place mutation of the clone (copy-on-write materializes only what's touched), and a deep copy for comparison.
  - **The expected shape:** clone is O(columns), not O(rows). This is measured, not claimed.
- **Many views from one frame:** a session pastes one frame and derives five views from it (a lazy group-by, two sorts, a head and a filter). It's shown in the evidence's transcript, with the original unchanged after each.

## 3. Controls

- **Independence, per type:**
  - an in-place mutation of a clone leaves the original unchanged, its values compared;
  - a mutation through an alias (`let b = df;`) changes both, and the contract says so;
  - `Series` and `LazyFrame` likewise.
- **Never consumed:** for each of the four types, a representative chain of by-reference methods runs twice on the same value, and the value is unchanged.
- **Consumed builders:** a `JoinBuilder` used twice fails the second time with a named message (Rune's moved-value error). It's shown, not changed.
- **The bindings test** recomputes the counts from `generated/functions.rs`.

**0139's U4 comment, corrected:** its `table()` helper says "a DataFrame's lazy and sorting methods take it by value in Rune, and it has no clone", and the first half is false. The comment is corrected (the script's behaviour is unchanged), with a pointer to this record. U4 is replayed bit-equal.

## Gates

- **Suites:** Polars (default and `generated`), core and Candle, plus clippy.
- **U4's replay:** bit-equal.
- **Launch:** within noise, for four more registrations.
- **`:dep polars candle`** after push.

## Out of scope

- Changing any binding's receiver kind.
- A deep-copy API.
- Copy-on-write semantics for mutation through aliases (Rune's sharing is the language's).
