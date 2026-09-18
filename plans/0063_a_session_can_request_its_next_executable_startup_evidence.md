# 0063 gate 3: real startup before session handover

Status: implemented, ready for review on Linux. Gate 2 is accepted at
`145f08c`; this checkpoint enables readiness and replacement only after the real
startup proof. Gates 4–6 remain open.

The exact production source measured here is `145f08c` plus the bench archive
`results/session-startup-0063/source.patch`, including new files. Its SHA-256 is
`8e0a5609b68cf3186c1586316f6c856ba2d1dc2dc079005b637d56d10fcdb50c`.
The evidence/status edit comes afterwards and changes the root native-tree
fingerprint, as usual. Conditions record the checked artifact and product binary
hashes. The fixture sources and instructions are in `probes/session-startup/`.

## Startup ownership

The tool runs the checked artifact in a separate process group, with the caller's
working directory and environment, the explicit leading presentation flags and
`repl` dispatch. A private inherited socket selects the probe; there is no public
probe flag. The artifact seals that descriptor before settings or builders run.

This follows the real session settings path, then constructs the real serving
context and extensions. It creates a session with the same memory-ceiling helper,
samples it, evaluates only `42`, checks the integer result, and closes. It reads
no terminal/history input, evaluates no application entry and installs no source
map. Existing settings warnings and fallback remain. Only after the inner entry
returns, including context and lifecycle disposal, does the probe write its
versioned success response. Ordinary eval still skips session settings.

The tool requires an exact success frame, zero exit and artifact revalidation.
Its five-second deadline covers spawn, builders, eval and cleanup. Nonblocking
output/control reads have bounded work per pass, a 64 KiB tail per stream, an
8 MiB overall output limit and a bounded control frame. Failure diagnostics use
at most the last 8 KiB of each output stream before UTF-8 conversion; the REPL
escapes terminal controls. Readiness-looking stdout cannot satisfy the separate
control channel. The owned probe group is terminated and the child waited on
every exit path; there are no detached reader threads.

After success, the tool revalidates project inputs, lock bytes and artifact,
then supplies the artifact path, its observed metadata stamp and a fresh session
association. The REPL waits for the helper, checks interruption and the stamp,
and only then says `restart is beginning`. The pending replacement is consumed
after the inner entry returns. Normal session close, history handling, editor,
values, context and lifecycle are gone before the final stamp recheck and exec.
The public entry signature is unchanged. Exec keeps the PID and carries the new
association and presentation flags, replacing the old carrier.

The stamp has the same size, mtime, executable-bit, device and inode coverage as
0059. This is not a new full-hash guarantee or an atomic filesystem-to-exec
transaction. After the announced commitment, cleanup or exec failure is terminal.
The full postcommit failure matrix remains gate 5.

## Review F3

Describe now prints `Adding:` and `Already declared:` from the tool's validated
lists, including `(none)`. Scratch ownership is labelled `New scratch project:`.
A mixed request for Polars and PostgreSQL when Polars already exists names only
PostgreSQL as the addition and does not print Polars's cold-build precedent.
Scratch describe/decline creates no state directory.

## Product fixture results

All **17 startup groups** pass. These use actual project commands and generated
executables, with tiny catalogue-shaped adapters for failure injection, not the
Polars engine. The terminal is xterm-256color at 120 columns; all settings,
history, project and cache paths are private fixture paths.

* A version-only call succeeds without invoking a deliberately broken builder;
  real startup invokes it and refuses. It is not readiness.
* A distinctive settings warning proves the selected config ran. Invalid
  settings retain warning/fallback success. A one-byte memory ceiling refuses.
  History bytes remain untouched by the probe, and an invalid application entry
  is never compiled. A builder's child cannot inherit the sealed readiness fd.
* Builder error, panic, abort, block and output flood all refuse. Forged stdout,
  malformed control, genuinely truncated control at EOF and tracked-destructor
  failure also refuse. Probe PIDs are gone at settlement. The final blocked case
  settles **4.998 seconds after the externally observed startup-phase message**;
  resolution and attachment are separately outside that measurement. This is a
  deadline observation, not a performance claim.
* In all nine refusal cases a non-Send tracked future was started through Rune
  select before consent. The old PID's exact poll/drop log at refusal equals its
  pre-consent log, before any subsequent input. The old binding then returns 42
  and that same future completes with 73. No old-runtime turn or drain is hidden
  in preparation. The matrix archives both snapshots and the later quit log.
* Successful handover has one builder call in the probe and one in the
  replacement. The old operation, context and value drop before the next
  same-PID builder. The new prompt starts at input 1, the old binding is missing,
  the extension returns 73, and saved history includes the dependency request.
  A repeated request is a no-op using the replacement's fresh association.
* The mixed and scratch notices prove F3 through actual terminals.

The unchanged **44-group gate 2 preparation matrix** also passes against this
source. Its rerun transcripts and matrix are archived under
`preparation-regression/`, leaving the accepted gate 2 results unchanged. This
retains the inherited-carrier, stamp-refresh, precommit cancellation and
nonterminal-following-input controls. No fixture runtime was left running.

During fixture development an attempted settings print was rightly refused by
the bare settings context; the final fixture uses an unknown-setting marker
instead. Editing tracked inputs during development also correctly invalidated
an in-progress build; the recorded run used frozen inputs. Neither refusal was
weakened to make the fixture pass.

## Regression and remaining scope

Formatting passes in both workspaces. Root suites ran serially to completion:
376 default, 419 test-support, and 438 with test-support/server-runtime/
project-sources, all passing. Tool suites pass 40 each in both configurations
(two existing ignored tests each); strict all-target clippy passes in both.
Root and tool notices are current. No dependency, manifest, lockfile, kernel,
adapter or server package changed.

Root strict clippy is not clean: the 13 production diagnostics match gate 2.
The broader all-target run additionally identifies two diagnostics in unchanged
memory/worker test code. The checks document distinguishes these from new code;
there are no new production diagnostics.

This proves the gate 3 startup and commitment path with fixture adapters. The
real Polars/PostgreSQL journey, the complete postcommit failure matrix and the
matched timing/regression closing gate remain 4–6. Builders may have external
side effects in both probe and replacement; a successful probe cannot guarantee
that a second startup will succeed. Native adapters are trusted, and process
containment does not cover a deliberately escaped descendant. Windows execution
is not claimed.
