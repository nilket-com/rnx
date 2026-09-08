# rnx 0009: diagnostics for the file runner

Status: proposed 2026-09-08. The ninth record of rnx. The session tells a
person where an error is; `run` does not, and the first port measured what
that costs: five rounds of inserting print statements to find one mixed
arithmetic error, in a script the runner had already compiled and executed.
This record gives `run` the diagnostics the session has.

## Context

### What the session already does

Record 0002 gave the session a source map per compiled unit, keyed by the
input that produced it, and a diagnostic that names the input, the line, the
column, the offending line of source, and a caret under it. Record 0006 made
those maps outlive the units they describe for as long as anything can call
into them. A runtime error inside a function defined many inputs earlier
still reports the input that defined it.

### What `run` does

Nothing of the sort. A compile error prints Rune's `Diagnostics` structure
through its debug formatter and then the whole compiled source, so the
useful part is buried in a wall of text. A runtime error prints its message
alone, with no file, no line, and no column. Both already reach standard
error, which is the one thing about them that is right.

`run` does not build a generated source the way the session does: it
compiles the file as written. So the mapping it needs is simpler than the
session's, not harder. There is no wrapper to see past and no prelude to
account for; a position in the compiled unit is a position in the file.

## Decision

### 1. Every diagnostic names a place

A compile error and a runtime error both report the file path, the line, the
column, the source line itself, and a caret under the column. The three
existing entry points agree on that shape: the session names an input where
`run` names a file, and everything after that reads the same.

### 2. Runtime errors are located too, including inside called functions

A runtime error carries the unit and the instruction that raised it, which
Rune's debug information maps to a span. `run` resolves that span against
the file it compiled. An error inside a function called from `main`, or
inside a function called from that, reports the line of the failing
expression, not the line of the call. That is the case the port needed and
did not have.

### 3. The separation of the streams is preserved, not introduced

A runner failure already reaches standard error today, and a script's own
output already reaches standard output: a compile failure prints nothing to
stdout and its diagnostic to stderr. This record does not change that; it
requires that everything it adds keeps it, the source dump of decision 5
included. It is a guarantee to preserve and to test, not new behaviour to
claim.

### 4. A script's own error is presented plainly, whatever it holds

A script that returns an error has it printed on standard error without the
wrapping and double escaping of today's `Error: "script returned Err:
\"...\""`. The exit code stays 1, as it is.

An error is not always a string. `Err(value)` can carry any Rune value, so a
string prints as it was written, and anything else prints through the
bounded, protocol-free renderer records 0002 and 0004 already use: the same
limits on depth, length, and total size, and no formatting protocol invoked
through the virtual machine.

A returned error is a value, not a fault at an instruction, so it has no
source position and none is invented for it. Only a failure the runtime
raises has a place to point at.

### 5. The compiled source belongs behind a named flag

Nothing prints a whole source by default. `run` gains `--debug-source`,
which prints the source it compiled to standard error, alongside the
diagnostic rather than instead of it.

The flag is read only before the script path. Everything after the path is
the script's, verbatim, so a script argument that happens to read
`--debug-source` reaches the script and does not turn the dump on. That
boundary is the rule for every flag `run` ever gains.

### 6. The instruction budget is preserved

`run` has always executed under a two million instruction budget, and
nothing here changes it. Unlike a session input, a file cannot be
interrupted from the keyboard, so the budget is the only thing that stops a
script that loops for ever. A halt for want of budget carries no location,
and says it ran out of budget rather than that it has no position. Changing
the limit is a decision for its own record.

### 7. What this record does not decide

Any change to what the session reports, to exit codes, or to the host
library's error text, which the first port also faulted and which is its own
cut. Nor does it touch Rune's formatting or alignment, which stay as the
language defines them.

## Acceptance gates

1. **A compile error is located.** A file with a syntax error on line 12,
   column 5 reports that path, line, and column, prints line 12, and puts a
   caret under column 5. The whole generated source appears nowhere in the
   output.
2. **A runtime error is located.** A file whose `main` divides by zero on a
   known line reports that line and column with the caret.
3. **An error inside a called function is located at the failure.** A file
   where `main` calls `outer`, which calls `inner`, which fails, reports
   `inner`'s line, not the line of either call.
4. **The streams stay separated.** A script that prints and then fails puts
   its own output on standard output and every diagnostic on standard error,
   the source dump included, checked by capturing the two separately.
5. **A script's error prints plainly, and carries no invented position.** A
   script returning a string error has exactly that message on standard
   error, with no wrapper and no escaping, and exits 1. One returning a
   vector, an object, and a struct each print through the bounded renderer,
   and a deep or long value is elided by its limits. None of them reports a
   line or a caret, because a returned error is a value rather than a fault
   at an instruction.
6. **The source dump is available on request only, and cannot be triggered
   by a script argument.** `--debug-source` before the path prints the
   compiled source to standard error beside the diagnostic; without it no
   output contains the source. The same text after the path reaches the
   script as an argument and prints nothing extra.
7. **The port is easier.** The mixed arithmetic error that took five rounds
   of print statements to find is reproduced in a fixture, and its
   diagnostic names the line.
8. **Positions survive awkward source.** A file whose failing line contains
   tab indentation, and one whose failing line contains characters outside
   ASCII before the column, each report a column that matches what the
   session would report for the same text, counted the same way.
9. **A failure with no span says so.** Running a path that does not exist
   reports the path and the reason without a line, a column, or a caret, and
   exits 1.
10. **The budget still stops a runaway script.** A file containing an
    endless loop is halted, says how many instructions it exceeded, and
    exits 1, while a loop that ends inside the budget is untouched.
11. **A fault with no resolvable place says so.** A file with no `main`
    reports that no source position is available, while a returned error and
    an unreadable file, which never had one, do not.
12. **Nothing regresses.** The session's gates from records 0002 through 0007
    pass unchanged, and the run-output gates of the first port pass.

## Guardrails and stop conditions

1. No diagnostic is printed without a position unless the position genuinely
   cannot be resolved, and then it says so, as the session already does.
2. Nothing prints a whole source file by default.
3. The session's diagnostics are not weakened to share code with `run`; if
   sharing costs the session anything, the two keep their own paths.
4. Script output never moves off standard output.

## Risks

- **`run` compiles a file where the session compiles a generated source, so
  the shared part may be smaller than it looks.** That is a reason the
  mapping is simpler here, not a reason it is harder, and the gates are the
  same either way.
- **A runtime error may resolve to a span Rune's debug information does not
  carry.** Then it says the position is unavailable rather than guessing,
  which is the rule record 0006 already set for the session.

## Forward

The host library's errors, which do not name the paths they touch; the text
helpers the first port had to hand-write; and `eval`, which shares the
session's path and should be checked against these gates rather than assumed
to pass them.
