# rnx 0025: the same contracts on Windows

Status: implemented on Windows, review pending 2026-09-10. The twenty-fifth record of rnx. Record 0001's
release gate wants a child cancelled, deadlined and cleaned up on Linux,
macOS and Windows. On Windows the code does not compile, let alone run. This
record decides how each contract records 0020 through 0023 established is met
there, and says which of them Windows cannot be asked the same question.

The original design and pre-port measurements below are retained as history.
Native implementation and acceptance results are recorded in
`0025_the_same_contracts_on_windows_evidence.md`; its closeout section
supersedes the earlier lists of unanswered Windows gates.

The seam is written and compiles; three rounds of review of it are folded
into decisions 2, 3, 4 and 6 below, with what each defect would have cost.
Four of the eight findings were on one mechanism — how a call reaches a
worker inside an operation — and decision 3 is written as that sequence,
because the wrong answers are the useful part. Those design-review paragraphs
predate the native Windows evidence.

## Context

Measured here, not assumed:

- `cargo check --target x86_64-pc-windows-msvc` fails with **18 errors** in
  twelve places, all of them in `host.rs`: `libc::kill`, `libc::signal`,
  `libc::fcntl`, `libc::isatty`, `libc::c_int`, `libc::sighandler_t`,
  `std::os::unix::process::CommandExt`, and `std::os::fd::AsRawFd`. There is
  no `cfg(windows)` anywhere in the crate and no Windows dependency.
- `cargo check --target aarch64-apple-darwin` and `--target
  x86_64-apple-darwin` both pass with **no errors** — the crate alone; with
  `--all-targets` a fixture does not, which gate 1 records. That is the whole of
  what it establishes: the code type-checks. It does not establish that a
  process group behaves the same way, that a signal arrives the same way, or
  that any of the fixtures below run — those are questions for the machine,
  and macOS is **unverified**, not assured.

So Windows is not a porting chore with a testing tail. It is an
implementation, and three of the four mechanisms it needs are load-bearing
for the lifecycle records 0020 to 0023 just closed.

## Decision

### 1. The contracts are the specification, not the calls

What must hold on every platform, in the words the records already use:

| Contract | Where it came from |
| --- | --- |
| A child is ended at its deadline and reports `timed_out` | 0016, 0020 |
| …and `code` says what the **child** chose, which is why a child rnx ended cannot say it — see below | 0013, 0016 |
| Ctrl-C ends a call and reports `cancelled`, while it waits **and** while it cleans up | 0020, 0023 |
| A child's descendants are ended with it, so nothing is left behind | 0016 |
| The input writer stops when the call does, and is always collected | 0022 |
| Both readers stop when the call does, and are always collected | 0023 |
| A capture reports `truncated`, `cut_short` and `unreadable` independently | 0023 |
| `host::stdin` refuses a terminal | 0012 |
| Nothing rnx prints can move a cursor | 0019 |

Each is a behaviour a test can ask about. The Windows work is whatever makes
each answer the same, and where an answer cannot be the same, this record
says so rather than a gate quietly weakening.

**One answer cannot be the same, and this is it.** A child rnx ended at its
deadline reports no exit status on Unix — a signalled process has none, so
`code` is empty — while on Windows `TerminateJobObject` **is** an exit status
and `code` is the 1 rnx passed it. That 1 is indistinguishable from a child
that chose to exit 1, where the Unix emptiness is unmistakable.

The contract underneath is unchanged and is record 0013's: `code` says what
the child chose, and a child that did not choose has nothing to say. What
differs is only whether the platform lets that be represented. `timed_out`
and `cancelled` are the unambiguous answer on both, which is why every
description of the three process functions now ends by saying to read them
first, and why the ports already do — record 0023's "cause before symptom"
rule reached this before the platform did.

The self-check asserts both forms rather than one: `process_checks` splits on
`cfg` at that assertion, so a platform reporting the other's answer fails.

### 2. A job object, assigned before the child can spawn anything

`process_group(0)` becomes a job object: create it, set
`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` so a parent that dies takes the job with
it, assign the child, and end everything with `TerminateJobObject` where the
Unix path sends `SIGKILL` to the group.

**Assignment must happen before the child runs a single instruction.** Spawn
and then assign is a race: a child that spawns a descendant in that window
leaves it outside the job, and the descendant survives the call — which is
precisely the leak the Unix path does not have. So the child is created
**suspended** (`CREATE_SUSPENDED` through `CommandExt::creation_flags`),
assigned to the job while it cannot act, and only then resumed.

Resuming needs the child's initial thread, which `std::process::Child` does
not hand over — it exposes the process handle alone. Two ways to get it, and
the record chooses the first:

- **Enumerate the child's threads** with the toolhelp snapshot
  (`CreateToolhelp32Snapshot`, `Thread32First`/`Thread32Next` filtered by
  owner process id), open the one thread a just-created process has, and
  `ResumeThread` it. Contained, and everything else stays `std::process`.
- Call `CreateProcessW` directly and take the thread handle from
  `PROCESS_INFORMATION`. Cleaner in principle, but it means reimplementing
  argument quoting, pipe creation and inheritance, and record 0016's rule
  that a child is never run through a shell would have to be re-established
  by hand rather than inherited.

If the toolhelp route proves unreliable in practice — a snapshot that races a
just-created process, say — that is a measurement, and the second option is
the recorded fallback rather than an improvisation.

**A suspended child is nobody else's to clean up.** Two corrections from
review, both on this path:

- Once the child exists, every failure between there and the resume must end
  **the child**, directly. The first draft ended the *job* on an assignment
  failure, which reaches nothing: a child that never joined the job is not in
  it, and would have been left suspended for as long as the machine ran. So
  that path now kills and collects the child, and reports the original reason
  rather than whatever the cleanup returned.
- Finding and opening a thread does not establish that it is running.
  `ResumeThread` answers the thread's **previous** suspend count: `u32::MAX`
  is a failure, and any answer above one leaves the thread still suspended.
  So it is called until the count says the thread is running or the call says
  it cannot be, and a failure abandons the child as above rather than
  returning a child that will never run.

### 3. Both directions of a pipe, because readiness is not cancellation

The Unix path makes the pipes non-blocking and reads a flag before every
attempt. Windows pipes made by `std::process` are not overlapped, so
`O_NONBLOCK` has no equivalent and each direction needs its own answer:

- **Reading**: `PeekNamedPipe` before each `ReadFile` says whether bytes are
  waiting, so the reader never enters a blocking read it cannot leave. The
  wait between attempts is record 0020's, unchanged.
- **Writing**: `PeekNamedPipe` says nothing about a write. A `WriteFile` into
  a full pipe blocks, and no flag the writer reads afterwards can help,
  because the writer is inside the kernel. So the parent calls
  **`CancelIoEx`** on the write handle at the moment it publishes the stop —
  the same moment records 0022 and 0023 publish theirs — and the blocked
  write returns `ERROR_OPERATION_ABORTED`, which the writer reports as
  abandoned rather than failed.

**This is the correction that shapes the design.** An earlier reading of the
problem had `PeekNamedPipe` covering both directions; it covers one. A write
is cancelled from outside or not at all, and that makes the stop flag a
*signal to the parent to cancel*, not only a flag for the worker to read.

`CancelIoEx` reaches a read the same way, so a reader between attempts and a
reader inside a read are both stopped by the same request — but **when** that
request is made is decision 6's, not "when the call ends": a reader is given
the cleanup allowance to reach the end of its stream first.

**How a parent reaches an operation without owning the handle.** The first
draft kept a copy of the worker's raw handle and cancelled it whenever the
worker was unfinished. Two rounds of review found four problems with reaching
a worker at all, none of them a matter of timing, and the protocol is what
answers all four:

- **A status check cannot protect a handle.** A worker can finish, and close
  its handle, immediately after `is_finished()` answers. A closed handle's
  value may already name something else, so the cancellation would land on
  whatever that is. Sharing ownership instead is worse, and was tried: a
  write end the parent still holds is a write end the child never sees the
  end of, and a child that reads to EOF waits for ever. It hung two fixtures
  for as long as they were allowed to run.
- **An operation must not be declared after a stop.** A worker that passes a
  stop check, and declares an operation behind a stop already decided, is a
  worker the call has no way left to reach.
- **A cancellation must outlast the submission it is aimed at.** Declaring an
  operation and submitting it cannot be one step: the lock cannot be held
  across a blocking write, or nothing could cancel it. So a single
  cancellation can land in the gap between the two, and Windows documents
  `CancelIoEx` as affecting operations already **outstanding** and as
  answering `ERROR_NOT_FOUND` when there are none — the ask is simply lost,
  and the write blocks afterwards with no-one asking again. A second review
  found this in the draft that fixed the first two, and a third review found
  it again in the fix: asking repeatedly but giving up after a second is the
  same defect with a longer gap. It is the one that would have hung a call
  rather than degraded it, because the parent joins that writer.
- **Unwinding must withdraw the handle.** A worker that dies between
  declaring and finishing drops its stream on the way out. A matching `end()`
  call is not reached by an unwind, so the handle stayed published and the
  next ask would have named a closed handle — arriving at the very defect the
  first problem describes, by the one route a lock does not cover. The
  poisoned mutex was no help: the state it preserved was the wrong state.

So, in the shape that answers all four:

- The worker keeps its stream and **publishes the handle for exactly as long
  as it is inside an operation**. `begin` checks and declares in one step
  under the lock, and hands back a **guard**; the guard withdraws the handle
  when it dies, at the end of the operation or on the way out of a panic.
  Nothing else can withdraw it, so there is no path that forgets to.
- `stop` sets the flag and then **keeps asking for as long as an operation is
  declared**, from a hundred microseconds between asks up to five
  milliseconds. Whatever the worker was doing when the first ask arrived, a
  later ask finds the operation outstanding and cancels it; the guard
  withdraws the handle; the loop ends. It answers one of two things: nothing
  to reach, or reached.

**There is no cutoff, and the draft that had one reopened the race.** It gave
up after a second and answered "still inside", which was recorded as meaning
cancellation was not working. It does not mean that. A worker descheduled
between declaring and submitting can spend that second there; every ask finds
nothing outstanding; the call gives up; and the worker then submits a write
with nobody left to cancel it. No length of timeout tells those two apart,
because the observation is the same in both.

Nor was that bound buying boundedness. What follows a `stop` is the worker's
collection, which waits for the **same** event — the operation ending —
without asking for it. Giving up early shortens nothing and removes the only
thing that would end the operation, which is how a bound turned into an
indefinite join. So the responsibility is held until the operation withdraws,
and what makes that terminate is the platform: an operation cancelled after
submission ends. If one ever neither returns nor can be cancelled, the loop
and the join wait alike, and that is a platform defect a timeout would hide
rather than fix.

The lock guards two assignments, and is held across neither an operation nor
a wait.

### 4. Ctrl-C, and what a terminal is

`libc::signal(SIGINT, …)` becomes `SetConsoleCtrlHandler`. The handler runs on
a thread of the console's choosing, which suits the existing design exactly:
it sets an atomic flag and returns, and every waiter and worker reads that
flag. `CTRL_C_EVENT` and `CTRL_BREAK_EVENT` both set it.

**Amended on the machine: installing a handler is not enough.** The first
Windows gate written for this decision found the flag never set, with the
handler registered and the event raised, both reporting success. A process
that ignores Ctrl-C passes that state to its children, and adding a handler
does not clear it — the handler is registered and simply never runs.
`SetConsoleCtrlHandler(NULL, FALSE)` is what restores delivery. On Unix
installing a handler is the whole of it; here it is two steps, and the missing
one fails silently, which is why nothing had reported it.

So this decision now has three parts, and the second and third are as
load-bearing as the first:

1. **Install the handler**, as above.
2. **Restore delivery afterwards, and only if the installation succeeded.**
   Both the other orders leave a window in which an interrupt finds only the
   default handler, which Windows documents as calling `ExitProcess`:
   restoring first opens it between the two calls, and restoring after a
   *failed* registration opens it for good. A process that cannot be
   interrupted keeps running and breaks record 0020's contract quietly; one
   that takes `ExitProcess` in the middle of a call loses the call's work and
   its status. The first is the better failure, so a registration that fails
   leaves delivery alone.
3. **rnx overrules a parent that suppressed Ctrl-C.** This is the part that is
   a choice rather than a correction. A parent may have disabled Ctrl-C
   deliberately, and rnx re-enables it for itself. Record 0020 says an
   interrupt ends a call and reports `cancelled`; a program that cannot be
   interrupted cannot keep that, and a contract that holds only when the
   launching environment happens to permit it is not one this record can
   claim.

   **The scope, precisely.** rnx changes its own delivery. It does not change
   the parent, or any process that already exists, or the console itself —
   but the setting **is inherited by children rnx creates afterwards**, which
   is the same mechanism that carried the suppression into rnx to begin with.
   So a child rnx spawns is interruptible too, whatever the state rnx was
   handed.

What that costs, said rather than implied: a service or CI runner that
suppressed Ctrl-C for everything it starts will find rnx, and rnx's children,
interruptible anyway. That is the intended reading of record 0020, and if it
is ever the wrong one, this decision is what changes.

What it does **not** do is give rnx an interrupt where there was never a
console to deliver one. Restoring delivery is not creating a console: a
process launched without one — a service proper, rather than a runner with a
console that suppresses Ctrl-C — has nothing to restore, and both calls fail
harmlessly. Record 0020's contract is unreachable there for a reason no
decision here can address.

`libc::isatty(STDIN_FILENO)`, which is how `host::stdin` refuses a terminal,
becomes a **console-specific** question. Record 0012's rule is unchanged — a
terminal is refused because nobody is going to send an end-of-file — and only
the question changes.

The first draft asked `GetFileType`, and `FILE_TYPE_CHAR` was the wrong
answer to take for "console": it covers any character device, `NUL` among
them. A script run with its input redirected from `NUL` has an input that
ends immediately, and would have had it refused as a terminal. So the
question is `GetConsoleMode` on the standard input handle, which succeeds
only for a console.

### 5. One question Windows cannot be asked the same way

The escaped-descendant fixtures use `setsid`, which has no counterpart:
**a Windows child cannot leave its job unless the job permits it.** With
`JOB_OBJECT_LIMIT_BREAKAWAY_OK` unset — which is the default and what this
record keeps — a descendant cannot escape, so the case records 0022 and 0023
were built around is **unreachable rather than handled**.

That is a stronger guarantee, and it must not be reported as a passing gate
for the same test. The Windows ladder therefore asserts the *reason*: that a
child which tries to break away fails to, and that the job's limit says so.
The gates that need an escaped holder are marked not applicable **with that
evidence**, rather than skipped.

**Amendment after the native fixture pass: the escaped lifetime is the
unreachable premise, not the worker or capture contract.** A Windows job can
end every descendant and still leave rnx with buffered output, a worker
scheduled late, a panicking reader, or a delivery result it must collect.
Gate 10 does not answer any of those questions. The terminal-filename gate's
refusal pattern therefore applies to the attempt to escape, not to an entire
test that also asserts a portable result.

The remaining fixtures are separated as follows. "Existing" names code that
already asks the native question; "to write" is acceptance work, not a pass.
The Unix escaped-holder fixtures remain on Unix. They may become
`cfg(unix)` only alongside the named native coverage and the recorded Gate
10 result; no whole file is excluded to dispose of its portable assertions.

| Current fixture | Windows question and disposition |
| --- | --- |
| `capture_bound::a_held_reply_pipe_no_longer_holds_the_call` | **Native replacement passes:** `capture_hold_windows::a_held_reply_pipe_no_longer_holds_the_call_windows` runs all four rows with an in-job grandchild that inherits the pipes it keeps. Which pipes it had is measured rather than assumed: it writes a marker to each stream it kept, and the case asserts the exact captured lengths alongside `timed_out=true`, `cancelled=false`, prompt return, and the ends of both the holder and the grandchild by owned handle while rnx is still alive. A `TerminateJobObject` mutation fails it on the first row. The escaped-holder fixture stays `cfg(unix)`. |
| `capture_bound::a_child_that_exits_while_a_pipe_is_held_returns_after_the_cleanup` | **Native lifecycle replacement passes:** `capture_hold_windows::a_normal_exit_ends_the_in_job_pipe_holder_during_cleanup` opens both processes while alive, then releases the direct child to exit 7. It observes that chosen status, no timeout or cancellation, exact grandchild markers on both pipes, and both processes terminated while rnx remains alive. With `test-support`, a 12-second cleanup allowance makes an omitted explicit job end fail the 8-second return bound rather than be hidden by job-handle drop. The escaped-holder fixture is now `cfg(unix)`. Killing a holder does not establish `cut_short`; the existing cap-hold gate tests that flag, and the uncapped-prefix row below now passes with a hold after seven captured bytes. |
| `capture_bound::the_three_shortfalls_are_independent` | **Split:** `the_size_cap_alone_does_not_cut_a_capture_short` now runs the portable cap-only half independently. The escaped-holder half remains Unix-specific. **Existing:** `capture_bound_windows::a_stream_past_the_cap_is_also_cut_short_when_the_call_ends` asks for `truncated=true`, `cut_short=true`, `unreadable=false` after a normal child exit using its cap handshake; its pipe-capacity prerequisite remains explicit. |
| `capture_bound::a_descendant_that_never_stops_writing_does_not_hold_the_call` | **Native replacement passes:** `capture_hold_windows::a_descendant_that_never_stops_writing_does_not_hold_the_call_windows` gives an in-job grandchild an endless `for /l` echo loop and asserts prompt return, `timed_out=true`, `cancelled=false`, `unreadable=false`, the ends of both processes by owned handle while rnx stays alive, and more than a pipeful captured, so the reader is known to have gone round its loop rather than taken one bufferful. A `TerminateJobObject` mutation fails it on exactly its own clause: the call does not return inside the bound. **The capture flag differs from Unix and the difference is asserted, not skipped:** `cut_short=false` here, because the job ends the writer, its write end closes, and the reader drains to the end of the file inside the allowance. Unix reports `cut_short=true` because its writer survives the group and never stops. That difference is the row's own point that the job test is not evidence for surviving-writer behaviour. |
| `capture_bound::a_completeness_check_can_see_a_prefix_that_truncation_cannot` | **Native replacement passes under test-support:** `capture_bound_windows::a_completeness_check_can_see_a_prefix_that_truncation_cannot_windows` captures exactly `partial`, acknowledges seven bytes, and holds stdout before its next read. The child waits for that acknowledgement before exiting normally. The test releases the reader only after cleanup publishes its stop, and asserts code 0, no timeout or cancellation, `truncated=false`, `cut_short=true`, `unreadable=false`, and empty stderr. A hook timeout fails. The original escaped-holder fixture is now `cfg(unix)`; the Windows replacement requires the test-support reader hook. |
| `capture_bound::an_interrupt_during_the_cleanup_is_still_an_interrupt` (`test-support`) | **Existing:** `console_interrupt::an_interrupt_during_the_cleanup_is_still_reported` owns its console and orders cleanup, event receipt, and reader release. Its result now asserts cancellation, absence of timeout, and `cut_short=true`. The escaped-holder and SIGINT mechanism stays Unix-specific. |
| `child_input::a_deadline_ends_the_call_while_delivery_is_blocked` | **Native replacement passes:** `delivery_deadline_windows::a_deadline_ends_a_blocked_delivery_and_the_writer_is_collected` feeds a mebibyte to an in-job `ping` that never reads, and establishes the block with `GetThreadIOPendingFlag` on the identified delivery thread rather than from the size of the input. It asserts the child still alive while the writer is blocked, `timed_out=true` with `cancelled=false`, the deadline ending the delivery promptly, and refusal to report while the writer is deliberately held before thread exit. An omitted-join mutation fails it. The escaped-holder fixture is now `cfg(unix)`. Note the cleanup order: `group.end()` precedes `writing.stop()`, so on this path the child is already gone when the delivery ends, and the unblocking is not attributable to the writer's stop — `pipe_cancel_windows` is what attributes that. |
| `child_input::a_cancellation_ends_the_call_while_delivery_is_blocked` | **Native replacement passes:** `console_interrupt::cancellation_collects_a_writer_blocked_on_child_input` observes pending I/O on the identified delivery thread in an owned console before raising the event. It asserts `cancelled=true`, `timed_out=false`, prompt return, writer and child termination while rnx stays alive, and refusal to return while the writer is deliberately held before thread exit. An omitted-join mutation fails. The independent real-I/O control passes in `pipe_cancel_windows`; the escaped-holder fixture is now `cfg(unix)`. |
| `child_input::the_delivery_thread_is_gone_before_the_call_returns` | **Native replacement passes:** `delivery_exit_windows::a_normal_child_exit_collects_the_unfinished_writer` observes pending I/O before allowing a non-reading child to exit 0. The worker is held after delivery ends; the call stays silent until it is released. Retained Windows handles then establish writer and child termination while rnx remains alive, and both the child's exit status and the call's `(code, timed_out, cancelled)` establish normal exit. Omitting the join fails the control. The escaped-holder fixture and its `/proc` observation are now `cfg(unix)`; Gate 10 supplies the applicability evidence. |
| `delivery_failure::an_unreadable_stream_is_reported_and_excuses_nothing` (`test-support`) | **Implemented portable substitution:** retain `RNX_TEST_CAPTURE_FAILS` and use a native child. Assert `unreadable=true`, `truncated=false`, and zero captured bytes. The shared decoding unit gates supply the malformed-tail exemption checks; this integration test does not generate a malformed tail. |
| `delivery_failure::a_failure_after_cleanup_begins_still_reaches_the_script` (`test-support`) | **Native replacement passes:** `delivery_failure_windows::a_failure_after_cleanup_begins_still_reaches_the_script_windows`. Two hooks were added for it: `RNX_TEST_WRITER_HOLDS_BEFORE_STOP` holds the writer once, at the top of its loop and before it looks at its stop flag, and `RNX_TEST_SIGNAL_WRITER_STOP_TO` publishes the writer's stop between `writing.stop()` and the collection — the readers' `announce_the_stop` fires after the delivery is joined, so waiting for that one would have waited on what the held writer prevents. The gate asserts exit 1, the injected words on standard error, no success line, and no hook timeout. Controlled twice: removing the injection consult makes it report success, and releasing the writer before the stop is published does the same, so the ordering is load-bearing rather than decorative. The escaped-holder fixture is now `cfg(unix)`. |
| `delivery_failure::a_reader_that_panics_does_not_strand_the_other` (`test-support`) | **Native replacement passes:** `reader_panic_windows::a_reader_that_panics_does_not_strand_the_other_windows` opens and retains the held stderr worker's Windows handle before releasing stdout to its injected panic. It observes the panic and cleanup stop, requires silence while stderr remains alive, then releases stderr and observes its termination while rnx stays alive after catching the exact `stdout reader panicked` error. Hook timeouts fail. Propagating stdout's join error before joining stderr fails the held-worker clause. The escaped-holder fixture is now `cfg(unix)`. |

**Gate 5 needs two observations.** In the current `run_child` cleanup,
`group.end()` precedes `writing.stop()`. On Windows, closing the last reader
when the job dies can end a blocked write without any `CancelIoEx`. The
original gate's claim that a flag-only implementation could not pass an
end-to-end cancellation test was therefore too strong.

Keep that end-to-end test for the public result and lifecycle. Add a native
mechanism gate around the actual `Pipe::begin` / `Pipe::stop` and synchronous
write: a fixture owns both pipe ends, keeps the read end open without
draining, and submits enough input to block. The stop must end the operation
with `ERROR_OPERATION_ABORTED` while that read end remains open, and the
worker must be joined. An announced intention to write is not proof of a
pending operation; the aborted-I/O result supplies that distinction. A
negative control without cancellation must stay unfinished until the
fixture releases the pipe. Run this inside an independently bounded helper
process so a broken cancellation cannot hang the suite. This is a controlled
pipe fixture, not a claim that a Windows descendant escaped its job.

New scheduling hooks are confined to `test-support`, with a no-op ordinary
build. Each announces the phase it reached, has a bounded wait, reports a
timeout as a failure, and is released during teardown before assertions.
The collection gates must fail under the corresponding omitted-join
regression; a worker that happens to finish quickly must not make a detached
worker look collected. Windows process/thread handles must remain owned
through the observation so identifier reuse cannot supply the answer.

**Gate 10 keeps its outside control.** One attributable refusal establishes
the common premise; repeating it under every Unix test name would inflate
the native evidence without testing more contracts. Cargo's restrictive job
still causes an explicit control failure on this machine. Record that
failure and run the compiled gate directly as section G prescribes; a direct
pass supplies Gate 10's result, not a claim that the Cargo suite passed.
If the direct launch also blocks the control, the applicability evidence is
pending. No assertions are weakened to obtain a green count.

### 6. Draining comes before reaching, on every platform

Record 0023 gave a call one deadline and a bounded cleanup allowance, and
promised that what was in the pipes when the child exited would be read. The
first draft of this seam broke that promise on the way past: it published the
cleanup instant and then cancelled the pending reads immediately, on an
ordinary exit as much as on an interrupt. A cancelled read is reported as
`cut_short`, so output that was sitting in a pipe — up to the pipe's capacity
of it — would have been discarded and the loss reported as the call's own
doing.

The order is therefore fixed, and it is the same order on both platforms:

1. Publish the shared cleanup instant, which is what stops a descendant that
   keeps writing, and reach nobody.
2. End the group, and stop and collect the writer, which has nothing left to
   deliver that anyone wants.
3. Wait until both readers have finished **or** the instant passes.
4. Reach only a reader still going when it passes.

A cancellation is not a special case in that list: it is an allowance of
zero, so the instant has already passed and step 4 reaches both readers at
once. Which is what makes Ctrl-C prompt and an ordinary exit complete,
without two mechanisms.

### 7. Where a session's history lives, on a platform without `HOME`

Added after the machine arrived, because the ladder's first run found it and
no gate above would have. `repl::history_path` reads `RNX_HISTORY`, else
`XDG_STATE_HOME`, else `HOME/.local/state`, and answers `Option`. On Windows
none of the three is set — `USERPROFILE` and `LOCALAPPDATA` are — so it
answers `None`, and both its consumers are `if let Some(path)`. Loading and
appending are skipped **silently**: a session keeps no history between runs
and nothing says why.

Record 0024 makes a session the default command, so this is the default
command's own behaviour, on the platform where the suite that covers it does
not run.

The decision, in the shape of the others here — the mechanism splits, the
contract does not:

- **`RNX_HISTORY` remains the explicit override, on every platform.** It is
  asked first and answers alone.
- **On Windows the base is `LOCALAPPDATA`**, so history lives at
  `%LOCALAPPDATA%\rnx\history`. `LOCALAPPDATA` is what `XDG_STATE_HOME`
  names on Unix: per-user, machine-local, not roamed, and set by the system
  rather than by a shell.
- **Unix is unchanged.** `XDG_STATE_HOME`, then `HOME/.local/state`.

`None` stays possible — `LOCALAPPDATA` can be unset in a service context —
and it keeps meaning "no history file". What this record does not decide is
whether that silence should stay silent; it is a diagnostic question, and
record 0019's surfaces are where it would belong.

### 8. What this record does not decide

macOS. Its code compiles today and its behaviour is unverified; the ladder
there is a run on the machine, and the fixtures need adapting — `/proc` is
absent, so the thread counts records 0022 and 0023 observe need a different
source, and `setsid` exists but process-group semantics want confirming
rather than assuming. That is its own record, written when the machine is
here.

Release metadata, which is separate and small, and the license, which record
0001 blocks on provenance.

## Acceptance gates

Every gate below runs on the Dell. Nothing here is claimed until it does.

1. **It compiles — met, before the machine arrived.**
   `cargo check --locked --all-targets` with and without `test-support`, for
   `x86_64-pc-windows-msvc` and both macOS targets: **no errors and no
   warnings**, from eighteen errors. Linux clippy is at the same eleven
   warnings it carried before, and so is `cargo clippy` for the Windows
   target. Linux passes 226 tests and 231 with `test-support`, the eight new
   ones being gates 13 and 14 below.

   Checking `--all-targets` for macOS found a defect the crate check had
   missed: `tests/stdin.rs` and `tests/repl.rs` pass `openpty` two null
   pointers that Apple's `libc` declares `*mut` and Linux's declares
   `*const`. `null_mut` satisfies both, since a `*mut` coerces where a
   `*const` is wanted. That is a fixture, not the seam, but the gates on the
   Mac run those fixtures.

   What that is worth is exactly what a type-check is worth: the eighteen
   errors are gone and there is a program to test. Every gate below is still
   unanswered.

   One fixture is `cfg(unix)`: `tests/stdin.rs` drives a pseudo-terminal,
   which is a Unix way to ask the question, and gate 8 is where Windows is
   asked it instead.

2. **The ladder, on the machine.** Run both default and `test-support`
   suites, with native fixtures. The measured Cargo launch path puts its
   test executables in a job that forbids Gate 10's outside breakaway
   control. On that machine, the acceptance commands are
   `./scripts/test-windows.ps1` and
   `./scripts/test-windows.ps1 -TestSupport`: build with `cargo test --locked
   --no-run --message-format=json`, then directly execute **every** reported
   test binary. No test is skipped, and any failing binary fails acceptance.
   A direct launch whose enclosing job also forbids the outside control
   still fails; an inner refusal alone never closes Gate 10.
3. **A deadline ends a child, and takes its descendants.** A child that
   outlives its deadline reports `timed_out`, and a descendant it spawned is
   gone afterwards — checked by asking Windows, not by inference.
4. **The job is assigned before the child can act.** A child whose very first
   action spawns a descendant still has that descendant inside the job.
   This is the race decision 2 exists to prevent, and it is what a
   spawn-then-assign implementation fails.
   `job_assignment_windows::assignment_precedes_the_first_descendant`
   observes the child before assignment, checks its threads' prior suspend
   counts, and verifies the immediate descendant's membership in the exact
   rnx job using a duplicated job handle. Removing `CREATE_SUSPENDED` fails
   the gate with zero prior suspend counts.
5. **Cancellation reaches a blocked write.** A child that never reads its
   standard input, fed more than a pipe holds, is cancelled: the call returns
   `cancelled`, the writer is collected, and no thread is left behind. A
   separate synchronous-pipe control also returns `ERROR_OPERATION_ABORTED`
   with its read end still held open. Decision 5's amendment separates the
   public cancellation result from proof that `CancelIoEx` reached the
   operation: job termination alone can otherwise unblock the write.
6. **Cancellation reaches the cleanup, not just the wait.** Record 0023's
   case: the child is already gone and an interrupt arrives while the readers
   are finishing. The call reports `cancelled`.
7. **The capture flags mean the same things.** `truncated` at the cap,
   `cut_short` when the call ends first, `unreadable` on a read failure,
   independently and in combination, with the decoding exemptions behaving as
   record 0023's unit gates require.
8. **`host::stdin` refuses a console and accepts a pipe.** Both, on Windows,
   with the same message.
   `tests/stdin.rs` runs the portable stream cases on Windows and uses a
   headless ConPTY console for the refusal, with a bounded wait.
9. **Nothing printed can move a cursor.** Record 0019's five surfaces, on a
   Windows console, which processes escape sequences when virtual terminal
   processing is enabled and must not be given any to process.
10. **The unreachable case is evidenced, not skipped.** A child that attempts
    `CREATE_BREAKAWAY_FROM_JOB` fails, and the ladder records that failure as
    the reason the escaped-descendant gates do not apply.
11. **Installation.** `cargo install --locked --path .` produces a working binary,
    which is record 0001's gate 6 for this third of it.
12. **Nothing regresses — Linux handoff at `96e3b88`.** Linux reports
    **238 tests and 243 with `test-support`**, zero failures, and clean
    formatting. Its Windows cross-check succeeds with one unused-import
    warning, and clippy adds a shared-fixture warning to the existing
    baseline. This closeout gates the Unix-only import and names the fixture
    type, removing those additions. Fresh native Linux execution belongs to
    the review of the amended tree; Windows cross-checking is not execution.

    Windows-only integration tests compile to zero tests on Linux; portable
    stdin and history tests retain their Unix coverage. The evidence's
    closeout section distinguishes fresh cross-target checks from the
    supplied native Linux run.
13. **The protocol's orderings — met now, on Linux.** Seven unit gates on
    decision 3's protocol: an operation is never declared after a stop; a
    stop reaches the operation a worker is inside; nothing is reachable once
    an operation is over; a cancellation that arrives before the submission
    is asked again; asking does not stop while the operation is still
    declared; a panic inside an operation withdraws the handle; and, across
    two hundred races between a worker declaring operations and a call
    stopping it, the worker never lets go of a stream anything can still
    reach.

    The fourth and fifth are the interleaving, not the declaration order.
    Both hold a worker between declaring and submitting — the gap in which an
    ask finds nothing outstanding and Windows answers `ERROR_NOT_FOUND` — and
    require an ask from **after** the submission, which is the only kind that
    ends a write blocked in the kernel. The fourth releases as soon as one ask
    has arrived, so it catches a `stop` that asks once. The fifth holds the
    gap open for 1,500 ms, past the cutoff the previous draft had, so it
    catches a `stop` that asks for a while and then gives up. Neither uses a
    sleep to order events: each side waits on the other's state, every wait is
    bounded, and a worker's teardown does not depend on the ask arriving — so
    a protocol that gives up fails these rather than hanging them.

    Controlled, each against the defect it names, and each control leaves the
    others passing:

    | Control | What fails |
    | --- | --- |
    | `begin` ignores the stop | the first, and the stress gate by its bound |
    | `stop` returns after one ask | the fourth **and** the fifth |
    | `stop` gives up after a second | the fifth alone |
    | the guard does not withdraw while panicking | the sixth |

    The third row is the one this record needed: with the cutoff restored,
    the gate that releases early still passes. That is how the defect
    survived a review — and why the fifth gate exists.

    The protocol is shared code, so these hold for both platforms. What they
    cannot do is prove that `CancelIoEx` ends a write blocked in the Windows
    kernel — they prove the protocol asks it at a moment when it can — and
    that is gate 5's job on the machine.
14. **Draining before reaching — met now, on Linux.** Decision 6's order,
    with the readers held back a known 300 ms by a `test-support` hook and an
    allowance of 3,000 ms: the six bytes the child left are all read,
    `cut_short` is false, and the call returns well inside the allowance.

    Controlled: with the readers reached as soon as the instant is published,
    the same gate reports `out=0 cut_short=true` — the whole of the output
    lost, which is the defect stated exactly. No child can arrange that delay
    itself, which is why every other gate here missed it.
15. **A failed job assignment leaves nothing behind.** With
    `RNX_TEST_JOB_ASSIGNMENT_FAILS` set, `host::process` fails and the
    suspended child is gone — asked of Windows, since a suspended process is
    invisible to anything that watches for output. The hook exists because
    nothing a script can do makes `AssignProcessToJobObject` fail.
    `job_assignment_windows::failed_assignment_collects_the_suspended_child`
    opens the live child before releasing the failure hook, requires the
    original error and failure exit, and observes termination through that
    retained process handle. Removing kill/reap fails with `WAIT_TIMEOUT`.
16. **Redirected empty input is read, not refused.** `rnx run` with its
    standard input redirected from `NUL` reads an empty input and reports it
    as such, where a console is still refused with record 0012's message.
    This is decision 4's correction, and a `GetFileType` implementation fails
    it.
    `stdin::nul_is_an_empty_stream_not_a_console` passes with empty output
    text and fails with the terminal-refusal error under a `GetFileType`
    character-device mutation.
17. **`rnx selfcheck` succeeds on Windows, asking three of its four probes
    here and the fourth elsewhere.** The interruption probe has a child send
    `SIGINT` to its parent, reaching that process alone. Its Windows
    counterpart, `GenerateConsoleCtrlEvent`, reaches every process sharing the
    console — for someone who has just typed `rnx selfcheck`, their own shell
    and whatever else runs in it. A self-check that interrupted the terminal
    it was invoked from would be a worse defect than any it could find, so the
    probe is replaced by a line saying where the question is asked instead.

    **Not decision 5's case.** An escaped descendant cannot be asked about on
    Windows at all. An interrupt can, and gates 19 and 6 do, in a console
    created for it. What changes is the venue, not the reachability, and this
    gate is what records that — three probes here, one moved, none dropped.
18. **A session's history survives a restart on Windows.** Decision 7:
    `%LOCALAPPDATA%\rnx\history` by default, `RNX_HISTORY` when set, and the
    Unix answers unchanged. Asked by running a session, ending it, and
    starting another — not by reading the path back, which is the check that
    would have passed while the defect was there.
    `history::a_second_windows_session_recalls_saved_history` creates two
    separate ConPTY sessions for each path setting. The second types only
    Up, Up, Enter, recalls past `:quit`, and evaluates the saved expression;
    its numeric result was never typed into that session. Removing history
    loading makes the second session fail its bounded result check.
19. **An interrupt reaches rnx even from a parent that suppresses it.**
    Decision 4's amendment. A call waiting on a live child reports
    `cancelled` and **not** `timed_out`, well inside its deadline, in a
    console of the child's own — asked twice: once inheriting whatever the
    runner had, and once with the process put into the "ignore Ctrl-C" state
    deliberately, because whether a runner suppresses Ctrl-C is not something
    a fixture chooses. The second case is the gate; the first would pass
    against the unamended code whenever the runner happened to permit
    delivery.

## What was written before the machine arrived

The mechanisms of decisions 2 to 4, behind a seam: `src/platform.rs` holds
what each platform does, and `host.rs` asks for the contract rather than the
call. Twelve Unix-specific places became seven questions — watch for an
interrupt, is standard input a console, spawn in a group, end the group,
prepare a stream, is it readable, and reach a worker inside an operation.

The last of those is `Pipe`, decision 3's protocol, and it is the part this
record got wrong four times before getting it right. Not a handle the parent
keeps and cancels while the worker looks unfinished — that is a
use-after-close waiting for a scheduler. Not shared ownership of the stream
either — that denies a child the end of its own input. Not a single ask,
which a worker can be a few instructions short of being able to receive. Not
asking on a timer either: giving up while the operation is still declared is
the same defect with a longer gap, and the join behind it waits for the very
event the asking would have caused. What stands: the worker keeps its stream
and publishes the handle for the length of one operation, withdrawn by a
guard so an unwind withdraws it too; the call keeps asking for as long as the
operation stands. On Unix `cancel` does nothing,
because a non-blocking descriptor is never inside an operation for longer
than it takes to return, and the loop ends on its second look; on Windows it
is `CancelIoEx`, repeated until the operation it is aimed at is over.

That protocol is shared code, so gate 13 tests it here, on Linux, including
the races. What cannot be tested here is what `CancelIoEx` does to a blocked
write, what a job object does to a descendant, and what `GetConsoleMode` says
about a redirected handle. Those are gates 3 to 11, 15 and 16, and they wait
for the machine.

Also written here, and gated here: decision 6's cleanup order, which is not
Windows-specific at all — the first draft of this seam broke a record 0023
promise on every platform, and gate 14 is what would have caught it.

## Guardrails and stop conditions

1. No behaviour is claimed for a platform it has not run on. A type-check is
   a type-check.
2. `cfg` splits the mechanism, never the contract. If a contract cannot be
   met on Windows, the record says which and why, and the gate becomes
   evidence of the difference rather than a pass.
3. The child is never run through a shell, on any platform. Record 0016's
   rule survives whatever machinery is needed to spawn it.
4. No `unsafe` beyond the Windows calls themselves, each with the invariant
   it relies on written next to it, as the Unix ones are.

## Risks

- **The toolhelp route is a guess until it runs.** Enumerating a
  just-created process's threads may race with its creation. Decision 2
  names the fallback and the record will say which was used, with the
  measurement that decided it.
- **`CancelIoEx` on a handle `std` owns.** The write handle belongs to a
  `ChildStdin`; cancelling I/O on it from another thread is supported, but
  the interaction with `std`'s own buffering is a question the gates answer
  rather than assume.
- **An operation that cannot be cancelled at all.** `stop` holds its
  responsibility until the operation withdraws, so if `CancelIoEx` does not
  end a write blocked on a full pipe, the call does not come back — the loop
  waits, and so would the join behind it. That is the honest failure mode: a
  hang on the Dell at gate 5, attributable to one mechanism, rather than a
  timeout that returns while the write stays blocked and reports nothing.
  Decision 3 says why no cutoff can tell that case from a worker that has not
  submitted yet.
- **A failed resumption is inspected, not gated.** Decision 2 now checks
  `ResumeThread`'s answer and abandons the child when it is a failure or
  leaves the thread suspended. Nothing here can provoke either: a
  just-created process has one thread with a suspend count of one. So that
  path rests on the documented meaning of the return value and on reading the
  code, which is weaker than the rest of this record and is said so here
  rather than implied by a gate that cannot fail.
- **A job object is not a process group.** Decision 5 argues the difference
  is a stronger guarantee, and gate 10 asks Windows to say so. If a
  descendant does escape — a service started on the child's behalf, say,
  which is not the child spawning it — then the guarantee is narrower than
  the argument, and the record is what changes, not the gate.
- **An interrupt on Windows is console-wide.** The Unix probes send
  `SIGINT` to a single process; `GenerateConsoleCtrlEvent(CTRL_C_EVENT, 0)`
  reaches **every process attached to the console**, which includes the shell
  that invoked the ladder. So an interruption probe is not a like-for-like
  port of `kill -INT $PPID`: it must be established in an isolated, bounded
  harness — its own console — before it goes anywhere near a shipped
  self-check, or a passing gate takes the operator's shell with it.

  Gate 6 is a further step again. It wants an interrupt **during cleanup**,
  with the child already gone and the readers finishing. A probe that
  establishes only that an interrupt arrives does not establish gate 6.

- **A console's escape handling differs.** Windows terminals process escape
  sequences when virtual terminal processing is on, and record 0019's
  contract is that rnx emits none — so the risk is the fixtures, which must
  assert on bytes rather than on what a console did with them.

## Forward

macOS, on the Mac Mini, as its own record. Release metadata, small and
independent. And record 0001's gate 5, which is provenance and not code.
