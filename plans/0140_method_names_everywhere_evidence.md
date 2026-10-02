# rnx 0140 evidence: missing methods named on every type

**The result:** a missing method is now named on **every type whose printed path parses and whose hash proof succeeds,** adapters included, in a session, in `eval`, in `run` and through the server, by the same proof 0040 and 0041 established. A malformed or unreconstructible path deliberately keeps the original message, hash and all.
- **Example:** `df.clone()` reads "no method `clone` on `::polars::DataFrame`", where 0139 saw a hash.
- **A `Result` receiver** with a proven name also says "(this value is a `Result`; did you mean to unwrap it with `?` first?)". Two of 0139's three diagnosis rounds were that mistake.

## 1. The change (`src/method.rs`)

- **`type_hash_of(path)`** computes a type's hash from the path the diagnostic prints, as Rune derives it: `Hash::type_hash(ItemBuf::with_crate_item(first, rest))`.
  - **The parsing is strict:** a leading `::` and at least two segments, each exactly one identifier by **Rune's own lexer**.
  - **Refused:** empty segments, a trailing `::`, whitespace and control characters, generic and tuple syntax, digit-led segments, and keywords. An identifier Rune accepts is accepted.
- **`named(message, source)` keeps its contract:**
  - the whole message must be the diagnostic;
  - candidates come only from the given source, in method position;
  - a name is proven only when `Hash::associated_function(type_hash, candidate)` equals the reported hash;
  - otherwise the message is unchanged.
- **Only the type's hash moved** from the fixed `known_types()` table to `type_hash_of`. The table is now test-only, the equivalence reference.
- **The `Result` hint:** only when the receiver is exactly `::std::result::Result` and the name is proven. It never guesses which call produced the `Result`.
- **The callers are unchanged:** `run`, the session (still proving against the call site's retained input only), `eval` and the server.

## 2. Controls

**`src/method.rs` unit tests:**
- **The equivalence that replaces the plan's exploratory probe:** `type_hash_of` of each `known_types()` entry's printed path equals its `TypeHash`, for all ten, and `::std::result::Result` equals `Result<Value, Value>`'s.
- **Malformed paths refused:**
  - ``, `::`, `polars::DataFrame` (no leading `::`), `::polars` (one segment);
  - `::polars::` and `::polars::::DataFrame` (empty segments);
  - ` ::polars:: DataFrame` and `::polars::Data Frame` (whitespace), a trailing newline, a control character;
  - `::std::vec::Vec<i64>` and `::std::(i64, i64)` (generic and tuple syntax), `::polars::2DataFrame` (digit-led), `::polars::fn` (a keyword).
- **Named, synthetically:** `DataFrame` and `Tensor`, and a `Result` receiver with the hint.
- **Kept as the hash:**
  - no candidate in the source;
  - a `Result` receiver whose name isn't proven (no hint either);
  - the diagnostic quoted inside a panic;
  - a malformed path inside an otherwise well-formed diagnostic.

**The existing integration tests,** updated where they pinned the old table:
- **`tests/method_naming.rs`:** `#{a: 1}.values().frobnicate()` is now named on `::std::object::Values` in both `eval` and a session. Before, it kept the hash because `Values` wasn't in the table.
- **`tests/run_diagnostics.rs`:** the same type under `rnx run`, named, with its position and caret. The test still pins Rune's upstream message shape: a change would make the sentence disappear.
- **The quoted-panic and unmapped-retained-origin controls** (0041's) are unchanged and pass.

**The server** (`src/server.rs`, `server-runtime`): a script calling `frobnicate` on `Ok(a)` fails with "no method `frobnicate` on `::std::result::Result` (this value is a `Result`; …", at line 3.

**Real adapter diagnostics through the binary** (`probes/0140/names.py`, `out/names.txt`; runner0134 with this tree's Polars and Candle):

| mode | case | message |
|---|---|---|
| session | 0139: `df.clone()` | "no method `clone` on `::polars::DataFrame`" |
| session | 0139: `df.slice(0, 1).select_(…)` without `?` | "no method `select_` on `::std::result::Result` (this value is a `Result`; did you mean to unwrap it with `?` first?)" |
| session | 0139: `…with_order_descending_multi([true]).with_maintain_order(true)` without `?` | "no method `with_maintain_order` on `::std::result::Result` (…hint…)" |
| session | Series, Tensor, TextEncoder, LazyFrame | "no method `frobnicate` on `::polars::Series`", "…`::candle::Tensor`", "…`::candle::TextEncoder`", "…`::polars::LazyFrame`" |
| `eval` | Series | "no method `frobnicate` on `::polars::Series`" |
| `run` | a script's `df.clone()` | "…, line 3, column 5: no method `clone` on `::polars::DataFrame`" |

**Every probe case is named, and none keeps a hash.**

## Gates

- **0139's U4,** replayed on this tree as a session paste with the script unchanged: it passes, and is **BIT-EQUAL** to its twin with the table validated (`out/u4-replay.txt`).
- **Suites:**

| suite | passed |
|---|---:|
| core, with `server-runtime` | 408 |
| Polars | 41 |
| Candle, `test-support` | 61 |
| project tool | 102 |

  Clippy reports nothing in the files this record touches (`src/method.rs`, `src/server.rs` and the two test files). Pre-existing warnings elsewhere are unchanged.
- **`:dep polars candle`, after push** (`out/dep/`): a clean worktree binary at the pushed `d855722` and a fresh cache. The probe ran in `RNX_DEP=1` mode, the session cases after `:dep polars candle`; `eval` and `run` are skipped, because a `:dep` binary has no adapters outside a session.
  - **The first attempt** reported one case, `with_order_descending_multi` without `?`, as showing no error, while the other seven were named. It was the probe's fault: it attributed output to a case by waiting for the next prompt, and a prompt can be drawn before the error text arrives.
  - **The fix,** in the probe only: after seeing the prompt, it reads on until the output has been idle for 0.5 s.
  - **Two runs since,** with the same binary: **8 of 8 session cases named,** none keeping a hash (`names-1.txt`, `names-2.txt`). The runner-mode probe, rerun with the fix, names every case (`out/names.txt`).
