# rnx 0011: the second port

Status: proposed 2026-09-08. The eleventh record of rnx. Record 0010 added
three text helpers from the evidence of one script and said plainly that only
a second port could show whether they were the right three. This is that
port. It reports what a second real script exposed, and it does not treat
its own helpers as the thing to be justified.

## What was ported

`summarize_runtime_filter_perft.py`, in the Toron bench scripts. It reads raw
bench log lines, keeps the ones a distributed join bench emits, groups them by
seven case fields, and prints a markdown report of the reductions that the
runtime-filter discussions keep quoting: probe rows rejected, shuffle bytes
rejected, time in the bytes-movement cluster, wall time, digest parity, and an
implied Bloom false-positive rate. Four optional flags turn it into a gate,
three of them carrying a percentage floor.

It was chosen because it is nothing like the first port. The first was line
classification with integer counters and a fixed report. This one has
value-carrying flags, an input that may be standard input, a map keyed by a
tuple, floating-point arithmetic throughout, an optional-valued percentage
that has to render as `n/a`, and an exit code that depends on validation
failures accumulated while printing.

The input is real. Twenty-five bench rows are checked into two results
records, and the fixture is those rows verbatim rather than a sample written
to suit the port. Nine of the eleven groups in them are incomplete, because
the older rows predate a field and never carried a full grid of settings. The
port had to reproduce that partial report exactly, not just the tidy case.

## How it was compared

`fixtures/summarize_runtime_filter_perft/compare.sh` runs both
implementations over sixteen cases and compares standard output, standard
error, and the exit code.

| | Result |
| --- | --- |
| Standard output | identical on all sixteen, byte for byte |
| Exit code | identical on twelve; four differ, below |
| Standard error | as recorded on all sixteen, against a per-case expectation |

Separately from the harness, the invocation the bench README documents was
run through both implementations on the results record it names, from the
directory the README says to run it in. Identical report, exit 0 from both.

The gate was checked against four deliberate breakages. Changing a percentage
from one decimal to two put eight cases in the standard-output column. Making
a usage error succeed instead of failing was reported as a recorded exit code
that no longer holds. A runner that threw every diagnostic away and printed
one fixed line failed nineteen checks. A runner that hung was killed and
counted as a failure. It is not a gate that only knows how to pass.

### A hole in the first version of this gate

The gate as first written printed whether standard error differed and never
failed on it. Codex substituted a runner that kept standard output and the
exit status, suppressed every real diagnostic, and printed one fixed line on
every invocation including the successful ones. The gate still exited 0.

A column that reports without being able to fail is not a check, and the
distance between the two is the whole value of the gate. The exit codes had
been given a recorded expectation and the diagnostics had not, which is the
same discipline applied unevenly rather than a hard case.

Each case now carries a recorded expectation in `expected/`, in the three
groups the failures actually fall into: a successful case must say nothing, a
validation failure must print the bare `error: ` line described below, and an
input or usage failure must print the diagnostic that names what went wrong.
Where the original succeeds, neither implementation may write to standard
error at all. Two harness faults were fixed in the same pass: the script
shared fixed scratch filenames in the fixture directory, and it bounded no
invocation, so a hanging runner would have hung the gate. It now works in a
temporary directory it removes on the way out and treats a timeout as a
failure.

The port matched on the first run it compiled. Nothing about the report had
to be adjusted afterwards: the thousands-separated floats, the negative
percentage, the `n/a`, the twelve-character digest prefixes, and the ordering
of eleven groups all came out right the first time. That is worth recording
because the first port needed five rounds of print statements to find a
single arithmetic fault, and the difference is record 0009.

## What it exposed

### 1. There is no way to read standard input

The original reads standard input when no path is given, which is how a CI
lane would pipe a bench log through it. rnx has `host::read` for a path and
nothing for a stream. The port refuses with a message naming the gap rather
than pretending the case does not exist.

This is the first thing a second port asked for that the first did not.

### 2. Every nonzero exit prints something, and only 1 is reachable

A Rune script signals failure by returning an error, and the runner prints
that error and exits 1. Two consequences turned up in one script:

- The original exits 1 with a complete report on standard output and nothing
  on standard error. The port has to return an error to get the exit code,
  so it prints `error: ` with an empty message. The exit code is right and
  the report is right, and there is a stray line that should not be there.
- `argparse` exits 2 for a usage error. The port cannot exit 2. Four of the
  sixteen cases carry that difference, recorded rather than hidden.

An exit code is part of a script's contract with whatever runs it. rnx can
express two of the codes this one script uses and cannot express the third.

### 3. A missing method is reported by hash, not by name

Two methods this port expected do not exist in Rune 0.14.1. The diagnostic
for the first was:

```
runtime error at ..., line 73, column 2: Missing instance function
`0xf77d93259f11131a` for `::std::vec::Vec`
  	parts.join(UNIT)
```

Record 0009 put the file, the line, the column, and the source line in front
of a person, and that is what made this a ten-second fix rather than a hunt.
But the name `join` is in the source line by luck of it being a short
statement, not because the message carries it. On a longer line, or where a
method is called on a value whose type is not obvious, a hash is not an
answer. The message should name the method.

### 4. Rune has no `join` and no `contains` on a sequence

The two missing methods, found by needing them:

| Missing | Hand-written | Used for |
| --- | --- | --- |
| `join(parts, separator)` | 13 lines | the group label, the group key, the whole report |
| `contains(items, wanted)` | 9 lines | collecting the distinct digests in a group |

Twenty-two lines. Both are ordinary sequence operations rather than anything
this domain invented, and `join` was needed three times in one script.

These are candidates for a later cut, and this record does not add them.
Guardrail 1 of record 0010 asks for a script that needed a helper and a line
count showing it earns its place, which is now on the table for exactly two
things. It asks for that evidence before the helper, not after, and one
script is the same evidence the first port had. A third port deciding it
still wants `join` is a better reason than this record's impatience.

### 5. Of the three text helpers, this port used two

| Helper | Call sites here | What it did |
| --- | --- | --- |
| `text::find` | 3 | split a field at its first `=`, split a rendered float at its point, split a flag from an inline value |
| `text::group_digits` | 2 | the integer columns, and the integer part of a grouped float |
| `text::split_max` | 0 | nothing; this script never splits on whitespace |

`find` earned its place twice over: three call sites in a script that has
nothing to do with the one that introduced it. `split_max` was not needed at
all, which is the expected outcome for a helper drawn from one script's
shape rather than a wrong one. It is not evidence to remove it; it is
evidence that one port is one port.

### 6. `group_digits` covers integers, and the float case is at the call site

The original renders a float with both a thousands separator and three
decimals. `text::group_digits` takes a 64-bit integer, so the port formats to
three decimals first, finds the point, groups the integer part, and puts the
two back together: seventeen lines where Python writes one format specifier.

Doing it in that order is not a workaround, it is the correct order, because
grouping the rendered digits is what makes a number that rounds up into a new
digit group print the way its digits read. But it is at the call site, and
the next script that prints a grouped float writes it again. A separator on a
float belongs in formatting rather than in a helper, and neither this record
nor 0010 has an answer for it.

## What the port cost

| Point | Lines |
| --- | --- |
| The Python original | 321 |
| The Rune port | 461 |

1.44 times, against the first port's 1.7 after its helpers and 2.07 before
them. The ratio improved on a bigger script, and 22 of the 140 extra lines
are the two sequence helpers named above.

## What it did not expose

No arithmetic fault, no formatting mismatch, and no ordering difference. The
tuple-keyed grouping was reproduced by joining the seven values with a
separator that sorts below every character they can contain, and the eleven
groups came out in the original's order. Rune's `{:.3}` and Python's `{:.3f}`
agreed on every number in the corpus, including the halfway-looking ones.

## What this suggests, in order

1. **Standard input**, because a script that cannot be piped into cannot
   replace a filter, and this is the first port that wanted it.
2. **An exit code the script chooses**, and a way to fail without printing.
   Both are contract, not convenience, and this script needs both.
3. **A method name in the missing-method diagnostic**, which is a small fix
   to a message that already knows the answer.
4. **`join` and `contains`**, when a third port confirms them.
5. **A grouped float**, which is a formatting question rather than a helper.

Nothing here changes record 0010's three helpers. Two of them were used by a
script written for a different purpose, which was the question that record
left open, and the third was not needed by it, which is not the same as being
wrong.
