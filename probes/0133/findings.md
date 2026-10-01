# 0133 findings log (working notes; the evidence is the record)

| # | where | status | finding |
|---|---|---|---|
| F1 | text | friction | Rune `String` has no `split_whitespace`; workaround: replace newlines and tabs, `split(" ")`, drop empties |
| F2 | text | friction | Rune `Vec` has no `join`; workaround: a loop with `+=` |
| F3 | text | friction | Rune `String` has no `strip_prefix`; workaround: `starts_with`, then byte slicing |
| F4a | run | friction | `rnx run` defaults to 2,000,000 instructions; chunking 321 documents halts. `--budget N` works (it needs about 50M) |
| F4b | project run | blocked | `rnx project run` accepts no budget option (`--budget` is refused in every position), yet its halt message says "--budget N raises it". Sessions allow 2,000,000,000 per input |
| F5 | project | friction | any edit to the entry script needs `project lock` and `build` again (about 1.6 s when the assembly attaches) |
| F6 | session | friction | no `:load`; the project entry isn't evaluated in a session, so a workflow's helpers must be pasted. Multi-line definitions do work |
| F7 | session | friction | a pasted `pub fn main` is refused ("`main` is reserved by the session"), so a run file and a session script need different entry names |
| F9 | session | friction | a compile error inside a pasted multi-line definition breaks the continuation: every later line of the paste becomes its own input, so one error cascades into a dozen misleading ones and leaves later definitions undefined ("Missing item run") |
| F10 | rune and polars | friction | `select` is a Rune keyword, so a method named `select` can't be called; the bindings are `select_` (DataFrame and LazyFrame), discoverable only from the catalogue. The error reads "Unsupported field access" |
| F11 | polars 0.55.2 | friction | no unpivot binding: finding pairs above a threshold in a similarity frame needs a per-column mask loop |
| F12 | readback | friction | no numeric readback from a Polars column or a Dense into Rune: values are cast to strings, read with `strings`, then parsed |
| F13 | candle | friction (performance) | embedding 3,658 near-256-token passages takes 387 s against PyTorch's 49 s: 0132's calibrated budget admits one long batch at a time. The twin's sequential 371 s confirms it is Candle's own cost when serialized |
| F14 | chunking | friction | under the frozen 180-word rule, 39% of D1 passages exceed 256 tokens (median 235, max 2,079), so their tails are truncated; a script has no token-aware way to split text |
| F15 | session | friction | a missing method in a session reads "Missing instance function `0x779d621a7d8e4d9d` for `::polars::BooleanChunked`": a hash, not the name (`bitand`); 0040's naming applies to `run` only. The fix (the `&` operator, a `Result` per 0113) is undiscoverable from the message. Likewise, calling `.collect()` on the `Result` that the generated `LazyFrame::select_` returns reads "Missing instance function `0x49dc…` for `::std::result::Result`", naming neither method |
| F16 | polars 0.55.2 | friction | two boolean masks can't be combined: `BooleanChunked` has neither `bitand` nor the `&` operator ("Unsupported binary operation `BIT_AND`"); workaround: filter twice |
| F17 | quality | finding | the frozen thresholds don't fit MiniLM on real tickets: across D2's 31,626 pairs the maximum similarity is 0.832 (reference model, diagnostic), so no pair reaches 0.85 or 0.95, and only 2 reach the U3 edge of 0.80. Genuine near-duplicates sit just below, for example #588 "Cannot mutate nested field" and #835 "Problem when change a inner field in a mutable struct" at 0.799 |
| F18 | session | friction | a macro call split across lines inside a pasted function (`println!("…",` then its arguments on the next line) ends the input early: "Expected close delimiter `)`, but got `}`", then F9's cascade. Workaround: one line per macro call |
