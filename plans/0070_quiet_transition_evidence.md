# 0070 gate 1: the tool's quiet transition

Status: passes on Linux, ready for review; third revision after review. The product is this record's impl commit; it
carries both sides of the record, because the tool's capture can only be
exercised by a session that asks for it. This evidence is gate 1's: the
wire, the capture, the log and the failure report. Gate 2 covers the two
spellings at the prompt with the same product and the same probe
(`rnx-bench/probes/quiet-dep/`, results `rnx-bench/results/quiet-dep-0070/`).

The plan was revised while gate 1 was under way, on the user's
clarification of what quiet means: not one line of noise, noise being any
text that is time invariant. Quiet therefore prints nothing on success and,
on failure, the error itself with nothing standard around it. The first
draft of this gate printed a notice, phase lines and a timing; they are
gone.

## What changed

- `tools/project/src/workflow/quiet.rs`: `Capture::begin` starts the drain
  thread first, then saves the tool's stdout and stderr and dups one pipe
  onto both, rolling the descriptors back if that fails. The drain reads
  until the end, whatever the volume, into a rolling tail — the last
  thirty lines, a trailing partial line counted, and at most 16 KiB — and,
  once `attach_log` has opened the log under the project's command lock,
  into the log: the reopen command first, then everything captured before
  the log existed (the scratch repair notice among it, the review's second
  finding), then a prefix up to the document limit. Both streams and every
  child inherit the pipe; `cargo metadata` keeps its own piped stdout as
  parsed data. `finish` restores both descriptors, waits for the drain at
  most five seconds (a writer the tool does not own can keep the pipe open;
  the report then says output was still arriving), and finalizes the log
  through the checked handle only — the marker with the count of bytes not
  written, then on failure the phase, the error and the recovery commands
  (the review's first finding: the previous draft reopened the path). On
  success it returns nothing; on failure `<phase>: <the underlying error>`,
  the actual tail, and the log's path, or the reason no log was written. The
  log is opened privately, no-follow and non-blocking, refused before
  truncation when the path is a symlink, a FIFO, a device or a file with
  another link.
- `transition.rs`: field 7 of the describe request is the mode: absent or
  `verbose` is the prompted, verbose behaviour; `quiet` is quiet; anything
  else is a protocol error. The capability answer stays exactly `1`,
  because an older session requires it; a newer session learns of an older
  tool from that tool's refusal of the field, which happens in `describe`
  before anything is written. The phase notices go to the log in quiet
  mode and to stderr otherwise.
- `src/dep_transition.rs` and `repl.rs`: `:dep` sends the field, prints no
  notice, answers the consent frame itself, prints nothing on success,
  execs the replacement with `--no-splash` and without the reopen notice,
  and prints a failure bare; `:depv` sends no field and keeps the notice,
  the prompt, "restart is beginning", the reopen notice and the output;
  both are in `:help`, where the first build's cost and the restart are
  documented; an old tool's refusal is translated to "predates quiet
  preparation; update the installed rnx, or use :depv".

## What the probe proves

Stock installs of the product and of 0069's product (capability 1, no
`:depv`) from a private origin by `cargo install --git` (fixture commits
point `repository` at the origin, as 0067's journey did), and a runner-only
session build of each for the cross-version directions. Every session runs
in a private HOME.

**Quiet, at a real prompt** (`journey.json`), in a session started *with*
its banner: `:dep polars` in a fresh HOME prints nothing between the echo
and the replacement's `[1] >` — zero lines, no banner — in 120 s; the new
session has Polars. The scratch project's `.rnx/dep.log` exists, mode
0600, its first line is the reopen command, and it holds the phase
notices and the Cargo output (308 `Compiling` lines, 18 KB). A fresh
session's `:dep polars` attaches in 7.9 s printing nothing; `:dep postgres`
in 45 s printing nothing; `:dep polars` once more, already installed,
prints nothing. The quiet and the verbose preparation of the same
declaration lock to the same identity: capture is not an input.

**Verbose** (`:depv polars`): the eleven-line notice, `Continue? [y/N]`,
then the phase notices and Cargo's own lines; no `dep.log`.

**A quiet failure** (`--offline` in a HOME with no Git checkout): the first
line is `resolve: Cargo Git acquisition failed: "cargo" failed: exit status:
101`, then the actual tail — the tool's `added polars` lines, the phase
notices, Cargo's `failed to get rnx-polars … offline` — then the log's
path; no "dependency preparation refused", no "last lines of the output",
no "retry with" (that is in the log); the old session continues.

**Ctrl-C during a quiet build**: `^Cinterrupted; old session unchanged`,
and the session continues.

**The two peer directions** (`compatibility.json`): a new runner-only
session with the old tool as `RNX_PROJECT_TOOL`: `:dep polars` is refused
by name, nothing is written under the private state directory, and `:depv
polars` is prompted with the notice; the old runner-only session with the
new tool: `:dep polars` is prompted and verbose.

**The repair notice** (the review's second finding): the failure home's
state directory is group-writable, so the legacy scratch repair runs before
a project exists; its notice is in the log after the header and in the
failure's tail, and nothing reached the terminal on its own. After Ctrl-C
no process of the build remains.

**Checks** (`checks.json`): root and tool formatting; tool strict clippy in
both configurations; tool suites 81 and 82 (the review's third finding, as
real controls on the capture: a `yes` flood on both streams past pipe
capacity and the log limit followed by a distinct final error — the tail
ends with the error, the log holds the header, the notice, the prefix and a
byte-counted marker, nothing reaches the terminal; a child's parsed stdout
intact while its stderr is captured; an injected mid-stream write failure
stops the log, keeps the tail and reports no usable path; finalization
after the path was replaced by a symlink leaves the sentinel untouched and
the report no longer advertises that path (a path is printed only when it
still names the file the tool wrote, by device and inode); cancellation
with full pipes reaps the group — asserted by `ESRCH` on the group — and
reports within the bound with the streams restored; a lingering writer
cannot hang finalization, and the control reaps its own group, `sleep`
included, and proves it gone; the
tail's bounds including a partial trailing line; the private log and its
planted-path refusals; the mode field); root suites 390 and 435, on a
pristine checkout (the fixture checkout's rewritten `repository` fails the
packaged-manifest test, as it should).

## Qualifications and next gate

The mid-stream write failure is injected at the writer, not produced by
a full disk; the marker's byte count is the drain's own accounting. The
plan's decision 5 named a capability bump; the product keeps version 1
for the reason above. The "interrupted; old session unchanged" text on
Ctrl-C is the existing interrupt error, printed because it is an error.
No cost claim beyond the single runs shown.
