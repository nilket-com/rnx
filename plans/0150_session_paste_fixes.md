# rnx 0150: two session-paste fixes, a macro call after a block statement and `const` items

Status: plan. The user (2026-10-02) put these before more model work: pasting examples reliably matters more now than another model family. They are 0149's findings F1 and F2.

**The two problems, reproduced and explained before this plan:**
- **F1:** a multi-line macro call still ends a pasted input early, but only when a **block-ended statement** (a `for`, `if`, `while` or bare block) comes before it in the same function. Alone, the call pastes fine, which is why 0144's tests missed it.
- **F2:** `const` items are **refused** by the session, not lost between inputs: they fall through to "a session accepts fn, struct, and enum declarations". Every later input that uses one then reports it missing.

## 1. F1: the cause, measured

**0144's rule:**
- **What it recognises:** an open macro call by its first parse error, a zero-width span at the call's **opening delimiter**. It then reads Rune's tokens from there (`open_to_the_end`) to see whether the delimiter is still open at the end of the input.
- **The measurement:** after a block-ended statement, Rune places that zero-width error **one token inside** the delimiter instead, at the first argument. A throwaway probe of `completeness` on every line prefix showed this:

| input before `println!("{} a",` | the zero-width error | `completeness` today |
|---|---|---|
| nothing | at `(` | incomplete (right) |
| `let s = 0;` | at `(` | incomplete (right) |
| `for r in [1] { s += r; }` | after `(` | **complete (wrong)** |
| `if true { 1; }` | after `(` | **complete (wrong)** |
| `while false { }` | after `(` | **complete (wrong)** |
| `{ 1; }` | after `(` | **complete (wrong)** |

- **0149's U7** had `for r in summaries { … }` just before its two-line `println!`. The error landed at offset 2,144, the format string's opening quote, one past the `(` at 2,143. `open_to_the_end` started reading at the quote, saw no opening delimiter, and answered complete.

## 2. F1: the change (`src/session.rs`, `completeness`)

**Where the zero-width error isn't at an opening delimiter, look back for one:**
- **The look-back:** skip whitespace backwards from the error. If the character found there is `(`, `[` or `{`, ask `open_to_the_end` from **that** position.
- **The verdict:** the input is incomplete only if that delimiter, counted by Rune's own tokens with 0144's delimiter-kind stack, is still open at the end.
- **What doesn't change:**
  - every other shape on the closed list keeps its place and its answer;
  - a mismatched closer at any depth is still a real error, shown at once;
  - an outermost delimiter that closes before the end is still complete.
- **Why it's narrow:** the look-back runs only on the one error shape 0144 already recognises (zero-width, with text after it), and only reaches over whitespace to a single delimiter character.

## 3. F2: `const` items become session declarations

- **The change:** `ast::Item::Const` joins `fn`, `struct` and `enum` as a retained declaration.
  - **Its name** is the const's name, checked by the same `valid_name` rule (`main` and `__rnx_*` reserved).
  - **Its source** is kept exactly as typed, as a function's is, and re-emitted at module level in every later unit.
  - **It isn't a type,** so redeclaring it with a new value replaces it, as a function does.
- **Still refused:** modules, imports, macro declarations and impl blocks. The refusal's message now lists `const` among what a session accepts.

## 4. Controls

**`completeness` unit tests:**
- **Now incomplete:**
  - a two-line `println!(`, `format!(`, `vec![` and `println!{` call after each of `for`, `if`, `while`, `loop` and a bare block;
  - the call's first argument on the next line (`println!(` then a newline);
  - a nested `foo(format!("{}",` after a `for`;
  - **0149's exact U7 prefix:** the `run` function through its two-line `println!`.
- **Still complete after a block statement:**
  - a finished call;
  - an extra closer;
  - a mismatched closer at any depth (the six kinds 0144 lists);
  - a real error before the open call;
  - an opening delimiter that its tokens close before the end.
- **Unchanged:** every case of 0144's and the earlier lists.

**Session unit tests, for `const`:**
- a `const` declared in one input and used in a later one, at the top level and inside a function declared later;
- a `const` that uses another `const` and a function;
- a `const` redeclared with a new value;
- `:reset` clearing it;
- the reserved names refused;
- the four other item kinds still refused, with the new message.

**Real sessions, through 0133's terminal driver:**
- **The reproductions:** `for`, `if`, `while` and a block, each followed by a two-line `println!`, plus a `const` script. **Before** (the pushed 0149 tree): each fails. **After:** each passes.
- **0149's U7 with both originals restored:** the two-line `println!` after its `for` loop, and the two `const` system prompts. It must paste and run, with outputs **byte-identical** to 0149's saved `u7-script` files.
- **Replays:** 0144's four reproductions and its E6, and 0149's U7 as committed (which uses the workarounds), unchanged.

## Gates

- **Suites:** core with `server-runtime`, the project tool, Polars and Candle.
- **After push:** `:dep polars candle` runs the restored U7 from a clean worktree binary, byte-identical to 0149's saved outputs.

## Out of scope

- **Other item kinds in sessions:** modules, imports, impl blocks and macro declarations stay file-only.
- **Any change to how inputs are compiled,** beyond accepting `const`.
