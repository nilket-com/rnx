# rnx 0008: the first port

Status: findings, 2026-09-08. Not a decision record. This is the first port
of an existing utility. rnx has run a real workload before, the nineteen
action Polariton harness the spike reproduced, but that was written for it;
this one was written for Python and had to be met on its own terms. One
repository script was ported and run beside its original until the two
agreed. This records what that cost and what it exposed.

## What was ported

`nilket/scripts/classify_fold_perf.py`, 190 lines, in the ket repository. It
was chosen over the other three legacy Python scripts because it exercises
what the first release record's host library claims to cover: a subprocess
with captured output, JSON parsing, file reading, error paths, and a
non-trivial amount of text handling and arithmetic. The other three are
worse fits: one needs streaming binary pipes, which rnx cannot do at all;
one needs HTTP, TOML, and dates, which the host library does not have; and
one exercises neither subprocesses nor JSON.

The port is `nilket/scripts/classify_fold_perf.rn`, beside the original.
Neither replaces the other yet.

## How they were compared

No perf capture or node profile is checked into the repository, so both
implementations were run against the same fixtures: a stand-in `perf` on the
path emitting a fixed sample stream, and a hand-written node profile. The
comparison is therefore implementation against implementation on identical
input, which is what the agreement claim rests on. It is not a claim about
real perf output.

| Check | Result |
| --- | --- |
| Report on the fixture | byte-identical |
| Exit code on success | 0 for both |
| Exit code on a missing stage, and on a missing file | 1 for both |
| Failure message content | differs; see below |

## What the language cost

Each of these was found by the port failing, not by reading documentation.

1. **No regular expressions.** The original's single use, a file marker
   followed by a line number, became a hand-written index-of and digit scan.
2. **No `String::find`.** The port carries its own index-of.
3. **No `split_whitespace`.** The port carries its own, including Python's
   split-with-a-maximum semantics, where the last field keeps its spaces.
4. **No `min` or `max` over an iterator of numbers, and no `str::repeat`.**
   Folded by hand.
5. **No thousands separator in formatting.** Hand-written.
6. **Rune left-aligns numbers where Python right-aligns them.**
   `format!("{:7.2}", 12.3456)` is `"12.35  "` in Rune and `"  12.35"` in
   Python: Rune pads on the right, Python on the left. Every width in the
   port needed an explicit `>`. Nothing fails; the columns just come out
   wrong, which is the worst kind of difference.
7. **No coercion between integers and floats.** Mixed arithmetic is a
   runtime error. A counter's default zero has to match the kind of what is
   added to it, which is a bug the original cannot have.
8. **`x is Some` does not compile**, because `Some` is a variant rather than
   a type. Options need `match`.

## What rnx cost

These are rnx's own defects, found by using it.

1. **A compile error from `run` prints a raw `Diagnostics { .. }` debug
   struct and then the whole generated source.** Record 0002 gave the
   session mapped diagnostics with a position and a caret; `run` never got
   them. On a 300-line script the error is a wall of text with the useful
   part in the middle.
2. **A runtime error from `run` has no position at all.** Locating the
   mixed-arithmetic error in finding 7 above took five rounds of inserting
   print statements, because nothing said where it happened. In the session
   the same class of error names its input, line, and column.
3. **`host::read` does not name the file it failed to open.** Its error is
   `No such file or directory (os error 2)`, where Python's names the path.
   A script reading several files cannot say which one was missing without
   wrapping every call.
4. **A script's own error is presented wrapped and double-escaped**, as
   `Error: "script returned Err: \"...\""`, where the original prints the
   message plainly.
5. **`run` printed `null` for a script returning unit.** Fixed in this cut,
   because the session already decided that an input producing unit prints
   nothing, and `run` disagreeing with it was an inconsistency rather than a
   design.

## What it cost to write

The port is 394 lines against the original's 190. Some of that is the
repository's wider brace style, but the hand-written index-of, whitespace
splitting, digit scanning, and numeric grouping are most of it, and they
will be written again by the next port. That ratio is the adoption problem
the proposed text helpers have to solve: a runner that doubles the size of
every script is a hard sell whatever else it offers.

## What went well

The host library's shape held. `host::process` with an argument array,
`host::json_parse`, and `host::read` covered the script's needs without a
workaround, and the JSON walk was shorter in Rune than in Python. Bounded
capture and the deadline never came up because the child is small, which is
the right outcome. Interactive debugging in the session worked exactly as
record 0002 intended: every hypothesis about a missing method or a type
mismatch was answered in one line at the prompt, and that is how findings 1
through 8 were found so quickly.

## What this suggests, in order

1. **Give `run` the session's diagnostics.** Findings 1 and 2 of the rnx
   list are the difference between a usable runner and a frustrating one,
   and the mapping already exists.
2. **Name the path in every host error that touches a path.** Finding 3.
3. **Present a script's error plainly.** Finding 4.
4. **Leave Rune's alignment alone and document it.** Matching Rust is what
   Rune does and changing it would be rnx overriding the language it hosts.
   The answer is to say so where a person porting will read it, since the
   difference is silent and a port from Python is exactly what rnx is for.
5. **A small text library**, measured against the 394 lines this port took
   to replace 190. Index-of, whitespace splitting, digit scanning, and
   numeric grouping were all hand-written in one script, and the next port
   writes them again.

Regular expressions remain a second-release question, and this port is one
data point for it: the single use here was cheap to replace by hand.
