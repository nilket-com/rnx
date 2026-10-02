# rnx 0140: missing methods named on every type, with a hint for an unwrapped Result

Status: plan, agreed in order with Codex after 0139 (0140, then 0141, then 0142).

**The finding (0133 F15):** a missing method in a session reads `Missing instance function 0x… for ::polars::DataFrame`, a hash, not the method's name. It cost 0139 three diagnosis rounds:
- **`DataFrame.clone()`:** the method doesn't exist.
- **`.slice(…)` and `.with_order_descending_multi(…)` without `?`:** each reads as a hash `for ::std::result::Result`, because the next call lands on the unwrapped `Result`.

**The cause, verified:**
- **Name recovery exists:** records 0040 and 0041 built it (`method::named`), and it's wired into `run`, the session (`Session::runtime_failure`, which proves names only against the call site's own retained input) and the server.
- **What it covers:** only the types in a fixed table, `known_types()`: ten Rune built-ins (`Vec`, `Object`, `String`, `Tuple`, `Bytes`, `Range`, `i64`, `f64`, `bool`, `char`). An adapter type (`::polars::DataFrame`, `::candle::Tensor`, …) or `::std::result::Result` isn't in the table, so recovery gives up and the hash stays.

**The fix, verified possible before planning:**
- **Rune derives a type's hash from its item path:** `Hash::type_hash(ItemBuf::with_crate_item(crate, rest))`.
- **A probe test** hashing each of the ten table entries' printed paths reproduced every `TypeHash::HASH` exactly. So did `::std::result::Result` against `Result<Value, Value>`'s hash.
- **So recovery can compute the type's hash from the very path the diagnostic prints,** for any type, with no table.

## 1. The change (`src/method.rs`)

**`named(message, source)` keeps its exact contract:**
- the whole message must be the diagnostic, start to end;
- candidates come only from the given source, in method position;
- a name is **proven** only when `Hash::associated_function(type_hash, candidate)` equals the reported hash;
- otherwise nothing changes, and the hash remains.

**Only the type's hash changes:**
- **Then:** it was looked up in `known_types()`.
- **Now:** it's computed from the printed path, parsed strictly: a leading `::`, then non-empty identifier segments.
- **A path that doesn't parse,** or any other shape, gives `None`, as an unknown type does now.

**`known_types()` stays.** It's what the existing tests and the display table use. A new test proves that for every entry, the computed hash equals the table's.

**The hint for an unwrapped `Result`:**
- **When it's added:** when the diagnostic's receiver is exactly `::std::result::Result` **and** the method name is proven, the message gains one line: "this value is a `Result`; did you mean to unwrap it with `?` first?"
- **Proven, not guessed:** the receiver type is the diagnostic's own, and the name is proven as above.
- **What it never does:** guess which earlier call produced the `Result`.
- **When it's absent:** with an unproven name, the hash stays and there's no hint.

**Every path that already calls `named` benefits:** `run`, sessions and eval, and the server. None of their attribution logic changes. The session still proves names only against the call site's retained input, and an unmapped origin still keeps the hash (0041's test stays as is).

## 2. Controls

- **The three exact 0139 failures, as session pastes** (the session driver):
  - `df.clone()` on a `DataFrame` reads "no method `clone` on `::polars::DataFrame`";
  - `opts.with_order_descending_multi(…).sort(…)`, without `?`, reads "no method `sort` on `::std::result::Result`" plus the `?` hint;
  - `df.slice(…).select_(…)`, without `?`, the same with `select_`.
- **Other adapter types:** `::candle::Tensor`, `::candle::TextEncoder`, and a Polars `Series` and `LazyFrame`.
- **Kept as hashes:**
  - a method called through a variable whose name isn't in the source (no candidate);
  - a message quoting the diagnostic inside other text;
  - a malformed path;
  - an unmapped retained origin (0041's control).
- **The table's equivalence:** the computed hash equals `known_types()`'s for all ten types and for `Result`.
- **`run` and the server** get the same named message for a script with the DataFrame case.

## Gates

- **Suites:** core, Polars, Candle and project; clippy.
- **0139's U4 replay** is unchanged; its script needs no change.
- **A session transcript** showing the three named messages, kept in the evidence (0123's display lesson).
- **`:dep polars candle`** after push.

## Out of scope

- Naming missing *functions* (not methods), or fields.
- Suggesting near-miss names ("did you mean `with_column`?"). A proven name only, as 0040 decided.
- Changing which errors are attributed where.
