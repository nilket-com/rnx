# rnx 0024: the default command is a session

Status: proposed 2026-09-09. The twenty-fourth record of rnx. Typing `rnx`
prints the spike's self-check, which is a page of retention semantics,
process outcomes and timings. That was the right default when the only
question was whether any of it worked. It is the wrong one now, and this
record makes the bare command a session.

## Context

The operator, using it as a calculator:

```
❯ rnx
retained closure/function, new function, old struct field/type/method: [11,10,20,7,true,7]
after compile failure: 11
...
100 incremental inputs (compile + execute): 16.252408ms
❯ rnx repl
rnx> 9*9
81
```

Two things are wrong with that. The first is the obvious one: the thing a
person most often wants is one word further away than the thing they almost
never want. The second is quieter — **any** unrecognised argument reaches the
self-check, because the dispatch tries `eval`, `repl` and `run` and then falls
through. `rnx frobnicate` prints a page of diagnostics and exits 0, which is
neither a run nor an error.

The self-check itself is worth keeping. It is not a demonstration: it asserts
what it prints, and it calls `host::process_checks` and `session::checks`,
which exercise a child's deadline, its capture cap, its cancellation, and the
session's retention rules. That is a smoke test with a printout, and a
release-time question worth being able to ask.

## Decision

### 1. No arguments starts a session

`rnx` is `rnx repl`. The banner it already prints says what it is and that
`:help` lists the rest.

A session works when standard input is not a terminal — measured:
`echo '9*9' | rnx repl` answers `81` — so the default needs no condition
attached to it. What a pipe gets is a session reading a pipe, which is what
was asked for.

### 2. The self-check is asked for by name

`rnx selfcheck` runs what the bare command used to. The name says what it is:
it checks, it does not demonstrate, and it fails rather than reports if an
invariant it asserts is broken.

### 3. An unrecognised command is an error, not a fallthrough

`rnx frobnicate` says so and exits nonzero, naming the commands. Falling
through to anything at all is how a typo became a page of output.

`rnx help`, `rnx --help` and `rnx -h` print the same list on standard output
and exit 0, because an explicit question deserves an answer rather than an
error.

**The word it did not recognise is escaped.** It is text from outside, and
record 0019's rule is that everything rnx prints on its own behalf goes
through the terminal-safe formatter. The first draft of this cut printed it
raw, so `rnx $'bad\033[2J'` cleared the terminal on its way to saying it did
not recognise the word — the contract was three records old and a new message
walked straight past it.

### 4. What this record does not decide

The self-check's contents, which record 0001's gates own. Any argument
parsing beyond the first word: flags stay where their records put them,
before a script's path.

## Acceptance gates

1. **`rnx` is a session.** `echo '9*9' | rnx` prints `81`, and the banner
   before it. The self-check's first line appears nowhere in the output.
2. **`rnx selfcheck` is the old default.** It prints what the bare command
   printed and exits 0.
3. **An unrecognised command is refused, and cannot repaint the terminal.**
   `rnx frobnicate` exits 2, names the commands, and prints none of the
   self-check. A word containing an escape or a carriage return is shown
   escaped — visibly, not dropped — with nothing on standard output and the
   same exit 2. Verified to fail against the draft that printed it raw.
4. **Help is not an error.** `rnx help`, `rnx --help` and `rnx -h` each print
   the commands on standard output and exit 0.
5. **The three named commands are untouched.** `run`, `eval` and `repl` behave
   as their records say, flags and all, and the four ports still match their
   originals.
6. **Nothing regresses.** The gates of records 0002 through 0023 pass.

## Guardrails and stop conditions

1. One dispatch, and every branch of it deliberate. If a word is not a
   command, the answer is that it is not a command.
2. The self-check keeps asserting. A smoke test that only prints is a
   printout.

## Risks

- **Anything invoking bare `rnx` for the self-check breaks.** Nothing in
  either repository does — checked rather than assumed — and the release has
  not happened, so there is no installed base to consider.

## Forward

Record 0001's release gates: the process ladder on macOS and Windows, and
`cargo install --locked` on all three, which is what the two machines were
ordered for. Provenance review before a license file, as record 0001 requires.
