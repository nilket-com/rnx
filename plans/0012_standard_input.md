# rnx 0012: standard input

Status: proposed 2026-09-08. The twelfth record of rnx. The second port
wanted standard input and could not have it, and that is the first thing
record 0011 asks for. This record decides what reading it means.

## Context

The script ported in record 0011 reads standard input when it is given no
path, which is how it would sit in a pipeline behind a bench run. rnx has
`host::read`, which takes a path, and nothing that reads a stream, so the
port refuses that case with a message naming the gap. It is the first thing
a second real script asked for that the first did not.

Three entry points share one context: `eval`, the interactive session, and
`run`. Anything registered on the host module is callable from all three,
and in a session the terminal belongs to the line editor. A function that
reads standard input without deciding what a terminal means would take the
characters a person is typing at the prompt, or block a script for ever
waiting for a stream that nobody is going to send.

## Decision

### 1. `host::stdin()` returns the whole of standard input as text

It mirrors `host::read`: one call, the whole stream, decoded as UTF-8. Not a
line iterator, because Rune has no host iterator that suits one, and the
scripts that want this split the text themselves. The port already does.

### 2. A terminal is refused rather than read

If standard input is a terminal, the call fails with a message saying so and
naming what to do instead. This is one decision covering two hangs:

- In the session, the line editor owns the terminal. A script that read it
  would swallow the next thing the person typed.
- Under `run` with no redirection, reading a terminal blocks until someone
  types an end-of-file, which looks exactly like a hung script.

A pipe, a redirected file, and a closed stream are all read. Only a terminal
is refused, and it is refused immediately.

### 3. The stream is read once, and a second read is an error

The first call consumes the stream. A second call fails with a message
saying it was already read, rather than returning the empty string that a
consumed stream would give.

The alternative is what most languages do: return nothing the second time.
It is rejected because nothing is also what an empty stream returns, so the
two cases would be indistinguishable at the point where a script is wrong
about which one it is in. A script that means to read once is unaffected,
and a script that reads twice by accident is told.

### 4. The same limit and the same refusals as `host::read`

Eight mebibytes, and the message says so in the same words. Input that is
not UTF-8 is refused the way a file that is not UTF-8 is refused. A stream
larger than the limit is a streaming problem, and streaming is not in this
cut.

### 5. It is discoverable the way every other host function is

Its path and its one-line description are recorded at the registration, so
completion and `:help` cover it without a second list, and its description
names a result because it returns one. Record 0010 settled both.

### 6. What this record does not decide

Writing to standard error from a script, which is the exit-status cut's
neighbour and not this one's. A script-chosen exit code, which record 0011
puts next and which this record leaves alone. Streaming a stream larger than
the limit. Reading standard input in the session for any purpose other than
refusing it.

## Acceptance gates

1. **A pipe is read.** A script that reads standard input and prints what it
   got receives exactly the bytes that were piped in, including a final
   newline or its absence.
2. **A redirected file is read**, and gives the same text as `host::read` of
   the same file.
3. **An empty stream is not an error.** Standard input closed with nothing in
   it gives an empty string, and the script carries on.
4. **A terminal is refused, and immediately.** Under a pseudo-terminal, both
   a script run from a file and an input typed at the session prompt get an
   error naming the terminal, and neither blocks. The session's prompt is
   still usable afterwards, and the characters the person types next are
   still theirs.
5. **A second read is refused.** The message names that the stream was
   already read, and it is not the message for an empty stream.
6. **The limit holds, and the read stops at it.** More than eight mebibytes
   is refused, in the same words `host::read` uses, and nothing near the
   limit is refused wrongly. The read is also bounded rather than merely
   checked afterwards: a stream carrying one byte past the limit is refused
   without waiting for an end-of-file, so refusing a stream costs the same
   whatever is behind it. A file cannot show that, because a file ends; the
   gate needs a pipe whose write end stays open.
7. **Input that is not UTF-8 is refused**, and the message says so rather
   than producing replacement characters.
8. **It can be found at the prompt.** `host::` completes to it, `:help`
   describes it from its registration, and the description names a result.
9. **The second port closes its gap.** The summarizer reads standard input
   when given no path, as the original does. The comparison in
   `fixtures/summarize_runtime_filter_perft/compare.sh` gains a case that
   pipes the same rows into both implementations and requires the same
   report, the same exit code, and the recorded diagnostics. Record 0011's
   first finding is then answered, and the fixture README's note about it
   goes.
10. **Nothing regresses.** The gates of records 0002 through 0011 pass.

## Guardrails and stop conditions

1. Nothing in this cut changes how the session reads a line. The line editor
   keeps the terminal; this only refuses to fight it.
2. No streaming, no partial reads, no second entry point for a bigger stream.
   If a real script needs one, that is its own record with its own evidence.
3. The refusals are behaviour, not documentation. A gate that cannot fail is
   not a gate, which record 0011's amendment paid for once already.
4. If refusing a terminal turns out to break a use that matters, the stop is
   to record it rather than to add a flag that turns the refusal off.

## Risks

- **A script that worked by accident now fails.** Nothing reads standard
  input today, so there is nothing to break; the risk is entirely in the
  second read, and gate 5 is the thing that makes that choice visible.
- **The eight mebibyte limit is wrong for a bench log.** It may be. The limit
  is the one `host::read` already has, and moving both is a decision with
  evidence behind it rather than a number changed in one place.

## Forward

The exit status a script chooses, which record 0011 puts second and which
this record deliberately leaves alone; writing to standard error, which that
cut will have to face at the same time; and the third port, which decides
whether `join` and `contains` are earned.
