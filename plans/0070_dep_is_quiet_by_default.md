# rnx 0070: `:dep` is quiet by default, `:depv` says everything

Status: accepted for gate 1 after two drafts; revised 2026-09-21 on the
user's clarification that quiet prints nothing on success (decision 2 and
the journey). Gate 1 accepted, see
[the quiet transition evidence](0070_quiet_transition_evidence.md); gate 2
accepted, see [the prompt evidence](0070_prompt_evidence.md); gate 3
passes on nano, see
[the costs and regression evidence](0070_costs_and_regression_evidence.md).
The record closes on Linux when the user reports the slim screens. 0069
awaits the user's slim confirmation and is otherwise complete; this record
builds on its transition path.

## Problem and user journey

On slim, `:dep polars` asks "Continue? [y/N]" under a nine-line notice, then
prints Cargo's entire output — two hundred lines of `Compiling …` for a first
Polars build, `Locking 385 packages` and its `Adding …` lines for every
resolution — before the session restarts. The user wants the ordinary case to
be one keystroke and a few lines: type `:dep polars`, see what is happening
and how long it took, and land at the new prompt. The full account should
remain one command away for the case where something goes wrong or the user
wants to watch: `:depv polars`.

The intended result, in the user's words: not one line of noise, where
noise is any text that is time invariant — anything that would print the
same on every run:

```
[1] > :dep polars
[1] > let x = polars::read_csv("sales.csv", …)?;
```

Quiet preparation prints nothing on success: no notice, no phases, no
timing, no "restart is beginning", no reopen command, and the replacement
session starts without its banner. What varies from run to run is a
failure, and a failure prints where it happened, the actual last lines of
Cargo's output and the log's path, or the tool's own error when Cargo never
ran. The reopen command for a scratch session is written as the first line
of the log and stays available through `rnx project session`. "restart is
beginning" stays where 0063 put it for `:depv` — printed when preparation
has succeeded and before the irreversible cleanup begins — and the reopen
command is printed by the initialized replacement in `:depv` only; cleanup
and exec failures keep their names in both modes.

## Current boundaries, checked in source

`repl.rs` handles `:dep` by calling `dep_transition::prepare(&input,
consent)` with a closure that reads "Continue? [y/N]" through the editor;
`prepare` prints the tool's notice (`description[&2]`) before calling it, and
refuses without a terminal on stdin and stdout because of that prompt. The
helper is the installed `rnx` running `project`'s transition protocol over
a socket pair; it prints `dependency phase: …` lines to its own stderr, and
`commands::run_inner` gives Cargo the helper's inherited stderr
(`Stdio::inherit()`) and routes Cargo's stdout there too. Nothing captures
Cargo's output today; the transition wire (kind 1) carries names,
association, executable path, installed names, offline flag and session
flags, so a new field is the established way to pass a mode. 0063's
interrupt handling — Ctrl-C during preparation kills the builder's process
group and the session continues — is unchanged by anything here.

## Decisions

### 1. Two spellings, one protocol

`:dep NAME…` is quiet; `:depv NAME…` is verbose. Both accept `--offline`.
Quiet does not prompt and does not explain: the request is the consent,
and what a first build costs and that the session restarts are documented
under `:help dep`, not printed. Verbose keeps today's notice, prompt and
full output exactly, so 0063's and 0067's journeys are `:depv`'s journeys.
`:help` lists both; history records what was typed.

### 2. Quiet prints nothing on success; capture is a drained, rolling tail

The session passes the mode to the helper in the transition request. In
quiet mode the tool gives Cargo *build* commands piped stdout and stderr
and drains both concurrently and completely, whatever their volume: a
noisy child is never blocked on a full pipe, and Ctrl-C still kills the
process group and reaps it as 0063 requires. `commands::run_inner`'s
bounded capture, which stops at the limit and fails, is not reused for
this; it stays what it is for the commands that parse their output. `cargo
metadata` keeps its stdout as returned data under its existing limit —
lock and verification parse it — and only its stderr is diagnostic.

Two sinks receive the drained bytes. The log, `<project>/.rnx/dep.log`,
receives the reopen command as its first line, then everything captured
before the log existed, then a prefix of Cargo's output of at most the
document limit, then a marker line naming how many bytes were not written,
and draining continues without writing. The log is written and finalized
through the handle the tool checked when it opened it; nothing reopens the
path. A rolling
tail in memory keeps the last thirty lines *and* at most 16 KiB, whichever
bound is tighter, so a single enormous line cannot grow it; the tail is
what a failure prints, terminal-escaped, so it is the actual end of the
output even when the log was cut long before. Streams are interleaved in
arrival order in both sinks with no claim of exact ordering between them.
A failure with an empty log or one where Cargo never started prints the
tool's own error, which is never replaced by a log path.

On the terminal in quiet mode, on success: nothing — not the tool's
notices (the scratch-directory repair notice included; it goes to the
log), not the session's, and the replacement starts with `--no-splash`.
Cargo's own progress bars are not shown; they need a terminal it no longer
has.

### 3. The terminal requirement stays

Quiet mode removes the prompt, not the restart: the replacement executable
takes over the terminal, so `:dep` still refuses when stdin or stdout is not
a terminal, with the existing message. A piped session that wants
dependencies uses the project commands, as today.

### 4. The log is owned like every other managed `.rnx` file

The log is created only when a preparation is about to run Cargo — not by
describe, a refusal or a no-op, which invent no state — under the project's
existing command lock, so two preparations of one project cannot interleave
into one file; a second attempt replaces the previous log, and the latest
attempt is retained until the next replaces it, across relocks, since quiet
preparation itself relocks. Creation is private and no-follow in the managed
`.rnx` directory: an existing path that is a symlink, a FIFO, a device or a
file with another link is refused before truncation, so a planted path
cannot make the tool write elsewhere. Open or write failure (a full disk
among them) does not fail the preparation and is not reported as a usable
log: the tail still prints, the message says the log could not be written
and why, and the underlying preparation error and the old session are
preserved. Logging changes no source inventory, assembly key, receipt
binding or identity: `.rnx/` is already outside the inventory, and gate 1
verifies that a project prepared with and without capture locks to the same
key. Ordinary `project` commands keep their current output.

### 5. Wire compatibility is explicit; a missing mode is never consent

The transition request (kind 1) gains a mode field. Readers validate exact
field sets, so the two peer directions are decided, not assumed. A new
session with an older management executable: the tool refuses the request
before writing anything, and the session prints that the installed `rnx`
predates quiet preparation and names the update. An older retained
generated session with the new tool: the field is absent, and the tool
proceeds with today's prompted, verbose behaviour, which that session still
drives. An unknown mode value is a protocol error. In no direction does a
missing or unknown mode become authorization for quiet preparation: quiet
is granted only by the explicit mode that the new session sends, and the
consent frame, describe and revalidation stay in the protocol even when the
session answers the frame itself.

### 6. Scope

No change to what is prepared, resolved, built, shared or verified; no
change to the identity, the receipts, the startup check or the exec.
`:depv` is not a `--verbose` flag on `:dep`, because the record's premise is
that the ordinary spelling must be the quiet one and a flag is what people
forget.

## Gates

### Gate 1 — the tool's quiet transition

Add the mode to the wire request, the concurrent drain, the log with its
prefix bound and marker, the rolling tail with both bounds, the phase and
outcome lines, the failure report, and the log's ownership rules. Tests:
a captured build's log holds Cargo's lines and the terminal holds none;
output beyond the limit followed by a distinct final error prints that
error in the tail while the log ends with the marker; one line larger than
the tail's byte bound; interleaved streams; `cargo metadata` still parsed
under capture; Ctrl-C with full pipes kills and reaps the group; a symlink,
a FIFO and a hard-linked file at the log path refuse before truncation; a
write failure keeps the tail and the preparation error and reports no
path; the same project locks to the same key with and without capture; the
old-session/new-tool and new-session/old-tool directions behave as decision
5 says; verbose mode's bytes on the terminal are unchanged against the 0067
journey fixture.

### Gate 2 — the session's two spellings

`:dep` and `:depv` in `repl.rs` and `:help`; no notice, no prompt and no
output in quiet mode; "restart is beginning" and the reopen command in
`:depv` only. At a real prompt: `:dep polars` first build, then a fresh
session's `:dep polars` attach, then `:dep postgres`, each printing nothing
between the echo and the replacement's first prompt (counted: zero lines);
a cleanup or exec failure still named;
`:depv polars` printing the notice, waiting for the prompt, and showing
Cargo's lines; a build failure in quiet mode (an unreachable origin) showing
the tail and the path; Ctrl-C during a quiet build returning to the prompt
as 0063 requires. The 0067 one-install journey rerun with `:depv` where it
typed `:dep`, unchanged.

### Gate 3 — costs, regression and slim

The stock 5% startup gate; the three root configurations, tool and adapter
suites, strict clippy and notices; the transition's wall time with and
without capture (the log write must not cost seconds); documentation of the
two spellings and the log. The user runs `:dep polars` and `:dep postgres`
on slim and reports what the screen showed.
