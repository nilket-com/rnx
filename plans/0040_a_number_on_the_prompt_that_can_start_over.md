# rnx 0040: a number on the prompt, that can start over

Status: implemented 2026-09-14; Linux gates pass, Windows execution remains
unverified. Evidence is beside this record. The
fortieth record of rnx, and the second
about how a session reads. It puts a number on the prompt and the same
number on the result, so that a person scrolling back can tell which
input made which value; it lets that number start over without touching
the session's bindings, because the person who asked for it restarts and
pivots every few minutes and resets the count far more often than the
memory; and it keeps the number a diagnostic cites honest across that
restart, which is the part that takes a decision.

## Context

The session already numbers inputs. Every input that reaches evaluation
is pushed onto a list, and its position is the number a diagnostic
cites: "error at input 2, line 1, column 9". Measured today, through a
pipe:

```
rnx: a Rune session. :help lists the commands, :quit ends it.
1
error at input 2, line 1, column 9: Expected expression but got `;`
  let x = ;
          ^
no bindings; :help lists the commands
error at input 3, line 1, column 1: No local variable `x`
```

Three inputs, one of them a failure, one colon command between them
that did not count. That number is also a *source identity*: a closure
defined at input 4 and called at input 9 reports its error "at input 4",
and a test in `session.rs` holds that. `:reset` clears the list along
with the bindings, so the count starts again at 1 with an empty session.

What the person cannot see is the number *before* they type — the prompt
is `rnx> ` — and cannot see it beside a result afterwards, so tying a
value on screen to the input that made it means counting. And they
cannot restart the count without `:reset`, which also empties the
session. Codex's brief for this record: keep `:reset` clearing both
state and count, add `:renumber` to restart the count while preserving
bindings, settle what advances the count, and keep display numbering
separate from source identities so renumbering cannot mislabel a later
diagnostic.

Two facts about the editor shape the transcript question. rustyline
checks for an unsupported terminal — `TERM` of `dumb`, `cons25` or
`emacs`, compared case-insensitively — before it looks at standard
input, so a prompt is written in exactly two situations: a
terminal on standard input, which runs the editor, or unsupported-
terminal mode, which writes the prompt plainly before each line
whatever standard input is. Measured: `TERM=dumb` with a piped standard
input prints `rnx> ` prompts. Outside those two, with standard input a
pipe, there is no prompt at all. So a change to the prompt changes
nothing a piped test sees unless that test selected unsupported-terminal mode, and record
0039's rule that a pipe sees nothing new holds on the input side with
that one stated exclusion. The output side is where the decision is.

## Decision

### 1. The prompt carries the number of the input about to be typed

```
[1] rnx> let x = 1;
[2] rnx> x + 1
[2] 2
[3] rnx>
```

The number is in front, so the prompt still ends in `rnx> ` and every
existing test that waits for that suffix keeps working. The number is
the position the input will have if it reaches evaluation — the same
number a diagnostic about it will cite — so what a person sees before
typing is what an error will say afterwards. A multi-line input keeps
its one number; the editor's continuation is the same buffer under the
same prompt, as today.

### 2. A result carries the same number, exactly when a prompt does

A result is printed as `[n] value`, the number of the input that made
it, on the first line only; a value that renders over several lines
continues unprefixed, so its layout is unchanged. The marker is emitted
under one condition, the same one the prompt is drawn under: **a
terminal on standard input, or unsupported-terminal mode**, whatever
standard output is — and "unsupported" is rustyline's own predicate,
the three names above compared case-insensitively, so that the marker
and the prompt can never disagree about the mode. With standard input a pipe and an ordinary `TERM`
there is no prompt and there is no marker, and that is why every such
piped transcript is byte-for-byte what it was: record 0039's promise,
kept, with unsupported-terminal mode as the stated exclusion on both sides.

Only a printed value gets a marker, and what is printed does not change.
Today an input whose value is unit — `()`, or a declaration such as
`let y = 1;` — prints nothing, measured; that stays, so such an input
gets no result line and no marker, and the number simply moves on. A
diagnostic already names its input in its first word, a colon command's
output is not a result, and the banner and the messages about
abandonment and reset are not results.

One consequence is stated: a session run with a terminal on standard
input and standard output redirected already writes its prompts into
the redirect (record 0039 measured it), and now writes markers there
too. The tests that use `TERM=dumb` through pipes to obtain a prompt —
the HTTP session gates — see the prompt and marker change; together
with record 0039's bold-prompt pty test and any comparison of `:help`'s
full output, they are the tests gate 1 names as the only ones this
record may touch.

### 3. What advances the count, stated as today's behaviour

The count advances exactly when the session's input list grows: an
input that is **admitted** to evaluation, whether it then produces a
value or a failure. That is the rule the session already has, and it is
kept, so that the number on the prompt is the number a diagnostic cites.
Named for the record because each is a question someone will ask:

- **an error advances it**: input 2 failed above and input 3 followed;
  a failed input is still an input, is still in history, and a later
  diagnostic may cite it;
- **two refusals happen before admission and do not**: the allocation-
  ceiling refusal (record 0005) and the input-cap refusal (an input over
  `INPUT_CAP` bytes) are made before the input is pushed, measured in
  the source, so neither takes a number and the next prompt shows the
  same one;
- **a recognised colon command does not**: `:vars`, `:help`, `:debug`,
  `:memory`, `:renumber` and `:reset` are handled before evaluation and
  are not inputs; an **unrecognised** one — `:bogus` — is not a command,
  reaches evaluation, fails there as input *n*, and advances the count,
  measured;
- **Ctrl-C while editing does not**, since the line never reaches the
  session; **Ctrl-C after admission does**, since the input was pushed
  before it ran, and an interrupted input keeps its number as a failed
  one does;
- **an abandoned input does not**: two blank lines end a multi-line
  input without evaluating it, and it goes to history and nowhere else,
  as today;
- **an empty line does not**;
- **`eval` and `run` have no count**: there is no prompt, and the
  diagnostic's "input 1" for `eval` is unchanged.

### 4. `:renumber` starts a new numbering; identities keep the old one

`:renumber` sets the next prompt to `[1]` and **clears nothing**: no
binding, no declaration, no retained source, no history entry, no
input. It is not free of side effects — the command itself goes into
history as any typed line does, and recording a new numbering may
allocate a few bytes — so the promise is about contents, not about an
allocator figure, and gate 4 tests the contents. To keep a diagnostic
honest, a source identity becomes a pair: **a numbering and a position
in it**. The session starts in numbering 1; each `:renumber` begins the
next; `:reset` returns to numbering 1 at position 1 with everything
cleared, which is what it does today.

Internally the input list and the source map keep their stable indices;
nothing stored is renumbered. The numbering and position are computed
from an index only when an origin is *presented* — in a diagnostic or
on the prompt — so display numbers never serve as storage keys, and
retained code keeps pointing at the same source it always did.

A diagnostic cites the position, and names the numbering only when it
is not the current one:

```
[4] rnx> let c = |v| v.missing_method();
[5] rnx> :renumber
[1] rnx> c(1)
error at input 4 of numbering 1, line 1, column 12: ...
```

Within a numbering nothing changes: "at input 4" means what it meant.
Across one, the phrase says which numbering, so a `[1]` after a
`:renumber` can never be mistaken for the `[1]` before it. The position
is the number the prompt showed when that input was typed, in every
case, which is the whole point.

### 5. Nothing else changes

`:debug` shows the source of the last evaluated input, whichever
numbering it was in. `:vars` lists bindings and does not mention
numbers. History is a list of lines and knows nothing of numbering.
`:help` gains `:renumber` with one line: "start the prompt count over,
keeping everything else". Completion of colon commands includes it.

### 6. What this record does not decide

- Showing the numbering in the prompt (`[2.1]`) or a `:renumber`
  message; the prompt going back to `[1]` is the message.
- A way to recall input *n* by number, IPython's `_5` or `In[5]`. It is
  the obvious next thing and it is a separate decision, because it
  gives a number a meaning in source code.
- Numbers in `eval` or `run` diagnostics beyond today's "input 1".

## Acceptance gates

1. **A pipe sees nothing new, and the intended changes are named.**
   Both suites pass with no test edited except these, listed in the
   evidence with the reason each changed: the session gates that set
   `TERM=dumb` to obtain a prompt through a pipe (`tests/http.rs`'s
   `Repl` helper and its users), whose transcripts gain the number and
   the marker; record 0039's pty test that expects a bold `rnx> `, which
   gains the number; and any test that compares `:help`'s full output,
   which gains the `:renumber` line. Nothing else changes, and a change
   anywhere else is a failure of this gate.
2. **The prompt number is the diagnostic's number.** Through the pty
   harness: `[1]` on the first prompt; after an input that fails, the
   next prompt is `[2]` and the failure said "input 1"; after a
   recognised colon command, an abandoned multi-line input, an empty
   line, an input refused by the input cap, and — with the ceiling set
   low — an input refused by the ceiling, the number is unchanged; after
   `:bogus` it advances and the diagnostic cites the number the prompt
   showed; a multi-line input keeps one number; Ctrl-C on a half-typed
   line leaves the number unchanged, and Ctrl-C during a running input
   advances it.
3. **Markers.** `[2] 2` beside a value; nothing at all for `()` and for a
   declaration, as today; no marker on a diagnostic, on `:vars` output,
   on the banner, or on the reset and abandonment messages; source review establishes that the prefix is emitted once before the
   complete rendering, so any subsequent lines would remain untouched
   (the current renderer produces only single-line values); with standard
   input a pipe and an ordinary `TERM`, no marker anywhere; with
   `TERM=dumb` and a pipe, prompt and marker both appear, and so they
   do with `TERM=EMACS`, to hold the predicate to rustyline's.
4. **`:renumber` keeps contents.** After inputs 1 through 4 and
   `:renumber`, the prompt is `[1]`, `:vars` lists the same bindings
   with the same values, `:debug` still shows the last input's source,
   and the history file holds the four inputs and the `:renumber` line;
   the memory figure is reported, not asserted equal. Then, as separate
   scenarios so one cannot mask another: a closure defined at 4 in
   numbering 1 and called at 1 in numbering 2 reports "input 4 of
   numbering 1", and the next admitted error is "input 2"; in a fresh
   numbering with nothing retained, the first admitted error reports
   "input 1" with no numbering named; two `:renumber` commands with no
   input between them leave the prompt at `[1]` and give the next error
   "input 1", so two numberings sharing one storage boundary do not
   confuse the lookup; after inputs in three numberings, an error from
   each names its own numbering except the current one; `:reset` after
   all of that gives `[1]`, no bindings, and diagnostics that name no
   numbering.
5. **The colour of it.** Under record 0039's switches the number is
   styled with the prompt and the marker with the same style, and
   removing the styling sequences gives the plain transcript.
6. **Existing guarantees.** Both suites, the startup measurements and
   the session baseline before and after; `eval` and `run` output
   byte-identical.

## Guardrails and stop conditions

1. The number on the prompt is the number a diagnostic about that input
   cites, always. If any path could show one and cite another, stop.
2. `:renumber` clears no binding, declaration, retained source, history
   entry or input, and renumbers nothing stored. If implementing it
   requires touching the input list's indices or the source map, stop.
3. With standard input a pipe and an ordinary `TERM`, no prompt and no
   marker: the piped transcript is unchanged. If any test outside gate
   1's named list has to change, stop.

## Risks

- **A result prefix changes the unsupported-terminal transcript.**
  Stated, and the tests that see it are named; whether anyone runs rnx
  that way outside the tests is not known, which is why the change is
  stated rather than assumed harmless.
- **"of numbering 1" is a phrase nobody has read before.** It appears
  only after a `:renumber`, only for older inputs, and it is one clause;
  the alternative — a diagnostic citing a number the prompt no longer
  shows — is the thing Codex's brief said to prevent.
- **A person may expect `:renumber` to clear.** `:help` says what it
  keeps; `:reset` is one word away.

## Forward

Recalling an input or a result by its number, if the numbers turn out to
be what people reach for.
