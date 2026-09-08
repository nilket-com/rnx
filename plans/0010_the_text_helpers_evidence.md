# rnx 0010 evidence: the text helpers

Implementation of `plans/0010_the_text_helpers.md`, 2026-09-08, on Linux
x86_64 against upstream Rune 0.14.1 with no patch. Status of the record
stays proposed until reviewed.

## What landed

- `src/text.rs`: `find`, `split_max`, and `group_digits`, installed as a
  `text` module. Each records its path and its one-line description at the
  registration itself, so completion and `:help` have no second list.
- `src/complete.rs`: the module prefixes a bare word completes to are now
  derived from the registered paths rather than from a hardcoded `host::`,
  so a module added later is completable without touching completion.
- `src/main.rs`: both modules are installed and their registrations joined.
- `nilket/scripts/classify_fold_perf.rn` in ket: the three hand-written
  helpers are gone, replaced by calls.

## Gates

| Gate | Result | Where |
| --- | --- | --- |
| 1 the port shrinks and does not change | pass: 394 lines to 328, output byte-identical to the Python original on the fixture, exit codes and failure messages unchanged | measured below |
| 2 each helper is worth its place | pass: 66 lines removed against three functions | below |
| 3 `find` behaves as Rust's does | pass: needle at the start, in the middle, at the end, absent, longer than the haystack, empty needle in a non-empty and an empty haystack; and a byte index a range accepts, checked on `café au lait` where `é` occupies two bytes | `src/text.rs::find_behaves_as_rusts_does`, `find_returns_a_byte_index_a_range_accepts` |
| 4 `split_max` behaves as its table says | pass: negative count errors and names the count; zero returns nothing; one strips leading whitespace and keeps the remainder with its trailing whitespace; empty and whitespace-only give nothing at counts one and three; runs of whitespace, fewer than the maximum, exactly the maximum, more with the remainder verbatim, and tabs and newlines as separators | `split_max_follows_its_table`, `split_max_keeps_the_remainder_verbatim` |
| 5 `group_digits` groups to both boundaries | pass: zero, one digit, exactly three, four, a negative, `i64::MAX`, and `i64::MIN` | `group_digits_groups_to_both_boundaries` |
| 6 they can be found at the prompt | pass: `tex` completes to `text::`; `text::` lists all three and nothing else; `:help` describes each from its registration; and both work from the session. Every description that names a return type names a result where the function returns one, which a test enforces across both modules | `tests/repl.rs::gate_0010_the_text_helpers_can_be_found_at_the_prompt`, `every_registered_function_carries_a_description`, `description_tests` |
| 7 nothing regresses | pass: 74 unit tests, 14 pseudo-terminal gates, 12 runner diagnostics, 3 run-output, 2 upstream reproducer, formatting clean, spike self-checks pass | whole suite |

### Gate 1 and 2, measured

| Point | Lines |
| --- | --- |
| The Python original | 190 |
| The port before the helpers | 394 |
| The port after | 328 |

Sixty-six lines removed, against three functions in `src/text.rs`. What each
one took out of the script:

| Helper | Lines it removed |
| --- | --- |
| `text::find` | 16, a hand-written index-of |
| `text::split_max` | 37, a hand-written whitespace splitter and its call sites |
| `text::group_digits` | 13, a hand-written digit grouper |

The port is still 1.7 times the original rather than 2.1. The rest is not
text handling: it is the brace style, the explicit `>` on every format
width, and the counters that need a zero of the right kind because integers
and floats do not mix.

### Gate 1, the comparison

Output on the fixture, which includes the samples containing `½`:
byte-identical to the Python original. Failure behaviour, all exiting 1 in
both:

| Case | The original | The port |
| --- | --- | --- |
| A node profile with no reconciled stage | `no reconciled row_lookup_update stage in <path>` | `error: no reconciled row_lookup_update stage in <path>` |
| A missing node profile | `FileNotFoundError: ... '<path>'` | `error: cannot read <path>: No such file or directory (os error 2)` |
| Too few arguments | `IndexError: list index out of range` | `error: usage: classify_fold_perf.rn CELL KEY-LINES UPDATE-LINES RUN.data...` |

The messages differ in wording, as they did before this cut, and both name
the path in the two cases that have one. The port's usage message is better
than the original's crash; that difference predates these helpers.

## What this cut did not need

No fourth helper was added. `char_indices`, `trim_start`, `repeat`, and
`min` or `max` over an iterator are still missing from Rune and were still
not needed by this script; guardrail 1 keeps them out until one is.

## Limits stated

- `split_max` splits on `char::is_whitespace`, which is Unicode's
  definition. The equivalence with Python holds for text whose separators
  are in that alphabet and for counts of one or more.
- `group_digits` takes a 64-bit signed integer and writes a comma. Another
  separator, another width, and another integer type are all questions this
  cut did not answer because no caller asked.
- A description carries a signature and a sentence, and now says `Result`
  where the function returns one. That correction applied to the host module
  too, where seven descriptions named their success type and mentioned an
  error only in prose.
- The helpers live in rnx, not upstream. Record 0010's decision 4 governs
  their removal: a matching name upstream is not enough, and a migration is
  recorded before any goes.
