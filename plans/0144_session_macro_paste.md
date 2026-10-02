# rnx 0144: a multi-line macro call no longer ends a pasted input early

Status: plan, agreed with Codex and the user after 0143 (0144 this fix, then 0145 Candle signal operations). A paste failure obstructs every example, so it gets this short detour first.

**The finding (0143 F1):** in a session, a pasted function whose macro call spans lines is submitted before it ends. The continuation is then parsed as new inputs, and the function is never defined. The minimal reproduction:

```
pub fn run(args) {
    println!("{} and {}",
        1, 2);
    println!("WORKFLOW OK repro");
    Ok(())
}
```

E6 hit this and had to keep its macro calls on one line.
- **Multi-line ordinary calls are fine** (`foo(1,` then `2)`).
- **`rnx run` and `eval` are unaffected,** since they don't decide completeness line by line.

**The cause, measured before planning** (a temporary probe on `session::completeness`):
- **For an unterminated macro call,** Rune's parser reports its first error as a **zero-width span at the macro's opening delimiter,** not at the end of the input.
  - **Examples:** `fn f() {\n    println!("{} and {}",` gives `21..21`, the `(`; `println!("{}", 1` gives `8..8`. `vec![1,` and `format!(` across lines are the same, and so are all three delimiters (`(`, `[`, `{`).
- **`completeness` counts a zero-width error as incomplete only when it is at the input's end,** so these read as finished, broken inputs.
- **An ordinary call** (`fn f() {\n    foo(1,`) reports at the end (`19..19`), so it waits for more, correctly.

## 1. The change (`src/session.rs`, `completeness`)

One more shape joins the closed list of incomplete inputs, recognized by Rune's own tokens, never by message text:
- **The trigger:** the first error is zero-width, its position is an **opening delimiter,** and only then do the tokens from that delimiter to the end of the input decide.
- **The tokens** are read with the public `Parser::parse::<ast::Token>()`, keeping a **stack of delimiter kinds** (Codex's review of this plan: a plain counter would hold a nested mismatch such as `println!([1}` open).
- **Incomplete:** the end of the input is reached with the delimiter still open, so more lines can close it.
- **Complete (a real error, shown at once):**
  - the delimiter closes before the end;
  - or **any closer doesn't match the top of the stack,** at any nesting depth.
- **A lexing error** while reading those tokens keeps the existing rule: incomplete only if its span reaches the end of the text being read (the suffix from the delimiter, so spans are compared with the suffix's length), for an unterminated string or comment inside the call; otherwise complete.
- **Nothing else changes.** Every existing incomplete and complete case keeps its answer, 0134's trailing-trivia rule included.

## 2. Controls

**`completeness` unit tests:**
- **Now incomplete:**
  - `println!(` with arguments over several lines, inside and outside a function;
  - `format!(`, and `vec![` with brackets;
  - a brace-delimited macro;
  - a macro nested in a call (`foo(format!("{}",`);
  - a macro call followed by a trailing `// comment`;
  - an unterminated string inside a macro call;
  - a balanced nested prefix with only the outer macro delimiter open (`println!([1, 2], {3},`).
- **Still complete:**
  - a finished macro call (`println!("a {}", 1);`);
  - a macro call with an extra closer (`println!("a"));`);
  - a mismatched closer at the root (`println!("a"];`);
  - **nested mismatches of every kind:** `println!([1}`, `println!([1)`, `println!({1]`, `println!({1)`, `println!((1]` and `println!((1}`;
  - a real error before an open macro (`let = 1; println!(`);
  - every case in the existing lists.

**A session, end to end:** the 6-line reproduction, pasted through the real runner's terminal (`probes/0133/session.py`), defines `run` and prints its marker. So does the same with `format!` and `vec!` spanning lines. Before the fix, each fails (kept in the evidence).

**E6, with its original multi-line `println!` restored** (0143's single-line workaround reverted in a copy), pastes and runs in a session bit-equal to 0143's trace.

## Gates

- **Suites:** core with `server-runtime`, the project tool, Polars and Candle; clippy.
- **Replays, as session pastes:** 0134's E1 to E3, 0137's E4 and E5, and 0139's U4, unchanged (their inputs cross this rule).
- **Launch:** within noise.
- **`:dep polars candle`** after push, with the reproduction and the restored E6.

## Out of scope

- Other completeness heuristics (no change to how `run` and `eval` parse).
- Rune's macro parser itself (the error's position is upstream behaviour, and the fix reads tokens rather than relying on messages).
