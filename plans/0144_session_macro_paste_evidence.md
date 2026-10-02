# rnx 0144 evidence: a multi-line macro call no longer ends a pasted input early

**The result:** a pasted function whose macro call spans lines now waits for the rest of the paste. That covers `println!`, `format!`, and macros delimited by brackets or braces.
- **0143's E6** pastes with its original two-line `println!`, and is BIT-EQUAL to 0143's twin trace.
- **Before the fix:** every reproduction failed. **After:** every one passes.
- **Existing answers:** nothing else about completeness changed. Every existing case keeps its answer, and every session-paste replay is unchanged.

## 1. The change (`src/session.rs`)

**One more shape on the closed list of incomplete inputs,** recognised by Rune's own tokens, never by message text:
- **The trigger:** Rune reports an unterminated macro call's first error as a **zero-width span at its opening delimiter**, not at the end of the input (measured: `fn f() {\n    println!("{} and {}",` gives `21..21`). The trigger is that error shape, with text after it.
- **The tokens:** `open_to_the_end` reads the tokens from that position with the public `Parser::parse::<ast::Token>()`, keeping a **stack of delimiter kinds** (Codex's clarification to the plan):
  - **incomplete:** the end of the text is reached with a delimiter still open;
  - **complete (a real error, shown at once):** the outermost delimiter closes before the end, **or any closer doesn't match the innermost open delimiter, at any depth;**
  - **a lexing error** (an unterminated string or comment inside the call) counts as open only when its span reaches the end of the text being read.
- **The existing rules are kept** in their places: the first-error trigger, 0134's trailing-trivia rule, and the grown-input check.

## 2. Controls

**`completeness` unit tests** (`session::tests`):
- **Now incomplete:**
  - `println!(` with arguments over lines, inside a function and at the top level;
  - `format!(`; `vec![` with brackets; a brace-delimited macro;
  - a macro nested in a call (`foo(format!("{}",`);
  - a trailing `// comment` after an open call;
  - an unterminated string inside the call;
  - a balanced nested prefix with only the outer delimiter open (`println!([1, 2], {3},`).
- **Still complete:**
  - a finished call, on one line and over lines;
  - an extra closer; a mismatched closer at the root;
  - a real error before an open call (`let = 1; println!(`);
  - **nested mismatches of every kind:** `println!([1}`, `println!([1)`, `println!({1]`, `println!({1)`, `println!((1]` and `println!((1}`. A plain depth counter would hold these open.
- **Unchanged:** every case of the two existing lists.

**Real sessions** (`probes/0144/paste.sh`, 0133's terminal driver, runner0134). The reproductions are `println.rn`, `format.rn`, `bracket.rn` (`println![…]`) and `brace.rn` (`println!{…}`), each a function with a macro call over two lines:

| reproduction | before (0143's tree, `out/before.txt`) | after (`out/after.txt`) |
|---|---|---|
| `println!(` | FAILED | OK |
| `format!(` | FAILED | OK |
| `println![` | FAILED | OK |
| `println!{` | FAILED | OK |

**E6 with its original two-line `println!`** (`probes/0144/e6_multiline.rn`: 0143's script with only that line restored, `out/e6-multiline.txt`): it pastes and runs. It's **BIT-EQUAL** to 0143's twin trace (each trace validated first): 23 iterations of 252 assignments, and 2,304 centroid values.

## Gates

- **Session-paste replays** (`out/replays.txt`), on the fixed runner:
  - E1 EQUAL (25 rows), E2 EQUAL (16 rows), and E3 identical to 0134's saved output;
  - E4 EQUAL (3 rows) and E5 EQUAL (11 rows);
  - U4 BIT-EQUAL (252 tickets).
  - **A mistake of mine:** the first attempt at E1 to E3 passed the dataset's parent directory (they take `d1/plans`). Those runs aren't in the record.
- **Suites:**

| suite | passed |
|---|---:|
| core, with `server-runtime` | 408 |
| project tool | 105 |
| Polars | 43 |
| Candle | 74 |

  `cargo fmt` and `git diff --check` are clean.
  - **Clippy:** the doc list item this record extended now has its following paragraph separated (no warning). Three other `session.rs` warnings, at lines 703, 1657 and 1660, are in code this record doesn't touch, and are unchanged.
- **Launch** (`probes/0144/launch.py`, 0129's method; stock `rnx` at 0143's `f9499ee` against this tree): deltas of −1.00, −0.72 and +0.27 ms. **Within noise;** completeness isn't on the launch path.
- **`:dep polars candle`, after push** (a clean worktree binary at the pushed `7ba2057`, a fresh private cache; `out/dep/dep-polars-candle.txt`): the four reproductions all pass, each pasted after `:dep polars candle`. The restored multi-line E6 pastes, runs, and is BIT-EQUAL to 0143's twin trace.
