# rnx 0013: the exit status and standard error

Status: proposed 2026-09-08. The thirteenth record of rnx. Record 0011 found
that a script can reach only two exit codes and cannot fail quietly, and put
that second on its list. Record 0012 took the first item and said this cut
would have to face standard error at the same time. It does, because a usage
error that exits 2 has nowhere to say what was wrong.

## Context

A Rune script signals failure by returning an error. The runner prints that
error and exits 1. Every other outcome exits 0. Two things follow, and the
second port ran into both:

- **Only 0 and 1 are reachable.** The script it was ported from uses
  `argparse`, which exits 2 for a usage error. The port cannot, so four of
  the nineteen comparison cases carry a recorded difference that is nobody's
  defect and cannot be closed by writing better Rune.
- **A nonzero exit always prints.** The original exits 1 with a complete
  report on standard output and nothing on standard error. The port returns
  an error to get the exit code, so it prints `error: ` with an empty message.
  Six of the recorded expectations are that stray line.

An exit code is a script's contract with whatever runs it. A shell, a CI lane,
and a `Makefile` all read it, and none of them read prose.

## Decision

### 1. `host::exit(code)` ends the script with that status

It ends the process there. Nothing after it runs, which is the point: a
script that has decided its outcome should not have to thread a return value
back through every caller to say so.

The status is the one given. The runner keeps deciding the status of a script
that does not call it, so nothing already written changes meaning.

### 2. A status outside 0 to 255 is refused, not truncated

An exit status is one byte once it reaches the shell. `exit(256)` arrives as
0, which turns a failure into a success, and `exit(300)` arrives as 44. Both
are silent today in every language that passes the number straight through.

rnx refuses a status outside 0 to 255, names the number and what it would
have become, and **ends the script with 1**. It does not hand back an error
for the script to deal with, because an error is a value a script can discard,
and a discarded one would let a script that asked for an impossible status
carry on and exit 0. That is the exact outcome the refusal exists to prevent,
so the refusal cannot depend on the script cooperating.

In a script, then, `host::exit` never returns. At a session prompt there is
nothing to end, so the refusal there is an ordinary error the session reports
and recovers from; decision 4 covers that.

### 3. `host::eprint(text)` writes to standard error, verbatim

Without it, decision 1 is unusable for the case that motivated it: a usage
error can exit 2 and cannot say what was wrong.

It adds nothing to the text. A function that appends a newline cannot be
asked not to, and one that does not can be asked to, so the primitive is the
one that appends nothing.

### 4. `host::exit` is refused at the session prompt, and `host::eprint` is not

`host::exit` at a prompt would end the person's session, and `:quit` already
does that deliberately. It is refused there, as record 0012 refuses reading a
terminal, and for the same reason: the meaning is wrong rather than the
mechanism missing.

`rnx eval` is allowed, because it runs one thing and exits, which is what a
status is for. So the rule is not about which module is loaded but about
whether rnx is running a script or holding a prompt.

`host::eprint` is allowed everywhere. Writing a line to standard error at a
prompt is harmless and occasionally what a person wants.

### 5. Output is flushed before the exit, and losing it is never a success

Standard output is flushed before the process ends, including output with no
trailing newline, which is buffered where a completed line is not.

A flush can fail: a full disk, a closed reader, a device that refuses writes.
The script's report is then lost, and a status of 0 would say the script did
what it said. So a failed flush is named on standard error, and a status of 0
becomes 1. A status that was already nonzero is kept, because the script had
decided it was failing and that is still true.

The flush has to be rnx's own for this reason. Ending the process flushes
standard output anyway, but it discards the error while doing so, which is
the difference between losing a report and knowing that a report was lost.

### 6. What this record does not decide

Reading a status back from `host::process`, which already reports a child's
code and is unaffected. A signal as an outcome rather than a status. Any
change to what the runner does with a script that returns a value or an
error, which stays exactly as record 0009 left it.

## Acceptance gates

1. **A chosen status reaches the shell.** 0, 1, 2, 7, and 255 each arrive as
   themselves.
2. **A script can fail quietly.** `host::exit(1)` after printing a report
   leaves standard output intact and standard error empty.
3. **Nothing printed is lost.** Output with no trailing newline still arrives.
4. **A status outside the range is refused, not truncated.** 256 does not
   become 0, 300 does not become 44, and a negative status does not become
   255. Each is refused, the message names the number and what it would have
   become, and the process still exits nonzero.
5. **The refusal does not depend on the script handling it.** A script that
   asks for an impossible status and then discards the result, or binds it and
   ignores it, still ends there and still exits nonzero. Nothing after the
   call runs. The same holds from `eval`.
6. **Output that could not be written is never reported as success.** With
   standard output pointed at a stream that refuses every write, a script that
   prints and then asks for 0 exits nonzero and names the lost output. A
   script that asks for a status that was already failing keeps it, and the
   lost output is named beside it.
7. **Nothing after it runs.** A print after `host::exit` does not appear.
8. **Refused at the prompt, and the prompt survives.** Under a pseudo-terminal
   the session refuses it, says why, and the next input typed is still the
   person's own. A status that could never be valid is refused there the same
   way, because the prompt is the reason.
9. **Allowed from `eval`.** `rnx eval` with a chosen status exits with it.
10. **`host::eprint` writes exactly what it is given**, to standard error, with
    nothing added and nothing escaped, and it does not touch standard output.
11. **Both can be found at the prompt.** Completion lists them and `:help`
    describes them from their registration.
12. **The second port closes record 0011's second finding.** The summarizer
    exits 2 for a usage error as its original does, and exits 1 with an empty
    standard error where its original does. The four recorded exit-code
    exceptions in `compare.sh` go, and so do the six recorded `error: `
    expectations. The gate then requires the exit codes to match on every
    case with no exceptions.
13. **Nothing regresses.** The gates of records 0002 through 0012 pass.

## Guardrails and stop conditions

1. One way to choose a status. A returned value keeps meaning what record 0009
   says it means, and no integer return is given a second meaning.
2. The range check is not optional and has no flag. A truncated status is a
   silent wrong answer, which is the class of defect this cut exists to
   remove.
3. If refusing at the prompt turns out to block a use that matters, that is
   recorded rather than softened with a flag.

## Risks

- **An abrupt exit skips cleanup.** It does. `host::process` is synchronous
  and leaves no child behind it, and nothing else in the host holds a resource
  whose release is observable, so today the risk is theoretical. It stops
  being theoretical the first time the host owns something that must be
  released, and that cut inherits this one.
- **A script exits before flushing something rnx does not know about.**
  Decision 5 covers standard output, which is the only thing rnx buffers, and
  covers a flush that fails as well as one that succeeds.

## Forward

The third port, which decides whether `join` and `contains` are earned; a
signal as an outcome, if a port ever needs to report one; and record 0011's
remaining suggestion, a method name in the missing-method diagnostic.
