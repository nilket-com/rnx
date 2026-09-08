# rnx 0010: the text helpers

Status: proposed 2026-09-08. The tenth record of rnx. The first port took
394 lines to replace 190, and most of the difference was text handling
written by hand. This record decides which helpers rnx adds, and it decides
it from what one real script actually needed rather than from a guess about
what a library should contain.

## Context

### What the port hand-wrote, and what survived

The port was written before its author had read Rune's string and character
API properly. `split(char::is_whitespace)` filtered of empties replaces most
of a hand-written splitter, which removed eleven lines and changed no
output. `char::is_numeric` looked like the same kind of win and was not: it
accepts `½`, which the original never matched, so the port keeps an explicit
ASCII digit range and the fixture now carries a case that separates the two.
Both corrections are in record 0008.

What is still hand-written, after that:

| Helper | Lines | Why it is still there |
| --- | --- | --- |
| `index_of(haystack, needle)` | 16 | Rune has neither `find` nor `index_of` on a string |
| `split_ws_max3(line)` | 28 | `split` cannot stop after a number of fields and keep the rest verbatim, which is what Python's `split(None, 2)` does and what the port needs, because a symbol may contain spaces |
| `thousands(value)` | 13 | No digit grouping in formatting |

Fifty-seven lines of a 383-line script, and the next port writes them again.

### What upstream has

`split` with a string or a character predicate, `trim`, `starts_with`,
`contains`, `chars`, `char_indices` absent, `get` with a byte range,
`parse`, `replace`, `to_uppercase`, and the character predicates. Missing,
as far as this port found: `find`, `splitn`, `char_indices`, `trim_start`,
`repeat`, and `min` or `max` over an iterator.

## Decision

### 1. Three helpers, each earned by the port

`text::find(haystack, needle)` returns the byte index of the first
occurrence, or none.

`text::split_max(text, count)` splits on runs of whitespace into at most
`count` **fields**, the last keeping the rest of the line verbatim. It counts
fields, where Python's `split` counts splits, so `split_max(text, 3)` is
Python's `split(None, 2)`; the name says fields because that is what a reader
of the call site wants to know.

It returns a result, because one of its counts is an error rather than an
answer:

| `count` | Result |
| --- | --- |
| negative | an error naming the count; it is not treated as unlimited |
| zero | an empty list: no field was asked for |
| one | one field, leading whitespace removed and everything after it kept verbatim, trailing whitespace included; an empty or whitespace-only text gives an empty list |
| more | at most that many fields, the last keeping the rest verbatim |

Whitespace is what `char::is_whitespace` calls whitespace, which is Unicode's
definition and not only the ASCII five. Leading whitespace never appears in a
field, at any count. The equivalence with Python holds for text whose
separators are in that alphabet and for counts of one or more; it is not a
claim about every input Python would accept.

`text::group_digits(value)` renders a 64-bit signed integer with a comma
every three digits. The separator is fixed rather than a parameter, because
no caller has asked for another and a parameter invites a locale question
this record does not answer. A negative number groups its digits and keeps
its sign outside the grouping. The smallest representable integer is
included in the gates, because negating before grouping overflows on exactly
that value and on nothing else.

Nothing else is added in this cut. A fourth helper needs a second script
that needs it.

### 2. They live in `text::`, not in `host::`

`host::` is the trusted-local host: files, processes, JSON, the outside
world. These are pure functions of their arguments and belong beside it
rather than inside it, so that a reader can tell at a glance which calls can
fail because of the world and which cannot.

### 3. They mirror Rust where Rust has an answer

`find` is Rust's `str::find` in name, argument order, and return. Where
Rust has no answer, as with the maximum-fields split, the name says what it
does rather than borrowing a name that means something else. Nothing here
invents a spelling that upstream Rune is likely to take.

### 4. A helper goes only when upstream replaces its meaning

These exist because upstream does not have them, not because rnx wants its
own dialect, so each goes when upstream offers the same thing. The test is
semantic, not nominal: a Rune that gains something called `splitn` has not
necessarily gained splitting on runs of whitespace with a verbatim
remainder, and adopting it on the strength of the name would change what
every caller does. Removing a helper requires a comparison of behaviour
against the gates below, and a recorded migration for the scripts that call
it.

### 5. They are discoverable the way the host functions are

Record 0003 could not offer completion for the standard library because
upstream cannot enumerate it, and record 0004 could not describe it for the
same reason. That is exactly the problem this record would deepen by adding
functions a person cannot find. So `text::` registers its paths and its
one-line descriptions at the registration site, as `host::` does, and both
completion and `:help` cover it from there. A helper that cannot be found
at the prompt is not finished.

### 6. What this record does not decide

Regular expressions, which remain the second release's question and which
this port did not need; HTTP, TOML, and dates, which no port has yet
demanded; and any change to Rune's formatting or alignment.

## Acceptance gates

1. **The port shrinks and does not change.** The classifier is rewritten to
   use the three helpers. Its output stays byte-identical to the Python
   original on the fixture, its exit codes stay 0 and 1 on the same inputs,
   and its failure messages are unchanged. The line counts before and after
   are both in the evidence.
2. **Each helper is worth its place.** The evidence names, for each helper,
   the lines it removed from the port. A helper that removes fewer lines
   than it costs is not added.
3. **`find` behaves as Rust's does.** An empty needle, a needle longer than
   the haystack, a needle at the start, at the end, and absent; a haystack
   and a needle containing characters outside ASCII, where the index
   returned is a byte index that `get` accepts.
4. **`split_max` behaves as its table says.** Runs of whitespace, leading
   and trailing whitespace, fewer fields than the maximum, exactly the
   maximum, more than the maximum with the remainder kept verbatim including
   its internal spacing and its trailing whitespace, an empty string, and a
   string of only whitespace. Each of the four counts in the table: a
   negative count returns an error naming it, zero returns an empty list,
   one returns a single field with leading whitespace removed, and a
   whitespace-only text at count one returns an empty list. Where the input
   is separated by ASCII whitespace and the count is one or more, the result
   equals Python's.
5. **`group_digits` groups, to both boundaries.** Zero, a single digit,
   exactly three digits, four, a negative number whose sign is not grouped
   into, and both `i64::MAX` and `i64::MIN`. The smallest is the case that
   catches an implementation negating before grouping, because that is the
   one value whose negation overflows.
6. **They can be found at the prompt.** `text::` completes after `tex`,
   listing every registered path and only those; `:help` on each names it
   and describes it; and the registration is the single source, so a helper
   without a description fails a test rather than shipping.
7. **Nothing regresses.** The gates of records 0002 through 0009 pass, and
   the classifier still matches the Python original on the fixture,
   including its samples containing `½`.

## Guardrails and stop conditions

1. No helper enters without a script that needed it and a line count that
   shows it earning its place.
2. A helper that upstream Rune provides with the same behaviour is not
   duplicated. A name alone does not establish that; decision 4 says what
   removal requires.
3. Nothing in `text::` touches the world: no file, no process, no clock.
4. The port's output and failure behaviour are the acceptance bar; a helper
   that changes either is wrong.

## Risks

- **Three helpers today become thirty by inertia.** Guardrail 1 is the
  answer, and it is a rule about evidence rather than taste.
- **A helper diverges from the Rust behaviour it is named after.** Gate 3
  tests `find` against Rust's documented edge cases rather than against what
  the port happens to need.

## Forward

The second port, which decides whether these three were the right three;
`min` and `max` over an iterator, which the port folded by hand and which
may belong upstream rather than here; and the question record 0008 raised
about discoverability, since a person cannot find `char::is_numeric` from
the prompt and `:help` cannot list the standard library.
