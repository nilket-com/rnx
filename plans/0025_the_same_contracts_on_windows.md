# rnx 0025: the same contracts on Windows

Status: proposed 2026-09-09. The twenty-fifth record of rnx. Record 0001's
release gate wants a child cancelled, deadlined and cleaned up on Linux,
macOS and Windows. On Windows the code does not compile, let alone run. This
record decides how each contract records 0020 through 0023 established is met
there, and says which of them Windows cannot be asked the same question.

The seam is written and compiles; three rounds of review of it are folded
into decisions 2, 3, 4 and 6 below, with what each defect would have cost.
Four of the eight findings were on one mechanism — how a call reaches a
worker inside an operation — and decision 3 is written as that sequence,
because the wrong answers are the useful part. No Windows behaviour is
claimed.

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

### 7. What this record does not decide

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

2. **The ladder, on the machine.** `cargo test --locked` and
   `cargo test --locked --features test-support` pass, with the fixtures
   adapted: a shell that exists there, a source of thread counts that is not
   `/proc`, and a way to make a child hold a pipe.
3. **A deadline ends a child, and takes its descendants.** A child that
   outlives its deadline reports `timed_out`, and a descendant it spawned is
   gone afterwards — checked by asking Windows, not by inference.
4. **The job is assigned before the child can act.** A child whose very first
   action spawns a descendant still has that descendant inside the job.
   This is the race decision 2 exists to prevent, and it is what a
   spawn-then-assign implementation fails.
5. **Cancellation reaches a blocked write.** A child that never reads its
   standard input, fed more than a pipe holds, is cancelled: the call returns
   `cancelled`, the writer is collected, and no thread is left behind. A
   `PeekNamedPipe`-only implementation cannot pass this, which is what makes
   it the gate for decision 3.
6. **Cancellation reaches the cleanup, not just the wait.** Record 0023's
   case: the child is already gone and an interrupt arrives while the readers
   are finishing. The call reports `cancelled`.
7. **The capture flags mean the same things.** `truncated` at the cap,
   `cut_short` when the call ends first, `unreadable` on a read failure,
   independently and in combination, with the decoding exemptions behaving as
   record 0023's unit gates require.
8. **`host::stdin` refuses a console and accepts a pipe.** Both, on Windows,
   with the same message.
9. **Nothing printed can move a cursor.** Record 0019's five surfaces, on a
   Windows console, which processes escape sequences when virtual terminal
   processing is enabled and must not be given any to process.
10. **The unreachable case is evidenced, not skipped.** A child that attempts
    `CREATE_BREAKAWAY_FROM_JOB` fails, and the ladder records that failure as
    the reason the escaped-descendant gates do not apply.
11. **Installation.** `cargo install --locked` produces a working binary,
    which is record 0001's gate 6 for this third of it.
12. **Nothing regresses.** Linux and both macOS targets still type-check, and
    the Linux ladder still passes — 226 tests, 231 with `test-support`.
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
16. **Redirected empty input is read, not refused.** `rnx run` with its
    standard input redirected from `NUL` reads an empty input and reports it
    as such, where a console is still refused with record 0012's message.
    This is decision 4's correction, and a `GetFileType` implementation fails
    it.

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
- **A console's escape handling differs.** Windows terminals process escape
  sequences when virtual terminal processing is on, and record 0019's
  contract is that rnx emits none — so the risk is the fixtures, which must
  assert on bytes rather than on what a console did with them.

## Forward

macOS, on the Mac Mini, as its own record. Release metadata, small and
independent. And record 0001's gate 5, which is provenance and not code.
