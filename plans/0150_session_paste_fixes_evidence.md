# rnx 0150 evidence: two session-paste fixes, a macro call after a block statement and `const` items

**The result:** both of 0149's paste findings are fixed in the session itself.
- **F1:** a multi-line macro call after a `for`, `if`, `while` or bare block now waits for the rest of the paste.
- **F2:** `const` items are accepted and retained like functions, and inspection labels them `const`.
- **The workarounds aren't needed:** 0149's U7 with both of its original forms restored pastes and runs, and its outputs are **byte-identical** to 0149's saved ones.
- **Nothing else changes:** every earlier completeness answer and every replay is unchanged.

## 1. F1: the cause and the change (`src/session.rs`)

**The cause**, measured before the plan with a probe of `completeness` on every line prefix (plan section 1):
- **Where Rune reports it:** after a block-ended statement, Rune places an open macro call's zero-width first error **one token inside** its opening delimiter. In 0149's U7 it landed at offset 2,144, the format string's quote, with the `(` at 2,143.
- **Why 0144's rule missed it:** `open_to_the_end` read from the quote, found no opening delimiter, and answered complete.

**The change:**
- **`delimiter_before`:** when 0144's zero-width error shape doesn't itself start an open delimiter, skip whitespace backwards from the error. If the character there is `(`, `[` or `{`, ask `open_to_the_end` from that delimiter.
- **Unchanged:** the delimiter-kind stack, the mismatched-closer rule and every other shape on the closed list.

## 2. F2: the change (`src/session.rs`, `src/inspect.rs`)

- **`ast::Item::Const`** joins `fn`, `struct` and `enum` as a retained declaration.
  - **Not a type:** redeclaring it replaces it.
  - **Its name** passes `valid_name`.
  - **Its source** is re-emitted at module level in every later unit.
- **The refusal's message** now lists `const`. Modules, imports and impl blocks are still refused.
- **The recorded kind** (Codex's clarification at plan acceptance): each declaration now stores its kind, "function", "const", "struct" or "enum", from the parsed item when it's entered.
  - **What changed:** `Session::declaration`, which `:help` uses, reports that kind. It used to call every non-type a "function", and decide struct-or-enum by whether the text started with `struct`, which also mislabelled a `pub struct`.

## 3. Controls

**`completeness` unit tests (`session::tests`):**
- **Now incomplete:**
  - a two-line `println!` after each of `for`, `if`, `while`, `loop` and a block;
  - `format!(`, `vec![` and `println!{` after a block statement;
  - the first argument on the next line;
  - a nested `foo(format!("{}",` after a `for`;
  - **0149's exact U7 prefix** (`probes/0150/u7_prefix.rn`, `the_exact_u7_prefix_is_incomplete`).
- **Still complete after a `for`:**
  - a finished call;
  - an extra closer;
  - all six mismatched-closer kinds;
  - a real error before the open call.
- **Every earlier case** of both lists is unchanged.

**`const` tests (`session::const_tests`):**
- **Retention:** a `const` used in a later input at the top level, and inside a function declared later; a `const` using another `const`; redeclared with a new value (replaced); `:reset` clearing it.
- **The recorded kind:** `const`, `pub const`, function, `pub struct` and `enum`, each reported by `Session::declaration`.
- **Transactional, with the origin kept:**
  - `const A = 1`, then `const C = A + 1`;
  - redeclaring `A` as `"x"` makes the retained `C` fail to compile, reported **at input 2, line 1, column 11** (inside `C`, where it was entered);
  - the previous `A` and `C` are still the working ones (1 and 2).
- **Rules that still apply:**
  - `main` and `__rnx_*` are reserved;
  - a `const` can't replace a struct: "is a type whose shape changed";
  - `mod`, `use` and `impl` are still refused, with the new message.

**Inspection (`inspect::tests::help_names_a_retained_const_by_its_recorded_kind`):** `:help` describes `LIMIT: const` with its source, `pub const SHOWN` as a const, and `pub struct Q` as a struct.

**Real sessions** (`probes/0150/paste.sh`, 0133's terminal driver):

| reproduction | before (the pushed 0149 runner, `out/before.txt`) | after (`out/after.txt`) |
|---|---|---|
| a `for`, then a two-line `println!` | FAILED | OK |
| an `if`, then the same | FAILED | OK |
| a `while`, then the same | FAILED | OK |
| a block, then the same | FAILED | OK |
| two `const` items used by a function and `run` | FAILED | OK |

**0149's U7 with both originals restored** (`probes/0150/u7_restored.rn`, `out/u7-replays.txt`):
- **The script:** 0149's committed script with only the two `const` system prompts and the two-line `println!` after its `for` loop put back.
- **The result:** it pastes and runs, and its `u7-triage.tsv`, `u7-summaries.tsv` and `u7-steps.tsv` are **byte-identical** to 0149's saved outputs.
- **Unchanged:** 0149's committed U7 (with the workarounds) also replays byte-identically. The frozen prompts and parsing are untouched.

**Replays:**
- **0144's four reproductions** (`println!(`, `format!(`, `println![`, `println!{`) still pass (`out/replays-0144.txt`).
- **0144's E6** with its two-line `println!` pastes, and its trace is byte-identical to 0143's, **BIT-EQUAL** to the twin (`out/replay-e6.txt`).

## Gates

- **Suites:**

| suite | passed |
|---|---:|
| core, with `server-runtime` | 413 |
| project tool | 105 |
| Polars | 43 |
| Candle | 97 |

- **Tooling:** fmt and `git diff --check` are clean.
  - **Root clippy** reports three warnings in `session.rs`, at code this record didn't change: they're at the same code in `HEAD`, part of the inherited baseline.
- **`:dep polars candle`, after push** (a clean worktree binary at the pushed `5b14b1f`, a fresh private cache; `out/dep/`): the restored U7 pastes into an ordinary `:dep` session (the `:dep` took 392.6 s). Its `u7-triage.tsv`, `u7-summaries.tsv` and `u7-steps.tsv` are **byte-identical** to 0149's saved outputs.
