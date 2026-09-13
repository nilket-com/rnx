# rnx 0030: answering before building the context

Status: proposed 2026-09-13. The thirtieth record of rnx. `rnx version`
takes as long as `rnx eval 42`, and neither takes long, so nobody noticed.
This record says where the time goes, and moves the two commands that need
none of it in front of it.

## Context

Measured on Linux (`nano`, Intel i7-14700), release build at c908068,
`hyperfine -N` pinned to one core with `taskset`, 100 runs. The whole set of
measurements, with the scripts and the raw exports, is in the evidence file
beside this record.

| command | mean |
| --- | --- |
| a Rust binary that exits at once | 0.56 ms |
| `rnx version` | 4.2 ms |
| `rnx eval 42` | 4.6 ms |

A scratch crate against the same Rune 0.14.2, built as one binary that
stops after a named phase, gives the deltas:

| after | mean | delta |
| --- | --- | --- |
| exit at once | 0.56 ms | the process floor |
| `Context::with_default_modules()` | 3.7 ms | +3.1 ms |
| `context.runtime()` | 3.7 ms | ~0 |
| compiling `pub fn main() { 42 }` | 3.7 ms | ~0 |
| running it | 3.7 ms | ~0 |

Three quarters of `rnx eval 42` is building Rune's default-module context.
Compiling and running a trivial program are below the noise. And `main`
builds that context, then installs `host` and `text` into it, **before it
looks at its arguments**: `version`, `help`, and the refusal of a word that is
not a command each pay for a context they never touch.

`version` is the one a script reads, and a script that asks a dozen tools
their versions notices a dozen 4 ms answers less than it notices one 60 ms
answer, so this is not a complaint anyone has made. It is a cost with no
reason behind it, found while looking for where the time goes, and the fix
is a reordering.

## Decision

### 1. `version` and `help` are answered before a context exists

The dispatch reads its arguments first. If the first word is `version`,
`--version` or `-V`, or `help`, `--help` or `-h`, it answers and returns, and
`Context::with_default_modules()` is never called. Everything else builds the
context exactly as before and dispatches exactly as before.

What the two commands print does not change by a byte: the same two lines
for `version`, the same `USAGE` for `help`, on standard output, exit 0. The
existing gates in `tests/commands.rs` and `tests/release_metadata.rs` say so
and are the ones that hold this.

### 2. The measurement is the evidence, not a test

A test that asserts `rnx version` takes under a millisecond is a test that
fails on a loaded machine. The improvement is recorded in the evidence file,
with the conditions it was measured under, and the gates assert what can be
asserted: the outputs and the exit statuses.

### 3. What this record does not decide

- **The refusal of an unknown word.** It also needs no context, but it is
  reached by elimination after every command has been tried, and moving it
  means the dispatch holding a list of its own commands. That is a separate
  change to a branch record 0024 wrote, and this record does not make it.
- **`eval` and `run`.** They need a context, so reordering gives them
  nothing. Making the context cheaper — fewer default modules, cheaper
  registration, precomputed metadata — is a different question with no
  measurement behind it yet.
- **The in-process number.** An `Instant` around the same phases, in
  process, read 7–9 ms for the context against the 3.1 ms delta above. The
  two were not taken under the same conditions and the cause is not
  established. The process-level deltas are the ones this record rests on,
  because they were taken the same way as every other number in it.

## Acceptance gates

1. **`version` is unchanged.** `rnx version`, `--version` and `-V` print the
   same two lines as before on standard output and exit 0. The gates in
   `tests/release_metadata.rs` pass unchanged.
2. **`help` is unchanged.** `rnx help`, `--help` and `-h` print `USAGE` on
   standard output, nothing on standard error, and exit 0. The gates in
   `tests/commands.rs` pass unchanged.
3. **Both are measured faster, and `eval` is not slower.** `hyperfine -N`,
   pinned, before and after, for `version`, `help`, and `eval 42`, recorded
   in the evidence with versions and conditions. The first two are expected
   near the process floor; the third is expected unchanged.
4. **Nothing regresses.** The gates of records 0002 through 0029 pass.

## Guardrails and stop conditions

1. The context is built once, in one place, for every command that needs
   it. This record moves two branches in front of that place; it does not
   add a second place.
2. If either command's output changes by a byte, stop: that is not this
   record.

## Risks

- **A later command added before the context, that needs one.** The
  comment at the reorder point says what may go there and why. Small, and
  the compiler catches the obvious form of it: there is no `context` in
  scope to use.

## Forward

The unknown-word refusal, if a list of commands is worth holding. The cost
of the context itself, which is the only lever left for `eval` and `run`,
and which needs a profile rather than a reorder — `perf` is refused on the
measuring machine (`perf_event_paranoid=4`), so that is a machine question
before it is a code question.
