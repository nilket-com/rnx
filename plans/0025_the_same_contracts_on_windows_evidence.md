# rnx 0025 evidence: the same contracts on Windows

Measured on the Dell on 2026-09-10, on native `x86_64-pc-windows-msvc` — not
WSL. This records the first Windows **behaviour** ever observed for rnx.
Record 0025 said nothing Windows-behavioural had been; that is no longer
true, and what follows is what the machine said rather than what the record
expected.

Nothing here completes a gate that needs a fixture which does not yet exist.

**Current status.** Linux reviewed `96e3b88`: **238 default / 243 with
test-support**, all passing, formatting clean, Windows cross-check successful
with an unused-import warning, and one shared-fixture clippy warning beyond
the baseline. The closeout below addresses its five remaining Windows gates
and both warnings. Earlier sections are chronological measurements; their
lists of missing fixtures are historical, not the current checklist.

## Windows closeout after review of `96e3b88`

Measured natively on 2026-09-10. All five questions returned by Linux now have
passing Windows fixtures:

| Gate | Native observation |
| --- | --- |
| 4 | Before assignment, the child is outside the exact rnx job and its threads have positive prior suspend counts. After release, its first helper action spawns a descendant; both processes belong to that exact job and are terminated at the deadline. |
| 8 | A headless ConPTY console is refused with `it is a terminal; redirect a file or pipe into it`. The formerly Unix-only portable stdin tests now run on Windows: exact pipe bytes, files, empty streams, single-read refusal, UTF-8 refusal, and the read limit. |
| 15 | The fixture opens the still-live suspended child before releasing the injected assignment failure. rnx reports the original failure and exits 1; Windows signals that retained process handle as terminated. No child output is needed to find it. |
| 16 | `rnx run` with `NUL` redirected to stdin reports empty text, exits 0, and emits no error. |
| 18 | Two separate interactive REPL processes share a scratch history location. The second sends only Up, Up, Enter to recall past `:quit`, then evaluates the previous expression to `104729`. It never receives that expression or result as typed input. Both default LOCALAPPDATA and explicit RNX_HISTORY cases pass. |

Gate 4 and Gate 15 use `tests/job_assignment_windows.rs`. The new
`RNX_TEST_BEFORE_JOB_ASSIGNMENT` hook is compiled only with `test-support`;
it publishes the child PID and job handle atomically and waits at most 15
seconds for the observer's release. The observer duplicates the exact job
handle and retains process handles, so an enclosing job or reused PID cannot
supply the membership or termination result. Keeping the duplicate job handle
open also prevents kill-on-close at rnx exit from supplying the deadline's
termination evidence. Every test teardown explicitly ends its owned objects.

The console helper in `tests/harness/console.rs` creates no visible window.
It drains output continuously through console shutdown and collects the
reader thread. Explicit null startup handles prevent the test runner's
redirected handles from overriding ConPTY, following
[Microsoft Terminal's explanation](https://github.com/microsoft/terminal/discussions/15814).
The fixture sets `TERM=xterm-256color`: this runner exports `TERM=dumb`, which
otherwise selects rustyline's noninteractive fallback and cannot test recall.

Regression controls, each restored before the acceptance runs:

| Mutation | Required failure observed |
| --- | --- |
| Remove `CREATE_SUSPENDED` | Gate 4 reports prior suspend counts `[0, 0, 0, 0, 0]`: the child could execute before assignment. |
| Remove `abandon`'s kill and reap | Gate 15 reports `WAIT_TIMEOUT` on the retained suspended-child handle; fixture teardown kills it. |
| Replace the console test with `GetFileType == FILE_TYPE_CHAR` | Gate 16 reports the terminal refusal and exit 1 for NUL. |
| Remove `editor.load_history` | Gate 18 times out waiting for the second session's evaluated result; merely writing the history file does not pass it. |

Full acceptance uses the explicit alternate launch required by Gate 10:

```powershell
./scripts/test-windows.ps1
./scripts/test-windows.ps1 -TestSupport
```

The script builds with Cargo's locked dependency graph, locates every test
executable from JSON artifacts, and runs all of them directly, outside
Cargo's restrictive job. **220 tests pass by default; 239 pass with
test-support; zero failures**, across 28 executables per configuration.
Gate 10's two tests pass in both runs. There are no acceptance skips, and a
runner unable to draw its outside control still fails the script.

Additional validation:

- `cargo check --locked --all-targets`, with and without `test-support`,
  passes without warnings for Windows, Linux, and both macOS targets.
- Installation with `cargo install --locked --path . --root
  ../tmp/rnx-closeout-install` succeeds; that installed binary's `selfcheck`
  exits 0. The isolated root keeps this validation out of the user's tools.
- The Unix-only unused `Duration` import, the shared fixture type-complexity
  warning, and two warnings in the native capture-holder fixture are fixed.
- `cargo clippy --locked --all-targets --features test-support` succeeds at
  the eleven pre-existing warnings, with no fixture warnings. `cargo fmt
  --check` and `git diff --check` pass. The three new gate test binaries
  also pass after adding scratch cleanup on history-test failures.

Local logs are `../tmp/rnx-closeout-default.log`,
`../tmp/rnx-closeout-support.log`, the `rnx-closeout-check-*.log` files,
`rnx-closeout-install.log`, `rnx-closeout-installed-selfcheck.log`, and the
four `rnx-control-*.log` files. They are execution artifacts, not shipped files.

**Linux review boundary.** The 238/243 native Linux result is the supplied
review of `96e3b88`, not execution of this amended tree. All four target
checks above are fresh. Native Linux rerunning remains part of the review
handoff: WSL enumeration here fails with `Wsl/EnumerateDistros/Service/
E_ACCESSDENIED`, so a type-check is not being reported as a Linux test run.

## Native rerun after the Linux handoff — `d7e649a`

Measured on 2026-09-10 with native Windows Rust/Cargo 1.98.1, from a clean
tree at `d7e649a`. This rerun answers the request to exercise the repaired
harnesses on Windows; it does not close the remaining fixture gaps.

- `cargo test --locked --features test-support --test capture_bound_windows
  --test console_interrupt --test history --test job_breakaway --no-fail-fast`:
  capture 1/1, console interrupts 4/4, history 3/3 pass. The breakaway
  timeout control passes; the attribution gate fails because Cargo's job
  forbids the outside control's breakaway (`Some(5)`).
- Direct execution of the compiled `job_breakaway` test binary: **2/2 pass**.
  Section G's distinction between launch paths still holds after the repairs.
- `cargo test --locked --no-fail-fast`: **180 passed, 34 failed**.
- The same full suite with `--features test-support`: **185 passed, 39 failed**.
- `cargo fmt --check` and `cargo run --locked -- selfcheck` both succeed.

The full-suite counts are not directly comparable with the first run:
this execution environment cannot find `sh`, `cat`, or `sleep`. Many fixtures
fail before exercising their intended contract. In particular, the new
`the_harness_never_reports_a_child_that_would_not_end_as_ended` test launches
`sleep` unconditionally and fails at spawn; its comment promises a different
Windows child, but the implementation does not provide one yet. That part
of the handoff has not been validated natively. The next section
closes it.

The two deep-value tests still exit with `-1073741571`, and the terminal
safety fixture still fails creating its filename with Windows error 123.
History's three passing tests establish writing and path selection, not
recall in a second interactive session; gate 18 remains partly unanswered.

Local full logs for this rerun are in `../tmp/rnx-windows-default.log`,
`../tmp/rnx-windows-test-support.log`, and `../tmp/rnx-windows-selfcheck.log`.
These are local artifacts, not files shipped with the repository.

## The self-test's Windows child — `d7e649a` plus one change

The rerun above named one gap it could close by itself. This closes it.
`SLEEPER` in `tests/child_input.rs` was a number of seconds handed to
`sleep`; it is now the program **and** its arguments, chosen per platform.

| | Child | Stay |
| --- | --- | --- |
| Unix | `sleep 97` | 97 seconds |
| Windows | `ping -n 98 127.0.0.1` | 97 seconds — `ping` waits a second between echoes |

`ping` is the substitution section F already made for the self-check's
deadline probe, and for the same reason: nothing about waiting needs a shell,
and this test's whole point is that no shell stands between the kill and the
sleeper. `timeout /t 97` was rejected rather than untried — it refuses a
redirected standard input, and this child is given `Stdio::null()` on all
three streams.

A spawn that fails now names the program it could not start. The first
Windows run of this test reported `NotFound` beside a line number, which says
a program is missing without saying which one.

Measured natively, on this tree:

- `cargo test --locked --test child_input the_harness_ --no-fail-fast`:
  `the_harness_never_reports_a_child_that_would_not_end_as_ended` **passes**.
  The reap reports that the child did not end and the kill reports that
  nothing survived — the two halves the test exists to separate — and the
  bounded pair returns well inside five seconds.
- `cargo test --locked --no-fail-fast`: **181 passed, 33 failed**, against 180
  and 34 above. One test moved and nothing else did.
- `cargo fmt --check` succeeds.

**The Unix arm is not compiled here.** Only `x86_64-pc-windows-msvc` is
installed, so `#[cfg(unix)]` is dead text on this machine. It was checked by
compiling the constant and its uses on their own; Linux has to say whether the
test still passes there.

**Two harness self-tests beside it still ask for Unix tools**, and this change
does not touch them:

- `the_harness_reports_a_stream_it_could_not_read` spawns `sh -c "sleep 1"`
  and fails at the spawn.
- `the_harness_remembers_a_run_that_outstayed_its_bound` runs
  `host::process("sleep", ["1"], 20000)` inside its script. The spawn fails,
  the run returns long before its 200 ms bound, and the `should_panic` never
  fires: it fails by **not** hanging, which is the more misleading of the two
  shapes.

The second cannot simply take `SLEEPER`. Its runner is killed by the test's
own bound, and a killed rnx runs no cleanup, so a ninety-seven second child
would be left behind for ninety-seven seconds on every Unix run, where
`sleep 1` is left behind for under one. It needs a **brief** portable child,
which is a second decision rather than this one repeated.

`child_input.rs` passes 2 of 12 on Windows, from 1.

## The other two harness self-tests use native children

The next change closes the two self-test gaps listed above. The unread-stream
test now spawns `SLEEPER` directly, reads with its 200 ms bound, then kills
and collects that same child before asserting the read failed. There is no
shell between the harness and the child holding the stream.

The run-timeout test uses a separate, brief child: `sleep 1` on Unix and
`ping -n 2 127.0.0.1` on Windows. It still runs through `host::process`,
still outlasts the harness's 200 ms limit, and still must panic with
`it hung` after cleanup. Using the long sleeper here would leave an orphan
for ninety-seven seconds on Unix when the harness kills rnx. The brief
child keeps the original roughly one-second lifetime instead.

`cargo test --locked --test child_input the_harness_` now passes **all four
harness self-tests** natively on Windows. No production code changes, and
no fixture is skipped. The Unix arms still require execution on Linux.

Full reruns on this tree: **183 passed, 31 failed** by default and **188
passed, 36 failed** with `test-support`; both commands use `--locked
--no-fail-fast` and exit 101. All four harness self-tests pass in both
configurations. `cargo fmt --check` and `git diff --check` pass. The remaining
suite failures are not closed by these two fixture changes. Local logs are
`../tmp/rnx-windows-harness-default.log` and
`../tmp/rnx-windows-harness-support.log`.

## The deep-value depth is the platform's, not the fixture's

The rerun's two `STATUS_STACK_OVERFLOW` failures are closed, and in the way
category C already said they should be: what failed was the constant the
acceptance cases chose, not record 0019's guarantee.

`tests/eval_outcome.rs` and `tests/json.rs` each wrote `nested(32_768)`
inline. Both now read `FAR_PAST_THE_BOUND`, a constant per platform:

| | Depth used | The platform's ceiling |
| --- | --- | --- |
| Unix | 32,768 | between 49,152 and 65,536 (record 0019) |
| Windows | 2,048 | between 4,096 and 4,608, measured here |

2,048 is eight times the 256 bound under test and less than half the lowest
depth measured to fail — a wider margin than 32,768 has on Linux.

**Category C's table is refined by two measurements it did not make.** Both
were taken against `target/debug/rnx.exe`, which is what the tests run, rather
than the installed release binary section C used.

1. The ceiling belongs to the **entry point**, not to the value. `rnx eval`
   renders, refuses and drops a 32,768-deep value here and exits 1. The same
   value through `rnx run`, returned from a script's `main`, does not: 4,096
   exits 1 and 4,608 is `STATUS_STACK_OVERFLOW`. Both fixtures use `run`.
   Consistent with section C's marker experiment, the recursion that overflows
   is the same drop either way and what differs is how much stack is left when
   it starts — but that is a reading, not a measurement. Nothing here counts
   the frames between the two paths.
2. The shape matters too, by less. `json.rs`'s script never renders the value,
   and it clears 4,096 where the rendering shape does not. One depth is used
   for both so that a single answer covers both shapes.

**Raising the stack rnx runs a script on was not done here.** Section C names
the 1 MB main thread against Linux's 8 MB as the obvious and untested
candidate for the ratio, and running a script on a thread whose stack rnx
chooses would move the ceiling on every platform at once. That is a decision
for the record, like D2's history location, and not something to improvise
inside a fixture repair. The measurement that would support it — why the two
entry points differ — has not been made either.

Measured natively, on this tree:

- `a_deep_value_is_refused_with_a_status_and_never_a_signal` and
  `a_refusal_is_catchable_and_the_script_carries_on` both **pass**, with the
  refusal text and the exit status the Linux runs assert.
- `cargo test --locked --no-fail-fast`: **185 passed, 29 failed**.
- With `--features test-support`: **190 passed, 34 failed**.
- `cargo fmt --check` succeeds.

`json.rs` passes whole, and `eval_outcome.rs` fails only on `false`. The 29
default failures group as **27 category A** — `process_bytes.rs` 9,
`child_input.rs` 8, `capture_bound.rs` 7, `exit_status.rs` 2 and
`eval_outcome.rs` 1 — plus the category B terminal safety fixture and gate
10's job attribution, which passes when its test binary is run directly. The
missing programs behind the 27 are `sh` 6, `cat` 6, and one each of `echo`,
`false` and `head`.

Section A's warning still stands over the seven: `capture_bound.rs` mixes
cases decision 5 rules unreachable with capture-flag contracts Windows must
still answer. Those are authoring work, not substitution.

## Native children for seventeen category A failures

`tests/harness/commands.rs` now supplies named children for the process
fixtures. Rune templates name the behaviour they need, and the test helper
expands that name into the executable and argument array. The host function,
input, deadline, and result assertions stay in the test. No production code
changes and no tests are skipped.

| Behaviour | Unix | Windows |
| --- | --- | --- |
| Copy a file to stdout or stderr | `cat`, with `sh` for stream redirection | PowerShell using `FileStream.CopyTo` and raw console streams |
| Copy stdin through EOF | `cat` | Raw input stream copied to raw output stream |
| Return one line and stop reading | `head -1` | Read and write bytes through the first LF, then exit |
| Emit zero bytes up to the requested count | `head -c` on `/dev/zero` | Write a zero-initialized byte array to the raw output stream |
| Emit one line | `echo` | UTF-8 bytes with an explicit LF, without a BOM |
| Choose an exit code | `true`, `false`, or `sh -c 'exit 7'` | `cmd.exe /D /C 'exit N'` |
| Stay past a short deadline | `sleep` | `ping` to loopback |
| Fill both reply pipes, then count input through EOF | `head`, `wc` | Write 200,000 raw bytes to each reply stream, then read and count all input |

PowerShell is the explicitly requested fixture child, launched with
`-NoProfile -NonInteractive` and terminating errors. It does not wrap another
child or use its text pipeline for the byte fixtures. The existing tests
verify every byte from 0 through 255, malformed UTF-8 on each stream,
truncation through a multibyte character, exact LF endings, and the
1,050,000-byte input count under pressure.

The two exit-status tests need a failing output stream rather than a child
utility. They retain `/dev/full` on Unix. On Windows they open a temporary
file read-only, verify that writing to that handle fails, and pass the handle
as rnx's standard output. Both tests still assert the write diagnostic and
the required status (nonzero for a requested success, exactly 2 for a
previously chosen failure).

The proposed twenty-case substitution had three cases that need more than a
replacement executable: `a_deadline_ends_the_call_while_delivery_is_blocked`,
`a_cancellation_ends_the_call_while_delivery_is_blocked`, and
`the_delivery_thread_is_gone_before_the_call_returns`. They depend on an
escaped descendant, Unix signal delivery, or `/proc` thread counts. Those
three remain failing and need native contract fixtures, alongside the
capture-bound work, rather than a passing substitution that loses their
premise.

Targeted Windows run: `process_bytes.rs` **10/10**, `exit_status.rs` **12/12**,
`eval_outcome.rs` **16/16**, and `child_input.rs` **9/12**. These are the
seventeen formerly failing tests passing with the same assertions. Unix
execution remains to be validated on Linux.

Full reruns with `--locked --no-fail-fast`: **202 passed, 12 failed** by
default and **207 passed, 17 failed** with `test-support`. Both exit 101;
`cargo fmt --check` and `git diff --check` pass. Default's twelve are the
seven capture-bound cases, the three child-input cases named above, the
terminal-safety filename, and Gate 10's blocked outside control under Cargo.
With `test-support`, two more capture-bound cases and the three
delivery-failure cases remain. Local logs are `../tmp/rnx-children-default.log`
and `../tmp/rnx-children-support.log`.

## The sixth surface is a refusal on Windows — category B closed

Section B said gate 9 should record **the reason** a path carrying `U+001B`
cannot exist on Windows, rather than skip the surface or claim it passes.
`nothing_rnx_prints_can_move_a_cursor` now does. Surfaces 1 to 5 are unchanged
and run everywhere; surface 6 splits:

- On Unix, as before: a script under a directory whose name carries an escape,
  and the diagnostic scanned for one.
- On Windows the refusal **is** the assertion. The directory is attempted and
  the error read rather than unwrapped, and the gate asserts
  `raw_os_error() == Some(123)`, which is `ERROR_INVALID_NAME`.

Written that way the gate fails the day Windows accepts such a name, which is
what makes it a record of the platform's answer rather than a skip. It does not
go through `run_in`: that unwraps, and a gate whose subject is a refusal has to
read it rather than die of it. Anything the call somehow creates is removed.

`Ran`'s `path` field is read only by the Unix arm, so it carries
`#[cfg_attr(windows, allow(dead_code))]` rather than letting the Windows build
warn. `terminal_safety.rs` is 3 of 3.

## Category A's remainder, separated

Section A said `capture_bound.rs` mixes cases decision 5 rules unreachable with
capture-flag contracts Windows must still answer, and that separating them is
the first authoring task. Two of the seven were the second kind and needed
nothing but the substitution the other files already had.

`reported` and `reported_with` took a **shell line** and passed it through
`argv` for the script to hand to `sh -c`. They now take a program and its
arguments, written the way `tests/harness/commands.rs` writes them, and the
Rune call is built around it: a fixture that needs no shell no longer gets one.
Two children were added there — `zeros_on_both`, and `shell`, which names the
Unix-shell cases as such instead of leaving `sh` spelled out at each call site.
`commands.rs` is now shared by four test binaries, so it takes the same
`#![allow(dead_code)]` and the same reason `tests/harness/mod.rs` carries.

Passing on Windows now:

- `a_child_that_ends_its_streams_spends_none_of_the_allowance` — `echo hello`,
  no descendant anywhere in it.
- `what_was_captured_is_still_captured` — a mebibyte on each stream.
- `the_readers_are_drained_before_any_of_them_is_reached`, under
  `test-support`, which was the same `echo hello`.

**Five remain in the default suite, and a sixth under `test-support`. They are
one question rather than six.** Each builds an escaped descendant out of
`setsid` and job control:

| Still failing | |
| --- | --- |
| `a_held_reply_pipe_no_longer_holds_the_call` | four `setsid` rows |
| `a_child_that_exits_while_a_pipe_is_held_returns_after_the_cleanup` | |
| `a_descendant_that_never_stops_writing_does_not_hold_the_call` | |
| `a_completeness_check_can_see_a_prefix_that_truncation_cannot` | |
| `the_three_shortfalls_are_independent` | **mixed**: the first half escapes, the second is the size cap alone and would pass |
| `an_interrupt_during_the_cleanup_is_still_an_interrupt` | `test-support` only, and it also needs `kill -INT` |

Decision 5 rules the escaped descendant unreachable on Windows, so there is no
program to substitute; `commands::shell` says so where they are written. What
is left is a decision and not a substitution: whether each of these contracts
can be asked another way on Windows, or whether gate 9's new pattern applies
and the record should say the question does not exist there. The mixed row asks
a second question — whether a gate that mixes the two should be split. Neither
is improvised here.

Measured natively, on this tree:

- `cargo test --locked --no-fail-fast`: **205 passed, 9 failed**, from 202 and
  12 at the start of this pass.
- With `--features test-support`: **211 passed, 13 failed**, from 207 and 17.
- `cargo fmt --check` succeeds.

The nine are the five above, the three `child_input.rs` cases that need the
same escaped-descendant answer, and gate 10's job attribution, which passes
when its test binary is run directly. Every failure left on Windows is now
either that one question or that one environment.

## Decision 5 separates the premise from the contracts

The statement immediately above describes the nine default failures. The
thirteen with `test-support` also include the cleanup-interrupt fixture and
all three `delivery_failure.rs` cases. In particular, the unreadable-stream
case needs no escaped descendant: it was blocked by `sh` before reaching
`RNX_TEST_CAPTURE_FAILS`.

The amendment to decision 5 in the record maps each remaining fixture to its
native question. An escaped holder surviving job termination is inapplicable
with Gate 10's evidence. Buffered output, capture flags, cancellation,
injected failures, and worker collection remain required. Existing native
coverage and fixtures still to write are labelled separately; the amendment
does not mark any unimplemented replacement as passing and does not hide
any of the existing failures with `cfg`.

One correction matters for gate 5: `run_child` calls `group.end()` before
`writing.stop()`. A write unblocked when job termination closes the read end
does not prove `CancelIoEx` cancelled it. The record now requires both the
end-to-end cancellation result and an isolated real-pipe control that keeps
the reader open and observes `ERROR_OPERATION_ABORTED`. That mechanism gate
is still to write; no new platform behavior is claimed from this code audit.

Three changes are implemented alongside the decision:

- `the_size_cap_alone_does_not_cut_a_capture_short` now runs independently
  of the escaped-holder half of `the_three_shortfalls_are_independent`.
  It asserts the exact cap, `truncated=true`, `cut_short=false`, and
  `unreadable=false` using the existing native zero-byte child.
- `an_unreadable_stream_is_reported_and_excuses_nothing` uses the native
  echo child, with the original injection and assertions. It now reaches
  the read-failure hook on Windows.
- The Windows cleanup-interrupt gate now asserts `(cancelled, timed_out,
  cut_short) == (true, false, true)`. Its existing phase handshakes and
  console-isolation checks remain in place.

All three targeted tests pass with `test-support` on native Windows.
The original escaped-holder cases remain failing until the planned native
coverage exists. The ordinary build still omits injection hooks, and no
production code changed in this pass.

Full Windows reruns with `--locked --no-fail-fast`: **206 passed, 9 failed**
by default, **213 passed, 12 failed** with `test-support`; both exit 101.
The new cap-only test adds one pass in each configuration, and the native
read-failure child moves one `test-support` failure to a pass. Formatting
and `git diff --check` pass. Local logs: `../tmp/rnx-contracts-default.log`
and `../tmp/rnx-contracts-support.log`. These changes have not run on Linux.

## Gate 5's mechanism control is written, and controlled

The record's amendment asks for a native gate around the real `Pipe::begin`
and `Pipe::stop` and a synchronous write, separate from the end-to-end
cancellation result: in `run_child`'s cleanup `group.end()` precedes
`writing.stop()`, and job termination can unblock a write with no `CancelIoEx`
involved. It is written and it passes.

**The premise was measured first.** `CancelIoEx`, called from another thread
against a `WriteFile` blocked on a full anonymous pipe, returns 1 and the write
returns 995. Without that this protocol would have been reasoning rather than a
mechanism, and the gate below would have had nothing to gate.

**What the control is.** `rnx test-pipe-control cancel|held`, in
`src/pipe_control.rs`: a command that exists only on Windows and only under
`test-support`. An ordinary build answers ``rnx: `test-pipe-control` is not a
command`` and prints the usage. It owns **both** ends of a `std` pipe, submits
one four-mebibyte write through the real `Pipe::begin`, and never closes the
read end. `tests/pipe_cancel_windows.rs` runs each half as its own process and
bounds it at twenty seconds, because a cancellation that does not reach the
operation leaves a write blocked in the kernel: a parent can kill that, and an
in-process gate would stop the suite it belongs to.

| Half | What it does | What it reported |
| --- | --- | --- |
| `cancel` | waits past any chance of the write returning, checks it is still unfinished, then stops | `blocked_before_stop=true stopped=Reached outcome=failed error=995` |
| `held` | the same wait and check, then **drains** the read end | `finished_before_release=false outcome=returned error=none drained=4194304` |

995 is `ERROR_OPERATION_ABORTED`. The read end is open and undrained through
the whole of the first half. The release in the second is a drain and not a
close, because an end that came from closing the read end is the very thing the
amendment says cannot be told apart from a cancellation.

**That the write was blocked is measured rather than assumed.** Both halves
wait first and report whether the worker was still unfinished when they acted.
The declaration alone would not have said it: an announced intention to write
is not a pending operation, which is the distinction the amendment draws and
the reason the aborted result is the evidence.

**Controlled.** With `platform::imp::cancel` made a no-op — the flag-only
implementation gate 5 was originally written against — `Pipe::stop` never
returns, the helper never reports, and the gate fails on its bound after
21.8 seconds with the helper killed rather than left behind. The negative
control passes unchanged under the same break, which is right: it cancels
nothing. So the gate fails for the reason it exists and not for a timing
accident.

This is a controlled pipe and not a claim about descendants. There is no
child, no job and no escape anywhere in it.

Measured natively, on this tree:

- `cargo test --locked --features test-support --no-fail-fast`: **215 passed,
  12 failed**, from 213 and 12. The two new cases are the whole of the change.
- `cargo test --locked --no-fail-fast`: **206 passed, 9 failed**, unchanged —
  the gate does not exist in a default build.
- `cargo fmt --check` succeeds.

What this does **not** close: gate 5's end-to-end half still has no Windows
fixture. `child_input::a_cancellation_ends_the_call_while_delivery_is_blocked`
is one of the nine, and the record lists it as work to write. This control
answers the mechanism question that fixture could not have answered anyway.

## Gate 5's end-to-end cancellation and writer collection

`console_interrupt::cancellation_collects_a_writer_blocked_on_child_input`
now asks the public half of Gate 5. It uses the existing console-isolation
detector and interrupt hook, but readiness comes from pending I/O on the
actual delivery worker, not from a marker before the process call.

The Rune script feeds 1,050,000 bytes to `ping -n 30 127.0.0.1`, which does
not read stdin, with a 15-second call deadline. After the call it prints
`(cancelled, timed_out)` and waits on its own stdin so the test can observe
worker termination while rnx remains alive.

The Windows-only `delivery_control` module is compiled only with
`test-support`. When `RNX_TEST_DELIVERY_CONTROL` names the test directory,
the parent publishes the writer thread ID, child PID, and input length.
The gate opens and retains Windows handles, verifies the thread belongs to
this rnx process, and waits for `GetThreadIOPendingFlag` to report pending
I/O while the child is live. That API reports pending requests, not a
permanent thread state; see [Microsoft's API contract](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getthreadiopendingflag).
Here the identified worker's only pre-interrupt I/O is delivery to a child
that never drains the pipe. The identity file is written by the parent;
the worker performs no file I/O before the interrupt, including if its
delivery unexpectedly ends early.

Only after those observations does the test release the interrupt trigger.
It waits for the handler's receipt and the writer's delivery-completed
handshake. The writer then remains held alive at a bounded test hook before
thread exit. For 300 ms the gate verifies that the call has reported nothing
and the writer's handle remains unsignalled. It releases the worker, waits
for the result, and requires all of the following before releasing rnx's
stdin:

- The result is exactly `(true, false)`: cancelled, not timed out.
- The writer and child handles are signalled: both have terminated.
- rnx is still alive, so process exit did not dispose of an uncollected
  worker on the test's behalf.
- The cancellation-to-result interval, including the deliberate hold, is
  under three seconds, rather than the child's 15-second deadline.
- The interrupt was installed, restored, raised and received, and none
  reached the test runner's console.

Every observation is bounded. Teardown releases the held worker, closes
rnx's stdin, reaps or kills rnx with a bound, and reads the file-backed output
before any assertion. A hold timeout is a failure. The ordinary build has
neither this module nor its environment hooks.

**Controlled against an omitted join.** Temporarily replacing
`delivery.join()` with dropping its handle and an abandoned outcome made
this gate fail with `the call returned before collecting the held writer`.
The join was restored immediately afterwards. This distinguishes collection
from a detached worker that happens to finish quickly. The positive native
run passes; the independent `pipe_cancel_windows` control remains the proof
about `CancelIoEx` itself.

The Unix escaped-holder cancellation fixture in `child_input.rs` is now
`cfg(unix)`, with an explicit reference to this replacement and Gate 10's
attributable breakaway refusal. It is not counted as a Windows pass. The
replacement runs with `test-support`, so both configurations are still
needed for the native ladder. This closes Gate 5's end-to-end half, not the
separate normal-exit writer-collection or blocked-input deadline fixtures.

Full native reruns: **206 passed, 8 failed** by default; **216 passed,
11 failed** with `test-support`, both using `--locked --no-fail-fast` and
exiting 101. The count changes are one Unix-only fixture removed from each
Windows run and one native replacement added with `test-support`; the
default run does not claim to have exercised cancellation. All five console
tests and both real-pipe controls pass. `cargo fmt --check` and
`git diff --check` pass. Linux has not run these changes.

Local logs: `../tmp/rnx-delivery-default.log`,
`../tmp/rnx-delivery-support.log`, and the expected-failure
`../tmp/rnx-delivery-no-join.log`.

## A deadline on a blocked delivery, and what the cleanup order does to it

The record's row for
`child_input::a_deadline_ends_the_call_while_delivery_is_blocked` is answered.
`tests/delivery_deadline_windows.rs` holds the read end with an ordinary
in-job child that simply never reads — `ping -n 30`, which has no reason to
— and gives it a mebibyte with nowhere to go. The escaped
fixture is now `cfg(unix)`, naming its replacement.

What it establishes, in the order it establishes it:

- **The delivery is blocked**, by `GetThreadIOPendingFlag` on the writer's own
  thread. More bytes than a pipe holds is a reason to expect a block, not
  evidence of one. The thread's identity comes from rnx through
  `RNX_TEST_DELIVERY_CONTROL`, and the handle is opened and held, so no id can
  be reused underneath the observation.
- **The child is alive while the writer is blocked**, which is what rules out a
  child that left of its own accord.
- **The deadline ends it**, promptly, and the call reports `(true, false)` —
  timed out, not cancelled.
- **The writer is collected before the call reports.** The worker is held past
  the end of its delivery and the call must stay silent throughout; then it is
  released, and the writer and the child are both observed terminated while
  rnx is still running.

**Controlled.** With `delivery.join()` replaced by dropping the handle, the
gate fails with "the call reported before collecting the held writer". A
worker that happened to finish first would otherwise let a missing join look
like a collection, which is what the hold is for.

**The first draft of this gate asserted something false, and the amendment is
why.** It asked that the child still be alive when the delivery *ended*. It is
not: `run_child`'s cleanup calls `group.end()` before `writing.stop()`, so on
the deadline path the job — and the child's read end with it — is gone
before the writer is ever stopped, and the write is unblocked by that closing.
This is the same finding the amendment records for cancellation, on the other
path that reaches it. So this gate does not attribute the unblocking to the
writer's stop, and the aliveness check moved to where it says something. What
attributes the mechanism is `pipe_cancel_windows`, and only that.

**One `test-support` hook grew a condition.** `before_thread_exit` waited for
an interrupt receipt before holding the worker, because a console event
arrives independently of the delivery and the child can close its pipe just
before rnx's handler runs. A deadline is the delivery's own ending and has
nothing arriving from elsewhere to wait for, so what the hold waits for is now
named: `RNX_TEST_DELIVERY_HOLD` is `interrupt` by default — unchanged — or
`deadline`. Without it every deadline run under the control would have waited
out the receipt's two-second bound and then not held at all.

Measured natively, on this tree:

- `cargo test --locked --features test-support --no-fail-fast`: **217 passed,
  10 failed**, from 216 and 11.
- `cargo test --locked --no-fail-fast`: **206 passed, 7 failed**, from 206 and
  8 — the escaped fixture no longer runs here, and the new gate is
  `test-support` only.
- `cargo fmt --check` succeeds.

The seven are the five `capture_bound.rs` escaped-holder cases,
`child_input::the_delivery_thread_is_gone_before_the_call_returns`, and gate
10's job attribution. Under `test-support` the two `delivery_failure.rs` cases
and `capture_bound::an_interrupt_during_the_cleanup_is_still_an_interrupt`
join them. Every one is a row the record already dispositions.

## A normal child exit still collects its unfinished delivery

`delivery_exit_windows::a_normal_child_exit_collects_the_unfinished_writer`
now supplies the Windows replacement for
`child_input::the_delivery_thread_is_gone_before_the_call_returns`.

The child is PowerShell using the shared native-command builder. It never
reads stdin: it waits for the gate's `release-child` file, then exits 0.
Its handshake has a twenty-second bound and throws on expiry, so a timed-out
handshake cannot provide the expected successful child status. The process
call has a thirty-second deadline, deliberately outside that window.

The script provides 1,050,000 bytes, reports `(code, timed_out, cancelled)`,
and waits on its own stdin after the call. Using the existing published
delivery identity, the test opens and retains the writer and child handles,
checks thread ownership and input size, and observes pending writer I/O while
the child is still alive. Only then does it let the child exit normally.

`RNX_TEST_DELIVERY_HOLD=exit` names the new premise explicitly. Like the
deadline hold, it needs no console-event receipt. After delivery ends the
worker announces completion but stays alive until released. The call must
remain silent during the 300 ms hold. After release, the test requires the
writer and child handles to be signalled while rnx is still alive, checks
`GetExitCodeProcess` is 0 for the child, and requires exactly
`(0, false, false)` from the script. Returning after a timeout or interruption
cannot satisfy that result. The delivery must end within three seconds of
the release, and the call must report within two seconds of worker release.

Teardown releases both the child and worker, closes rnx's stdin, and bounds
reaping and killing before checking assertions. Output is file-backed, and
an unreadable result file or a hold timeout fails the gate.

**Controlled:** dropping the delivery handle in place of `delivery.join()`
makes this test fail with `the call reported before collecting the held
writer`. The original `host.rs` bytes were restored after that run. The
positive native test passes. This is evidence about normal-exit collection,
not about which operation ended a blocked write; the independent
`pipe_cancel_windows` controls retain that responsibility.

The original escaped-holder fixture is now `cfg(unix)` with its native
replacement and Gate 10 named beside it. Its Unix-only setup helpers are
likewise gated. The new test and `exit` hold are available only with Windows
`test-support`; the ordinary build gains no control hook. No Linux result is
claimed for these changes.

Full native reruns with `--locked --no-fail-fast`: **206 passed, 6 failed**
by default and **218 passed, 9 failed** with `test-support`; both exit 101.
The ordinary Windows suite loses one inapplicable escaped-holder case;
`test-support` also gains its passing native replacement. `child_input.rs`
now passes all nine cases that run on Windows, with its three lifecycle
contracts asked by the separate native fixtures under `test-support`.
Formatting and `git diff --check` pass.

Local logs: `../tmp/rnx-normal-exit-default.log`,
`../tmp/rnx-normal-exit-support.log`, and the expected-failure control
`../tmp/rnx-exit-no-join.log`.

## A held reply pipe, with the holder inside the job

The record's first `capture_bound.rs` row is answered, and two rows it had
already dispositioned elsewhere are now gated where they said they would be.

**Two gatings the record had already decided.** Both were still running on
Windows and failing there, with their replacements already passing:

| Fixture | Now | Windows asks it through |
| --- | --- | --- |
| `capture_bound::the_three_shortfalls_are_independent` | `cfg(unix)` | `capture_bound_windows::a_stream_past_the_cap_is_also_cut_short_when_the_call_ends`, with the cap-only half already split out as `the_size_cap_alone_does_not_cut_a_capture_short`, which runs everywhere |
| `capture_bound::an_interrupt_during_the_cleanup_is_still_an_interrupt` | `cfg(all(unix, feature = "test-support"))` | `console_interrupt::an_interrupt_during_the_cleanup_is_still_reported` |

**The new gate.** `tests/capture_hold_windows.rs` runs all four of record
0023's rows — neither, stdout, stderr, both — with an in-job grandchild in
place of the `setsid` one. The direct child is a PowerShell holder that starts
the grandchild and then stays well past the deadline; a stream the grandchild
does not keep is redirected to the holder, which stays alive holding it,
because closing it instead would have ended the grandchild's write rather than
detaching it, and a grandchild that died of a broken pipe is not a holder.

**Which pipes the grandchild had is measured, not assumed.** This is the part
a fixture of this shape can get wrong invisibly: four rows that differ only in
what the fixture believes are one row run four times. So the grandchild writes
a marker to each stream it kept before settling down to wait, and the case
asserts rnx's captured lengths exactly — three bytes on standard output where
it kept it, four on standard error, and zero where it did not. Measured first:
a grandchild started with no redirection inherits its parent's handles, and a
pipe so inherited stays open after the direct child leaves.

Alongside that, each row asserts `timed_out=true` with `cancelled=false`, a
return well inside the holder's thirty seconds, and the ends of **both** the
holder and the grandchild by handles opened while both were alive — so no
process that later wears the same id can answer for them — with rnx still
running when they are read.

**Controlled.** With `Job::end` made a no-op — `TerminateJobObject` removed
— the gate fails on its first row, `neither`, where the pipes are held by the
direct child alone: the call never returns inside the bound, and the teardown
reports rnx failing to exit after its stdin was closed rather than quietly
leaving it. That is the failure in the shape the contract describes.

**What this does not claim.** No descendant escapes anything here. The row
says a descendant surviving the job's termination is inapplicable with gate
10's evidence, and this asks the question that is left: a held pipe must not
hold the call, and the job must end both processes.

Measured natively, on this tree:

- `cargo test --locked --no-fail-fast`: **207 passed, 4 failed**, from 206 and
  6.
- With `--features test-support`: **219 passed, 6 failed**, from 218 and 9.
- `cargo fmt --check` succeeds.

The four are gate 10's job attribution, which passes when its binary is run
directly, and the three `capture_bound.rs` rows the record still lists as work
to write: `a_child_that_exits_while_a_pipe_is_held_returns_after_the_cleanup`,
`a_descendant_that_never_stops_writing_does_not_hold_the_call`, and
`a_completeness_check_can_see_a_prefix_that_truncation_cannot`. Under
`test-support` the two `delivery_failure.rs` rows join them. The holder built
here is what the first two of those three need.

## Normal child exit ends an in-job reply-pipe holder

`capture_hold_windows::a_normal_exit_ends_the_in_job_pipe_holder_during_cleanup`
extends the existing holder fixture to the normal-exit path. The direct
PowerShell child starts the same grandchild, this time retaining both reply
streams. The grandchild writes its `OUT` and `ERR ` markers and then a
readiness file before settling into its thirty-second stay. The parent
publishes IDs only after that file exists, so releasing it cannot cut off
the grandchild before the pipe-inheritance evidence was written.

The test opens and retains both process handles while both are alive, then
releases the direct child to choose exit 7. The call has a thirty-second
deadline, but must return within eight seconds. The exact result is
`code=7 timed_out=false cancelled=false out=3 err=4`; Windows separately
reports exit code 7 for the direct child. Both processes must have terminated
while rnx is still alive waiting on its own stdin. Thus timeout, interruption,
and rnx's own exit cannot substitute for cleanup after the chosen child exit.

**The control has to distinguish job end from job-handle drop.** A normal
call gives readers a short cleanup allowance, then drops the job handle on
return. Kill-on-close could therefore hide an omitted `TerminateJobObject`
behind the ordinary hundred milliseconds. For this case, `test-support`
stretches that allowance to twelve seconds while retaining the eight-second
return bound. Proper job termination lets the readers see EOF promptly.
With `TerminateJobObject` temporarily replaced by a no-op, the gate fails
with `normal exit: the call did not return in 8s, so it waited for a held
pipe`. Teardown still collects rnx; the source bytes were restored after
the control. The observed failing run lasted 14.38 seconds including cleanup.

The ordinary build ignores the allowance override and tests the native
lifecycle and result; the stronger omitted-job-end control requires
`test-support`. The existing four deadline rows also now assert the Windows
job-termination code 1 alongside their pipe-marker lengths and flags.

The Unix escaped-holder normal-exit fixture is now `cfg(unix)` with the
native replacement named beside it. This does not claim that killing a job
implies `cut_short=true`: the cap-hold gate already demonstrates a stopped
reader, and the uncapped-prefix fixture is still separate work. No production
code changed in this pass, and Linux has not run these fixture changes.

Full native reruns: **208 passed, 3 failed** by default and **220 passed,
5 failed** with `test-support`, using `--locked --no-fail-fast`; both exit
101. The new ordinary-exit case passes in both builds and the Unix lifetime
case no longer runs on Windows. The remaining default failures are the
continuous-writer and uncapped-prefix fixtures plus Gate 10's blocked Cargo
control; `test-support` also has the two delivery-failure fixtures.

The unused Unix `holds` helper was also gated with `cfg(unix)`; a subsequent
`cargo check --locked --features test-support --test capture_bound` has no
warnings. Formatting and `git diff --check` pass. Local logs:
`../tmp/rnx-holder-exit-default.log`, `../tmp/rnx-holder-exit-support.log`,
and the expected-failure `../tmp/rnx-holder-exit-no-end.log`.

## A writer that never stops, inside the job

The record's continuous-writer row is answered.
`capture_hold_windows::a_descendant_that_never_stops_writing_does_not_hold_the_call_windows`
gives the in-job grandchild an endless `for /l` echo loop — a step of nothing
never reaches its end, so the only thing that stops it is rnx. `holder` grew a
`settles` parameter for the difference, rather than a second near-copy of
itself: everything else about the two cases has to be the same for their
answers to be comparable.

It asserts prompt return, `timed_out=true`, `cancelled=false`,
`unreadable=false`, and the ends of both the holder and the grandchild by
handles opened while both were alive, read while rnx is still running. And
**more than a pipeful captured** — 1.1 to 1.2 MB measured — so the reader is
known to have gone round its loop rather than taken one bufferful and been
stopped by the clock before it asked again.

**The capture flag comes out the other way from Unix, and it is asserted
rather than dropped.** Unix reports `cut_short=true`: its writer survives the
group's termination and goes on producing, so the reader gives up on a stream
that never ends. Windows reports `cut_short=false`, and correctly — the job
ends the writer, its write end closes, and the reader drains what is left and
sees the end of the file inside the allowance. `cut_short=true` there would
mean rnx stopped before a stream that had already finished. This is the row's
own point that the job test is not evidence about a writer which outlived it,
made into an assertion instead of a silence.

**Controlled.** With `TerminateJobObject` removed from `Job::end`, the gate
fails on its own clause — "the call did not return in 8s, so a writer that
never stops held it" — and the teardown reports rnx failing to exit after its
stdin was closed rather than leaving it behind.

**One defect in the gate itself, found by it failing.** The byte count was
first read by splitting the reported line on `out=`, which `timed_out=` also
contains, so it parsed the word `true` and reported no byte count at all. It
reads the field by name now. A gate that cannot parse its own measurement
would have been a gate that failed for a reason unrelated to the contract.

Measured natively, on this tree:

- `cargo test --locked --no-fail-fast`: **209 passed, 2 failed**, from 208 and
  3.
- With `--features test-support`: **221 passed, 4 failed**, from 220 and 5.
- `cargo fmt --check` succeeds.

What is left on Windows:
`a_completeness_check_can_see_a_prefix_that_truncation_cannot`, which the
record says needs a reader held after a known prefix and before the end of the
file, and gate 10's job attribution, which passes when
its binary is run directly. Under `test-support` the two `delivery_failure.rs`
rows join them.

## Uncapped prefix: held after capture and before EOF

The Windows replacement for
`a_completeness_check_can_see_a_prefix_that_truncation_cannot` now passes in
`capture_bound_windows.rs` under `test-support`. The Unix escaped-holder
fixture is `cfg(unix)`, with its native replacement named beside it.

The new `RNX_TEST_READER_PREFIX_BYTES` and
`RNX_TEST_READER_HOLDS_AFTER_PREFIX` hook holds stdout after the requested
number of bytes has been captured, before the next read. It handles a prefix
split across reads. Its acknowledgement publishes the measured byte count
by rename after writing, so the test cannot read a partially written count.
It has a 20-second bound and emits a diagnostic if that bound releases it.
The hook is inert without `test-support`.

The child writes exactly `partial` through PowerShell's raw standard-output
stream, then waits for the reader's acknowledgement before exiting 0.
The test observes seven captured bytes, waits for the existing stop-published
signal, and only then releases the reader. The cleanup allowance is zero;
the child cannot exit before the prefix is captured, so no startup delay or
pipe-capacity assumption is needed. The complete report is:

```text
code=0 timed_out=false cancelled=false truncated=false cut_short=true unreadable=false out=partial err=
```

The test rejects any rnx stderr, including hook-timeout diagnostics, and
bounds process collection and failed-test teardown. This establishes an
uncapped prefix cut short before observing EOF, not a surviving writer.

Controlled: temporarily replacing the capture loop's `got.cut_short = true`
assignments with `false` makes this gate fail on `cut_short=false`; code 0,
the exact prefix, and every other reported field remain unchanged. That
mutation was restored byte-for-byte before both full suites were run.

An initial fixture using PowerShell `-File` was refused by this machine's
script execution policy. The passing fixture uses `-Command`, as the shared
native command fixtures do, without changing that policy.

Measured with `cargo test --locked --no-fail-fast`: **209 passed, 1 failed**.
With `--features test-support`: **222 passed, 3 failed**. Both capture-bound
Windows gates pass. `cargo fmt --check` is clean. The remaining failures are
Gate 10's Cargo attribution control, plus the two already dispositioned
`delivery_failure.rs` fixtures under `test-support`. The default count drops
one Unix-only failure; the native replacement adds one test-support pass.
Logs are `C:/Users/em/work/tmp/rnx-prefix-{default,support,control}.log`.
Linux execution remains pending. Nothing is committed.

## A delivery failure that arrives after the cleanup has begun

The first of the two `delivery_failure.rs` rows is answered by
`delivery_failure_windows::a_failure_after_cleanup_begins_still_reaches_the_script_windows`,
and the row's warning is the whole of its design. Substituting the child would
not have done: the injection is consulted only on the branch that observes the
stop, and on Windows a write that is cancelled, or that finds its reader gone,
returns from the write itself and never reaches that branch. A fixture built
that way would report `Abandoned` and pass for asking nothing.

So what is arranged is not a blocked write but a **still** writer.

**Two hooks, and the second is the one that is easy to get wrong.**

| Hook | What it does |
| --- | --- |
| `RNX_TEST_WRITER_HOLDS_BEFORE_STOP` | Holds the writer once, at the top of its loop and before it looks at its stop flag. Once and only before anything is written: a writer held on every turn would hold the call for as many turns as it took, and one that had already submitted a write is the case that cannot be steered here. |
| `RNX_TEST_SIGNAL_WRITER_STOP_TO` | Publishes the writer's stop, between `writing.stop()` and the collection. |

The readers' existing `announce_the_stop` could not serve: it fires once the
readers have been reached, which is **after** the delivery is joined. A gate
that waited for it before releasing a held writer would be waiting for
something the held writer is what prevents.

The gate asserts exit 1, the injected words on rnx's standard error, no
success line on standard output, and no hook released by its own timeout — a
writer let go by a timeout may have looked at a stop that was not yet set,
which is the ordinary case wearing this gate's name.

**Controlled twice, and the second control is about the ordering rather than
the code.**

| Control | What the gate said |
| --- | --- |
| The injection consult removed from `deliver` | exit 0, nothing on standard error, and "the call succeeded, which it should not have" on standard output |
| The writer released **before** the stop is published | the same three, exactly — so the wait on the publication is load-bearing and not decoration |

The second is the more useful of the two: it establishes that this fixture is
asking about the branch it claims to, rather than passing because an injected
failure happened to be configured.

Measured natively, on this tree:

- `cargo test --locked --features test-support --no-fail-fast`: **223 passed,
  2 failed**, from 222 and 3.
- `cargo test --locked --no-fail-fast`: **209 passed, 1 failed**, unchanged —
  this gate does not exist in a default build.
- `cargo fmt --check` succeeds.

**Two failures are left on Windows, and one of them is the machine.**
`a_child_cannot_break_away_because_the_job_forbids_it` is gate 10's
attribution, which section G records as passing when its test binary is run
directly and failing under Cargo's own job. The other is
`delivery_failure::a_reader_that_panics_does_not_strand_the_other`, under
`test-support`, which the record still lists as work to write: the stdout
reader's panic injected while the stderr worker is held alive, with the actual
panic error asserted and the other worker observed collected before the call
returned.

## Reader panic: collect the other worker before reporting the error

`reader_panic_windows::a_reader_that_panics_does_not_strand_the_other_windows`
is the native replacement for the remaining reader-panic row. The original
escaped-holder and `/proc` fixture is now `cfg(unix)`, naming its replacement.

With Windows and `test-support`, `RNX_TEST_READER_COLLECTION_CONTROL` holds
both readers before the existing panic injection. Stderr publishes its own
thread ID atomically and waits for `release-stderr`. The gate opens that
thread, checks that it belongs to this rnx process and is alive, and retains
the owned handle. Only then does it create `release-stdout`, letting stdout
reach the existing injected panic. Both holds are bounded at 20 seconds;
publication errors and timeout releases produce diagnostics that fail the gate.

The gate waits for both the actual injected panic diagnostic and the cleanup
reader-stop publication. For 300 ms it requires the call to remain silent
and the retained stderr handle to remain unsignalled. It then releases stderr,
waits for the call's report, and requires the same handle to signal termination
while rnx is still running. The script catches the error and waits on stdin;
the exact result is `error=stdout reader panicked`, not merely a `Result`.
Teardown releases both hooks, closes rnx stdin, and bounds collection and kill.

Controlled: moving propagation of stdout's join error ahead of stderr's join
fails with **the call reported before collecting the held stderr worker**.
The actual panic error still reaches the script, so that error alone cannot
pass this ordering gate. The mutation was restored byte-for-byte before the
full suites. No production collection behavior was changed; the new scheduling
hook is inert without Windows plus `test-support`.

Measured: `cargo test --locked --no-fail-fast` reports **209 passed, 1 failed**;
adding `--features test-support` reports **224 passed, 1 failed**, from 223/2.
The sole failure in each is Gate 10's already recorded Cargo attribution
control. Both delivery-failure native replacements now pass. Formatting and
`git diff --check` pass. Logs are
`C:/Users/em/work/tmp/rnx-reader-panic-{default,support,control}.log`.
Nothing is committed; Linux verification remains pending.

## Where Windows stands, and gate 10 re-run on this tree

Every row of the record's disposition table is answered. One failure is left
under Cargo, and it is the one the record asks to be left.

**Gate 10, measured again here** — not from the handoff's supplied figures,
and after every fixture in the table changed around it.

| Launch path | Result |
| --- | --- |
| `cargo test` | fails, as designed: `in_job=true permits=Some(false)` and the runner's own breakaway is `Refused(Some(5))`, so the refusal inside rnx is not attributable to rnx's job |
| the compiled binary run directly | **2 of 2 pass** |

Section G's distinction between the launch paths holds unchanged. The gate
still refuses to pass on its four inner observations alone, and the direct run
still supplies gate 10's evidence. Nothing about that was weakened to reach a
green count; the failure that remains is the machine, and it is recorded
rather than skipped.

**Section G's "what this licenses" paragraph has been overtaken.** It speaks
of five `capture_bound.rs` cases that could be marked inapplicable. They were
not marked inapplicable: each was separated, and the contract it carried is
asked natively. The sections above are what happened instead.

**Every Unix fixture that could not be constructed here, and what asks its
contract now.** Each escaped-holder fixture stays `cfg(unix)` and names its
replacement in place.

| Unix fixture | Windows |
| --- | --- |
| `capture_bound::a_held_reply_pipe_no_longer_holds_the_call` | `capture_hold_windows::a_held_reply_pipe_no_longer_holds_the_call_windows` |
| `capture_bound::a_child_that_exits_while_a_pipe_is_held_returns_after_the_cleanup` | `capture_hold_windows::a_normal_exit_ends_the_in_job_pipe_holder_during_cleanup` |
| `capture_bound::a_descendant_that_never_stops_writing_does_not_hold_the_call` | `capture_hold_windows::a_descendant_that_never_stops_writing_does_not_hold_the_call_windows` |
| `capture_bound::the_three_shortfalls_are_independent` | split: `the_size_cap_alone_does_not_cut_a_capture_short` everywhere, and `capture_bound_windows::a_stream_past_the_cap_is_also_cut_short_when_the_call_ends` |
| `capture_bound::a_completeness_check_can_see_a_prefix_that_truncation_cannot` | `capture_bound_windows::a_completeness_check_can_see_a_prefix_that_truncation_cannot_windows` |
| `capture_bound::an_interrupt_during_the_cleanup_is_still_an_interrupt` | `console_interrupt::an_interrupt_during_the_cleanup_is_still_reported` |
| `child_input::a_deadline_ends_the_call_while_delivery_is_blocked` | `delivery_deadline_windows::a_deadline_ends_a_blocked_delivery_and_the_writer_is_collected` |
| `child_input::a_cancellation_ends_the_call_while_delivery_is_blocked` | `console_interrupt::cancellation_collects_a_writer_blocked_on_child_input`, with the mechanism in `pipe_cancel_windows` |
| `child_input::the_delivery_thread_is_gone_before_the_call_returns` | `delivery_exit_windows::a_normal_child_exit_collects_the_unfinished_writer` |
| `delivery_failure::a_failure_after_cleanup_begins_still_reaches_the_script` | `delivery_failure_windows::a_failure_after_cleanup_begins_still_reaches_the_script_windows` |
| `delivery_failure::a_reader_that_panics_does_not_strand_the_other` | `reader_panic_windows::a_reader_that_panics_does_not_strand_the_other_windows` |
| `delivery_failure::an_unreadable_stream_is_reported_and_excuses_nothing` | the same fixture, with a native child |

Measured natively, on this tree:

- `cargo test --locked --no-fail-fast`: **209 passed, 1 failed**.
- With `--features test-support`: **224 passed, 1 failed**.
- `cargo fmt --check` succeeds.
- `tests/job_breakaway.rs` run directly: **2 passed, 0 failed**.

**Gate 12's check, re-established on this tree.** Every target type-checks
with `--all-targets`, with and without `test-support`. This was run from the
Windows machine, with the other targets' standard libraries installed through
`rustup`; `cargo check` does not link, so no cross-linker is involved and none
of this is a behavioural claim.

| Target | plain | `--features test-support` |
| --- | --- | --- |
| `x86_64-unknown-linux-gnu` | ok | ok |
| `aarch64-apple-darwin` | ok | ok |
| `x86_64-apple-darwin` | ok | ok |
| `x86_64-pc-windows-msvc` | ok | ok |

So every `cfg(unix)` arm still compiles, including the ones now reached only
there: `holds`, the escaped-descendant fixtures, and the Unix halves of
`tests/harness/commands.rs`.

**What this still does not establish, and it is not small.** Type-checking is
not running. Nothing here has *run* on Linux since the first fixture changed.
The `cfg(unix)` gatings, the shared `tests/harness/commands.rs`, the two hooks
added to `deliver`, the writer-stop publication, and the per-platform depth
constants in `eval_outcome.rs` and `json.rs` are all changes Linux has
compiled here and executed nowhere. Record 0025's gates 1 and 12 are stated
against Linux counts, and those counts have not been taken since. Windows
answering its contracts is half of what record 0001 asks for; the other half
is that Linux still answers its own.

## Provenance

| | |
| --- | --- |
| Commit | `88cebb1b19ec8042bb681dbf9971f32a671edcbd`, tree clean, level with `origin/main` |
| OS | Windows 11 Pro, NT 10.0.26200.0 |
| Toolchain | rustc 1.98.1 (48a229cea 2026-09-01), host `x86_64-pc-windows-msvc` |
| Cargo | 1.98.1 (797e8a9bc 2026-08-05) |
| `rnx` on PATH | `C:\Users\em\.cargo\bin\rnx.exe`, written 2026-09-10 03:41 |
| `rnx version` | `rnx 0.0.0` / `rune 0.14.2`, which is the `=0.14.2` pin at `Cargo.toml:43` |

`core.autocrlf` is `true` and there is no `.gitattributes`, so the working
tree is CRLF. `cargo fmt --check` passes regardless: rustfmt infers newline
style per file.

Every probe below uses that one installed binary, a debug-profile
`cargo install` of this commit.

## Gate 11 — installation

`cargo install --locked --path .` produced the binary above, and it runs.
Record 0025's gate 11 says `cargo install --locked`, which is not a runnable
command on its own; `--path .` is the form used and the form the gate should
say.

That is the whole of what it establishes. A working binary is not a passing
ladder, and the rest of this file is the difference.

## Gate 2 — the ladder, first run

`cargo test --locked --no-fail-fast` and the same with `--features
test-support`. Both exit 101.

| | Run | Passed | Failed |
| --- | --- | --- | --- |
| default | 207 | **194** | 13 |
| `test-support` | 212 | **196** | 16 |

Linux at this commit runs 232 and 237 (record 0027's evidence). The 25 fewer
are excluded by `cfg`, not failing:

- `tests/repl.rs` is `#![cfg(target_os = "linux")]` — the entire interactive
  suite, and the largest single omission.
- `tests/stdin.rs` is `#![cfg(unix)]`; it drives a pseudo-terminal, and gate
  8 is where Windows is asked the same question instead.
- One packaging test in `tests/release_metadata.rs` is `#[cfg(unix)]`; it
  shells out to `tar`. The other five run here and pass.

**Record 0025's own baseline is stale.** Gates 1 and 12 say Linux passes 226
and 231. Those figures predate record 0027, which moved the pin to Rune
0.14.2 and left Linux at 232 and 237. Gate 12 must be checked against the
later pair. The record is what changes, not the measurement.

## Every failure, by name

Sixteen with `test-support`; the thirteen without it are the same list less
the three marked *ts*.

| Test | File | Kind |
| --- | --- | --- |
| `a_deadline_ends_the_call_while_delivery_is_blocked` | `child_input.rs` | A |
| `a_cancellation_ends_the_call_while_delivery_is_blocked` | `child_input.rs` | A |
| `the_delivery_thread_is_gone_before_the_call_returns` | `child_input.rs` | A |
| `a_status_that_was_already_failing_survives_a_failing_stream` | `exit_status.rs` | A |
| `output_that_could_not_be_written_never_reports_success` | `exit_status.rs` | A |
| `a_child_that_exits_while_a_pipe_is_held_returns_after_the_cleanup` | `capture_bound.rs` | A |
| `a_completeness_check_can_see_a_prefix_that_truncation_cannot` | `capture_bound.rs` | A |
| `the_three_shortfalls_are_independent` | `capture_bound.rs` | A |
| `a_descendant_that_never_stops_writing_does_not_hold_the_call` | `capture_bound.rs` | A |
| `an_interrupt_during_the_cleanup_is_still_an_interrupt` *(ts)* | `capture_bound.rs` | A |
| `a_reader_that_panics_does_not_strand_the_other` *(ts)* | `delivery_failure.rs` | A |
| `a_failure_after_cleanup_begins_still_reaches_the_script` *(ts)* | `delivery_failure.rs` | A |
| `nothing_rnx_prints_can_move_a_cursor` | `terminal_safety.rs` | B |
| `a_deep_value_is_refused_with_a_status_and_never_a_signal` | `eval_outcome.rs` | C |
| `a_refusal_is_catchable_and_the_script_carries_on` | `json.rs` | C |
| `the_self_check_is_asked_for_by_name` | `commands.rs` | **D** |

### A. Fixtures that ask a Unix question

Portability work, no contract in doubt. The contracts they cover still need
answering on Windows, by fixtures that do not exist yet.

- `child_input.rs` — "the descendant never took the descriptor", the
  `exec 3<&0` handshake in `tests/harness/mod.rs`.
- `delivery_failure.rs` — the **same** handshake, same message, at
  `delivery_failure.rs:50` and `:157`.
- `exit_status.rs` — `/dev/full`, i.e. a stream that refuses writes; os
  error 3.
- `capture_bound.rs` — `setsid sh -c`.

**The two `delivery_failure.rs` cases matter more than their kind suggests.**
They fail while the fixture is still being set up, so
`RNX_TEST_READER_PANICS` and `RNX_TEST_DELIVERY_FAILS` are never reached. The
contracts they carry — a reader that panics does not strand the other, and a
delivery failure after cleanup begins still reaches the script — are
therefore **unanswered on Windows**, not passing and not failing. A fixture
that dies before the hook fires reports nothing about the hook.

On `capture_bound.rs`: it mixes two categories under one shape. Some cases
need an **escaped descendant**, which decision 5 rules unreachable on
Windows; others are **capture-flag contracts** (gates 5 to 7) that Windows
must still answer, and `an_interrupt_during_the_cleanup_is_still_an_interrupt`
is gate 6's own case. Excluding the file wholesale would hide required
coverage. Separating them is the first authoring task.

### B. A question Windows cannot be asked, like decision 5's

`nothing_rnx_prints_can_move_a_cursor` fails at its **sixth** surface — a
file path carrying `U+001B` — with os error 123, `InvalidFilename`. Win32
refuses an escape byte in a path component, so the surface record 0019
defends does not exist here.

Surfaces 1 to 5 pass on Windows. This is structurally decision 5's case: a
stronger guarantee, and gate 9 should record **the reason** — that the OS
refuses the name — rather than skip the surface or claim it passes.

### C. A lower ceiling, not a broken contract

Both failures exit **-1073741571 = `0xC00000FD` = `STATUS_STACK_OVERFLOW`**
where Linux exits 1 with a refusal. The first draft of this file read that as
rnx's contract failing on Windows. It is not, and the correction matters more
than the observation.

Record 0019's guarantee is **qualified**: for any value Rune can itself drop,
rnx refuses cleanly and the process exits with a status. Record 0019 already
records that deep enough values abort while dropping, and that no rnx-side
check can prevent it.

That qualified guarantee **holds on Windows.** Controlled with loop-built
values — `let v = []; for i in 0..n { v = [v] } v`, which is the shape the
failing tests use — at depths inside and outside the Windows ceiling:

| Depth | Rendered | Drop only, `()` returned |
| --- | --- | --- |
| 1,000 | exit 1, "the value is nested deeper than 256" | exit 0 |
| 3,000 | exit 1, same refusal | exit 0 |
| 6,000 | exit 1, same refusal | exit 0 |
| 8,000 | `STATUS_STACK_OVERFLOW` | **`STATUS_STACK_OVERFLOW`** |
| 10,000 and above | `STATUS_STACK_OVERFLOW` | `STATUS_STACK_OVERFLOW` |

The drop-only column is what isolates the phase: it renders nothing, so no
rnx walk runs, and it still overflows — with `thread 'main' has overflowed
its stack` on standard error. So this is **Rune's own build-and-drop
ceiling**, not rnx's rendering.

An exit status alone does not say *which* phase, so a marker separates them.
The script builds the value, prints, and returns `()`, leaving the scope to
drop it:

| Depth | Marker printed | Then |
| --- | --- | --- |
| 6,000 | yes | exit 0 |
| 8,000 | **yes** | `STACK_OVERFLOW` |
| 10,000 | **yes** | `STACK_OVERFLOW` |

The value is built successfully at every depth tested, and nothing runs after
the marker but the scope ending. The recursion that overflows is therefore
the **drop**, which is the mechanism record 0019 describes — not the build,
and not any walk of rnx's.

| | Build-and-drop ceiling |
| --- | --- |
| Linux, debug (record 0019) | between 49,152 and 65,536 |
| Windows, debug (here) | **between 6,000 and 8,000** |

Roughly eight times lower. So what fails is the **constant the acceptance
case chose**: `eval_outcome.rs` uses 32,768 because it sits inside Linux's
ceiling with margin, and on Windows it is far outside. That is a portability
failure in the fixture, of the same kind as category A, and it does not on
its own establish that record 0019's guarantee is broken.

What is **not** established: why the ceiling differs. The default main-thread
stack being 1 MB on Windows against 8 MB on Linux is the obvious candidate,
is consistent with the ratio, and is untested — no measurement here supports
it.

Separately, and a **different mechanism**: deeply nested *source*
(`[` × n then `]` × n, no loop) overflows at between 284 and 288 on Windows.
That is parser recursion — record 0019 names it as upstream issue #1040,
open, and distinguishes it from drop glue. The Linux figure for the same
probe has not been measured, so there is no comparison here, only a Windows
number.

### D. Defects in shipped code, not in fixtures

Category D was found through a failing test; a sweep of `src/` for the same
class of assumption found a second one that **no test on any platform would
have caught**.

What the sweep found clean, which is the more reassuring half:

- Every `libc`, `std::os::unix` and `std::os::fd` use is inside
  `src/platform.rs`. `host.rs` mentions them in two comments and nowhere
  else. The seam record 0025 describes is holding.
- `cfg` splits are three `cfg(unix)` and three `cfg(windows)` in
  `platform.rs`, plus one in `host.rs` — `aborted()`, which reads
  `ERROR_OPERATION_ABORTED` (995) on Windows and nothing on Unix. That is
  decision 3's mechanism, split at the mechanism and not at the contract,
  which is guardrail 2.
- No path-separator assumptions anywhere in `src/`.

Two things are not clean.

#### D1 — `rnx selfcheck` does not run on Windows

It prints most of its invariants and then exits 1 on:

```text
Error: Error("cannot serialize external references", line: 0, column: 0)
```

never reaching `incremental inputs`. The cause is `host::process_checks` at
`src/host.rs:971`, which hardcodes Unix paths in shipped code:

```rust
host::process("/bin/sh", ["-c", "exit 7"], 1000)
host::process("/usr/bin/head", ["-c", "3000000", "/dev/zero"], 1000)
host::process("/bin/sh", ["-c", "kill -INT $PPID; sleep 10"], 1000)
```

This is the one failure so far that is not a test's problem. Record 0025's
"what was written before the machine arrived" says `host.rs` asks for the
contract rather than the call; `process_checks` still asks for the call, and
no gate in the record covers it because `selfcheck` is not in the gate list.

Two separate things are wrong, and the second is the more interesting:

1. The paths. `host::process` itself is fine for this one case — a control
   run of `host::process("cmd.exe", ["/c", "exit 7"], 1000)` returns
   `code: 7` with all seven flags present and correct. That is one exit
   status from one child, and nothing more: the deadline, capture and
   interruption probes in the same function have **not** been run natively,
   because they die on `/bin/sh` first. Broader process acceptance is gates 3
   to 7 and remains open.
2. **The reason is swallowed.** Run alone, the failing call reports
   `error: cannot run /bin/sh: The system cannot find the path specified.
   (os error 3)` — accurate and useful. Inside `process_checks` that becomes
   `cannot serialize external references`, because the error value reaches
   `serde_json::to_value` before anyone reads it. A diagnostic that replaces
   the cause with a serialisation complaint is worth fixing whatever happens
   to the paths.

These are **two fixes, not one**, and keeping them apart matters: making the
probes native would make this failure disappear without the swallowing being
fixed, and the swallowing is what made a missing `/bin/sh` look like a
serialisation bug for as long as it took to run the call by hand.

#### D2 — the session's history never persists on Windows

`repl::history_path` at `src/repl.rs:104` reads `RNX_HISTORY`, else
`XDG_STATE_HOME`, else `HOME/.local/state`, and returns `Option`. Measured in
a plain PowerShell session on this machine:

| Variable | Value |
| --- | --- |
| `RNX_HISTORY` | unset |
| `XDG_STATE_HOME` | unset |
| `HOME` | **unset** |
| `USERPROFILE` | `C:\Users\em` |
| `APPDATA`, `LOCALAPPDATA` | set |

So `history_path()` answers `None`. Its two consumers, at `src/repl.rs:245`
and `:270`, are both `if let Some(path)`, so loading and appending are
skipped — **silently**. There is no warning and no error. A session on
Windows keeps no history between runs, and nothing says so.

Windows has no `HOME` and no XDG layout; the equivalent question is
`LOCALAPPDATA`, which is set. What that should be is a decision for the
record, not something to improvise here.

This is the sharpest argument for treating `tests/repl.rs` being
`#![cfg(target_os = "linux")]` as a gap rather than an exclusion. A defect in
the default command — `rnx` with no arguments is a session, by record 0024 —
survived because the suite that would have found it does not run here, and
because a session whose history is missing still looks like it works.

## What has been changed since

The measurements above are all of `88cebb1` unmodified. What follows is on
top of it, and the ladder was re-run after each.

| | |
| --- | --- |
| Record 0025 | Decision 7 added — the history location. The old decision 7 becomes **8**. Gates **17** and **18** appended. A risk added: an interrupt on Windows is console-wide. |
| `src/host.rs` | D1's **error preservation**, the `probe` it was lifted into so a check can drive it, and that check. |
| `src/platform.rs`, `src/repl.rs` | D2. |
| `tests/history.rs` | New, and not `cfg`-gated: D2's two checks. |

After: **197 passed of 210**, the same **13** failures, unchanged in name.
Unit tests go from 97 to 98 — one added — and `tests/history.rs` adds the
other two.

Two checks written first were **deleted rather than kept**, on review. Both
were in `src/repl.rs`: one restated the path join, and the other accepted
`None`, which is the defect itself. Neither would have failed against the
code they were written for. That is why both surviving checks below are
controlled against the defect rather than merely passing.

`cargo clippy --locked --all-targets`, run natively here, reports **13
warnings both with these changes and with them stashed**. That is the whole
of the comparison: this change adds no native Windows clippy warning. It is
**not** gate 12, which also wants Linux and both macOS targets still
type-checking and the Linux ladder still passing — none of which has been run
from this machine.

The figure itself wants recording too: gate 1 says eleven, for Linux and for
the cross-checked Windows target alike, and native Windows `--all-targets`
says thirteen. Which two are new to the platform is not established here.

D1's check drives `probe`, the function the fix changed, rather than
reassembling its logic beside it. Controlled: with the two steps inside
`probe` swapped so serde runs first, it fails with `cannot serialize external
references` and names neither the program nor the reason. Both orders return
an error, so what discriminates is asserting on the message.

`the_self_check_is_asked_for_by_name` still fails, which is correct: only one
of D1's two fixes is done. What changed is what it says when it fails —

```text
Error: "exit status: cannot run /bin/sh: The system cannot find the path specified. (os error 3)"
```

— against `cannot serialize external references` before. The outer quotes are
Rust's own `Debug` print of `Box<dyn Error>` from `main`, which every
`selfcheck` failure has always had; it is not this change's doing and is not
fixed here.

**D2 writes history where it should; it has not been asked to recall it.**
`tests/history.rs` runs the binary in a controlled environment — the state
variable pointed at a scratch directory, the others removed, the runner's own
environment untouched — and looks at the filesystem afterwards. Two cases:
history appears under the platform's state directory, and `RNX_HISTORY` names
the file and wins over it.

Both are controlled against the defect. With `state_dir` returning the
pre-decision-7 answer — `HOME/.local/state` — the first fails and the second
still passes, so they are independent.

The control also justified a rule the first draft of these checks did not
have. It fails with **two** complaints, not one: no history under
`LOCALAPPDATA`, *and* "history was written under HOME as well". That second
is the reason `HOME` is pointed at scratch space rather than left alone. On
Linux, where `HOME` is set, a regression that ignored `XDG_STATE_HOME` would
have appended the test's own input to the developer's real history file —
a test damaging a person's data on its way to failing. It now cannot reach
anything outside its own directories.

Two more rules, from the same review: the child is bounded, and a session
that does not end is killed and collected, so a timeout is a failure rather
than a hung suite; and cleanup runs before the assertions, so a failing run
leaves no scratch directories behind.

That is **persistence, not gate 18**. Gate 18 asks that a session writes
history, ends, and a *second* session recalls it; nothing here starts a
second session or reads a line back. The recall half stays unasked.

Deliberately **not** done: D1's native probes, for the reason in the record's
new risk — the interruption probe is `GenerateConsoleCtrlEvent`, which is
decision 4's unrun mechanism and reaches the invoking shell — and the
`capture_bound.rs` split.

## E. Ctrl-C did not reach rnx when it was launched from a parent that ignores it

Found by the first gate written for decision 4, and the reason that gate was
written before D1's native probes rather than after.

`tests/console_interrupt.rs` gives a child a console of its own
(`CREATE_NEW_CONSOLE`, which is the flag documented to make one) and has it
run a script whose `host::process` call waits fifteen seconds on a live
child. The script makes a file as its last act before that call, and rnx's
hook waits for the file before raising — a handshake, so the event arrives
because the call has begun rather than because a guessed delay has passed.

It failed, and the bisection is the useful part:

| Asked | Answer |
| --- | --- |
| `SetConsoleCtrlHandler(Some(handler), TRUE)` | installed, **true** |
| `GenerateConsoleCtrlEvent(CTRL_C_EVENT, 0)` | **ok**, last error 0 |
| The flag, 250 ms later | **false** |
| The call | `cancelled=false timed_out=true`, after its whole deadline |

So the handler was registered, the event was raised successfully into the
console, and the handler never ran. Neither call failed; nothing reported
anything.

**A process that ignores Ctrl-C passes that on to its children, and
installing a handler does not undo it.** `SetConsoleCtrlHandler(NULL, TRUE)`
sets that state; `SetConsoleCtrlHandler(NULL, FALSE)` clears it. rnx only
ever added a handler and assumed delivery.

**Controlled twice, because the first version was not controlled at all.**
Whether the process that starts a test run is already ignoring Ctrl-C is not
something a fixture chooses, so a gate that only ever ran under whatever it
inherited would establish the fix for that one accident. A second case sets
the state deliberately, through `RNX_TEST_IGNORE_CTRL_C_FIRST`, before
anything is installed.

| | restore removed | restore in place |
| --- | --- | --- |
| inherits the runner's state | **passes** | `(true, false)` |
| deliberately ignoring | `(false, true)` at 16.5 s | `(true, false)` |

**The first row changed meaning as the fixture improved, and the change is
itself the measurement.** In an earlier version both rows failed without the
restore, because the runner was ignoring Ctrl-C and rnx inherited it. The
fixture now installs a leak-watching handler in the runner and restores
delivery to itself — and children inherit *that*, so the ordinary case stops
depending on rnx's own restore.

Which is the inheritance claim, measured rather than reasoned: changing the
parent's state changed whether the child received an interrupt, with nothing
else altered. The second row is what the fix is for — a parent that suppresses
Ctrl-C, whatever this particular runner happens to do.

**So the claim is narrower than "Ctrl-C never reached rnx on Windows".** What
is measured is that it did not reach rnx **when launched from a parent that
ignores Ctrl-C**, of which this test runner is one. An interactive console
where Ctrl-C is not suppressed has not been tested, and the earlier heading
here overstated the reach of the evidence.

**Decision 4's mapping is therefore incomplete as written.** It says
`libc::signal(SIGINT, …)` becomes `SetConsoleCtrlHandler`. On Unix,
installing a handler is the whole of it; on Windows it is two steps, and the
missing one is invisible — a run whose parent had disabled Ctrl-C would have
ignored it silently, with record 0020's cancellation contract simply not
holding and nothing saying so. A service, a CI runner, or a non-interactive
shell is enough to produce that parent.

**The order of the two calls is itself load-bearing**, and the first draft of
the fix had it wrong twice. Restoring delivery *before* installing the
handler leaves a window in which an interrupt finds only the default handler,
which Windows documents as calling `ExitProcess`. So does restoring it after
a registration that **failed** — the second draft still called restore
unconditionally. Both trade a contract that silently does not hold for a
process that sometimes dies outright, which is worse. Install; then, only if
that succeeded, restore.

**The isolation watch is itself proved.** The fixture watches for the event
escaping into the test process, and none does — but "no leak" is also what a
detector that could never fire would report, so a third case establishes that
it can. It re-runs the test binary in a console of its own, raises an event
there, and requires the handler to have seen it. Nothing reaches the shell
running the suite, which is why an earlier draft left this unproved and said
so.

What that still does not do is watch every process that might have received a
leak — only this one. `CREATE_NEW_CONSOLE` being documented to isolate is
what covers the rest.

**This is a decision the record owes, not only a patch.** rnx now overrules
an inherited "ignore Ctrl-C". That is defensible — record 0020's contract is
that Ctrl-C ends a call, and a program that cannot be interrupted cannot keep
it — but it is rnx overriding a parent's explicit choice, and that belongs in
the record rather than in a line of `src/platform.rs`. The amendment is not
written here, pending review.

### Gate 6, on the same machinery

Once an interrupt could arrive at all, record 0023's harder half became
askable: it must be reported whether it lands while the call **waits** or
while it **cleans up**. The second is a different moment, not a later one —
the child has already exited, and the readers have been given the allowance
but not yet told to stop.

Nothing outside can time that window, so **four steps in order**, each waiting
on the one before rather than on a clock:

1. rnx writes `RNX_TEST_SIGNAL_CLEANUP_TO` at the instant the allowance is
   published — the one point where the wait is over and the readers are still
   going.
2. The raise waits for that file, then generates the event and requires it to
   have been accepted.
3. It then waits, bounded, for the **handler's flag**. Raising is a request,
   not a receipt: the handler runs on a thread of the console's choosing, so
   `ok=true` says only that Windows took the ask. A first version published
   the release here and could have let the readers finish before the flag was
   ever set.
4. Only then is `RNX_TEST_SIGNAL_RAISED_TO` written, and the readers, held at
   `RNX_TEST_READER_WAITS_FOR`, read their first byte.

So the readers are provably unfinished when the interrupt lands: they had not
begun. A flag that never arrives leaves the file unwritten on purpose, and a
reader that gives up waiting says so on standard error rather than reading
regardless — a reader released by a timeout could finish before a late event
and let the gate pass having asked nothing.

An earlier draft held the readers back a fixed 1,500 ms and called that "still
finishing". It says only that they started late; on an idle machine they could
finish first.

**Controlled twice.**

| Control | What the gate says |
| --- | --- |
| `cancelled \|\| interrupted()` after the cleanup reduced to `cancelled` | `(false, false)` — the interrupt arrived, the call completed, nothing reported it |
| the handler no longer records the flag | "never received, so the readers were never released; a reader gave up waiting and read regardless", then `(false, false)` |

The first is the defect the gate exists for. The second is the gate checking
itself: it establishes that the release really does wait for receipt, and that
a reader's own timeout is reported as a failure rather than absorbed into a
pass.

What remains unasked here is the input side: gate 5, a write blocked on a full
pipe reached by `CancelIoEx`. This gate interrupts a call whose readers are
draining, not one whose writer is stuck.

## F. `rnx selfcheck` runs on Windows, and one probe moved rather than passing

D1's second half. `host::process_checks` now asks each platform's programs
for the same four contracts, with the assertions unchanged except where the
platform's answer genuinely differs. `rnx selfcheck` exits 0.

| Probe | Unix | Windows | Answer |
| --- | --- | --- | --- |
| exit status | `/bin/sh -c "exit 7"` | `cmd /c exit 7` | `code: 7` |
| deadline | `/bin/sh -c "sleep 10 & wait"` | `ping -n 20 127.0.0.1` | `timed_out: true` |
| capture cap | `head -c 3000000 /dev/zero` | `cmd /c type` a file made for it | `[true, 2097152]` |
| interruption | `kill -INT $PPID` | **moved** | see below |

The deadline probe uses `ping` rather than a shell, because nothing about it
needs one. The capture probe needs three megabytes from somewhere and Windows
has no `/dev/zero`, so the self-check writes a file and removes it — through a
`Drop` guard rather than a line after the loop, because the first version left
three megabytes behind on exactly the runs a person would be repeating.
Controlled: with the capture assertion made to fail, `selfcheck` exits 101 and
leaves nothing.

**A contract that reads differently, and the reason.** A deadlined child
reports `code: null` on Unix and `code: 1` on Windows. Unix kills it with a
signal, and a process that died by signal has no exit status at all; Windows
has no signals, so `TerminateJobObject` **is** an exit status, and the 1 is
the one rnx passes it. Both say the child did not choose how it ended, and
`host::process`'s own description already tells a caller to read `timed_out`
and `cancelled` before `code`.

**Normalising the two to `None` was considered and rejected.** It would make
the platforms read alike, and it would be wrong: `cancelled` can arrive during
the cleanup, *after* a child has exited normally with a status of its own —
which is exactly what gate 6 arranges. Reporting `None` there would erase a
real exit code the child chose, in order to tidy a field the caller has
already been told to read second. The difference stays, and it stays
documented.

**The interruption probe is moved, not skipped.** Its Unix form has a child
send `SIGINT` to its parent, reaching that process and nothing else. The
Windows counterpart is `GenerateConsoleCtrlEvent`, which reaches every process
sharing the console — for someone who has just typed `rnx selfcheck`, their
own shell and everything running in it. A self-check that interrupted the
terminal it was invoked from would be a worse defect than any it could find.

So the contract is asked where it can be asked safely: gates 19 and 6, in a
console created for the purpose. `selfcheck` prints the reason in place of the
probe.

**This is not decision 5's case, and calling it that would be wrong.** An
escaped descendant is *unreachable* on Windows — the question cannot be put at
all, and gate 10 records why. An interruption is perfectly reachable, and
gates 19 and 6 put it. What moved is only **where it is asked**, because a
self-check runs in the operator's own console and this question cannot be
asked there without answering it for everything else in that console too.

That makes it a **scope change to gate 17**, which the record has to carry
rather than an evidence file: `rnx selfcheck` on Windows asserts three of the
four contracts the Unix one does, and the fourth is asserted elsewhere. Gate
17 is not closed until the record says so.

## G. Gate 10 — the breakaway is refused, so the escaped case is unreachable

Decision 5 argues a Windows descendant cannot leave its job, and that this is
a **stronger** guarantee than the Unix one rather than a missing test. Until
Windows said so, that was an argument. `tests/job_breakaway.rs` asks it.

The attempt has to happen **inside** the job, so the fixture is three deep:
the test runs rnx, rnx runs the test binary again through `host::process`
— which is what assigns it to a job — and that copy attempts
`CREATE_BREAKAWAY_FROM_JOB`. The supplied direct-run measurements on
2026-09-10 report the outside control first:

```text
IN_JOB true
BREAKAWAY_OK permitted
PLAIN spawned
BREAKAWAY spawned
```

Inside rnx's job, the same executable and arguments report:

```text
CALL code=0 timed_out=false cancelled=false
IN_JOB true
BREAKAWAY_OK forbidden
PLAIN spawned
BREAKAWAY refused Some(5)
```

The inner observations establish job membership, limits without
`JOB_OBJECT_LIMIT_BREAKAWAY_OK`, a working spawn without the flag, and
`ERROR_ACCESS_DENIED` with it. The clean call outcome establishes that the
helper completed. The outside control is necessary: a refusal alone cannot
distinguish rnx's policy from an enclosing job's restrictions. Here the
control succeeds even though the runner is itself in a job.

**The launch path matters.** The supplied comparison from the same shell is:

| Runner | `IN_JOB` | `BREAKAWAY_OK` | Breakaway attempt |
| --- | --- | --- | --- |
| `cargo test` | true | forbidden | `Err(Some(5))` |
| compiled test run directly | true | permitted | spawned |

A fresh Start-menu PowerShell running `cargo test` also reproduced the
blocked control. These measurements identify the Cargo launch path as the
obstruction on this machine; they do not require assuming that every shell
or every Cargo version has the same job configuration.

**Keep the gate failing when the control cannot be drawn.** It must not pass
on four inner observations alone. A normal Cargo run therefore retains this
environmental failure, and the direct run supplies Gate 10's evidence
separately. The diagnostic prints the executable's direct-run command.

Build and locate the binary without hardcoding its hash, then run it outside
Cargo (no `test-support` feature is required):

```powershell
$buildMessages = cargo test --test job_breakaway --no-run --message-format=json
if ($LASTEXITCODE -ne 0) { throw 'Gate 10 build failed' }
$gateBinaries = @($buildMessages | ForEach-Object {
    $message = $_ | ConvertFrom-Json
    if ($message.reason -eq 'compiler-artifact' -and
        $message.target.name -eq 'job_breakaway' -and
        $message.profile.test -and $message.executable) {
        $message.executable
    }
})
if ($gateBinaries.Count -ne 1) { throw 'Expected one Gate 10 test binary' }
& $gateBinaries[0] --nocapture
```

If direct execution also refuses the outside breakaway, the control remains
blocked by that environment. Being in a job alone is not a reason to fail;
the test requires the outside attempt to succeed.

**What this licenses.** The five `capture_bound.rs` cases that fail here all
use `setsid` to put a descendant outside the process group and have it hold a
pipe open. That descendant is what Windows will not permit. So those cases can
now be marked inapplicable **with this as the reason**, in decision 5's shape,
rather than skipped — and, per the same decision, the capture contracts they
also carry must still be asked natively, by fixtures whose descendants stay
inside the job.

## What the record should add

Not decided here — recorded as what the measurements ask for:

1. **Four fixes, kept distinct.** Three are made: native probes in
   `process_checks` (section F); the original process error preserved before
   serialisation; and a Windows answer for the history location. The fourth,
   the fixture adaptations of category A, is not.
2. **`rnx selfcheck` succeeding on Windows becomes an acceptance gate.**
   Adopted — it is gate 17. Section F closed it for three probes and moved the
   fourth elsewhere, which the gate's wording has to carry.
3. **Gate 12's baseline** moves to 232 and 237.
4. **Gate 11's command** is `cargo install --locked --path .`.
5. **Gate 9** records surface 6 as unreachable with its reason, in decision
   5's shape.
6. **`tests/repl.rs`** is a gap, not an exclusion. D2 is what it cost.
7. **Gate 10's evidence names its launch path.** Passed by direct execution of
   the compiled test; under Cargo the outside control is blocked on the
   measured machine, and the gate fails rather than passing without it.

## What this does not establish

Gates 3, 4, 5, 8, 15 and 16 have **no Windows fixtures**. Gate 6 has one
(section E), gate 10 has one (section G), and gate 7's hardest pair has one
(`tests/capture_bound_windows.rs`) — the rest of gate 7 still rides on
category A's Unix-shaped fixtures. `RNX_TEST_JOB_ASSIGNMENT_FAILS` is wired at
`src/platform.rs:374` and nothing under `tests/` references it.

Named as behaviours rather than as mechanisms, because some of the code has
now run here while none of these has been observed:

- A descendant is inside the job before the child executes an instruction, so
  one spawned immediately at startup is still ended with it.
- A write blocked on a full pipe returns when the call is cancelled, and the
  writer is collected.
- `GetConsoleMode` distinguishes a console from input redirected from `NUL`.
- A failed job assignment leaves no suspended process behind.

Three that this list carried until now have since been observed, each in its
own section: a cancellation arriving during cleanup is reported as `cancelled`
(E, gate 6); an interrupt reaches the process at all, including from a parent
that suppressed it (E, gate 19); and a child attempting
`CREATE_BREAKAWAY_FROM_JOB` is refused (G, gate 10).

**Gate 10's answer has a launch path attached.** It is answered by running the
compiled test directly. Under `cargo test` the outside control cannot be
drawn — Cargo's own job forbids breakaway on the measured machine — and the
gate fails there rather than passing on its four inner observations alone.
That failure is a blocked control in one environment, not a defect in rnx and
not a property of every environment; a runner whose enclosing job permits
breakaway would draw the control and pass.

**Gate 2 is not closed.** The ladder has run and its result is recorded above.
That is the baseline, not the acceptance: gate 2 wants both suites passing
with the fixtures adapted, and the failures besides gate 10's blocked control
are work to resolve rather than a settled floor.

**Gate 12 is pending.** `src/host.rs`, `src/platform.rs` and `src/repl.rs`
have all changed since anything ran on Linux, and only
`x86_64-pc-windows-msvc` is installed on this machine, so neither the Linux
run nor the macOS type-checks can be done from here.

> **Answered on Linux at `0ec0606`.** 235 tests and 240 with `test-support`,
> zero failures; clippy at its eleven; `fmt --check` clean; and no errors or
> warnings from `cargo check --locked --all-targets`, with and without
> `test-support`, for `x86_64-pc-windows-msvc` and both macOS targets. Gate 12
> in the record carries the numbers. Two harness defects found in that review
> — an unbounded cleanup and output-read failures reported as empty output —
> were repaired on Linux and are noted where each was found; their Windows
> behaviour needs a native rerun.

What **has** run natively is the shared protocol — the 98 unit tests include
decision 3's seven ordering gates — a `selfcheck` asking three of its four
probes, section E's two interrupt gates, gate 10's refusal, and gate 7's
`truncated` with `cut_short`.

Both handoffs read as "run the ladder". Enough of the Windows half has now run
that the useful distinction is no longer written-versus-run but asked-versus
-unasked, which is what the list above separates.

Two contracts are **unanswered rather than failing**, because their fixtures
die before the hook they drive is reached: a reader that panics does not
strand the other (`RNX_TEST_READER_PANICS`), and a delivery failure after
cleanup begins still reaches the script (`RNX_TEST_DELIVERY_FAILS`). Adapting
those fixtures is what turns them into answers.

The unit tests are the first native run of decision 3's protocol gates. That
is the protocol asking correctly; it is still not evidence about what
`CancelIoEx` does to a write blocked in the Windows kernel. Gate 5 remains
gate 5's job.

Three things carried forward, so that none of them reads as settled:

1. **D1 is done, and gate 17's scope moved with it.** Error preservation is
   fixed and controlled, and the native probes are written — section F. The
   interruption probe is not among them: it is asked by gates 19 and 6
   instead, in a console the gate owns. That is a scope change to gate 17,
   not a probe that quietly passed.
2. **Gate 18 is half asked.** History is written and its contents checked. No
   second session recalls it, which is the half the gate is actually about.
3. **Nothing here has run on Linux.** Everything above is one machine's
   answer. `tests/history.rs` is the first file this port adds that is not
   `cfg`-gated, so it runs there too — and it is the one that manipulates
   `HOME`, which makes a Linux run worth more than any reasoning about it
   from here.
