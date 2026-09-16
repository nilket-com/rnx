# 0053 evidence: revoking an operation held by an older binding

Measured by Codex on nano/Linux, 2026-09-16. Plan `bc471c7`.
Implementation complete, review pending. The PostgreSQL adapter remains stopped
under 0052 until this lifecycle implementation and rerun are accepted.

Raw evidence and executable fixtures are in rnx-bench commit `51766e4`,
`results/lifecycle-0053/`,
`probes/extensions/`, and `probes/postgres/ownership/`. Conditions record toolchain,
binary hashes, commands, sources, core pinning and complete comparison outputs.
The separate kernel package, root dependencies, lockfile and notices are unchanged.

## Ownership and API

`Extensions::with_lifecycle` lends the existing module plus a clonable `Scope`.
`Scope::track` returns an opaque future with the inner `Result<T, String>` output.
Both the future and T can be non-Send/non-Sync. Scope contains only a process-wide
atomic context identity, its thread identity, and the extension's static name.
The name attributes destructor failures; it does not authorize another context.

The exact live context is resolved through a thread-local weak registry. No
ambient-context fallback exists. Identities and execution generations refuse
exhaustion instead of wrapping. A tracked owner holds a pinned boxed future;
its wrapper is strong and the registry weak. Polling takes the pinned box out
of the cell, stamps the execution generation and polls without borrowing the
registry or owner. Cancellation takes and drops it between polls. Completion
and final wrapper drop remove the weak entry. There is no unsafe code, detached
task, extra runtime entry, or notification that depends on a subsequent poll.

Wrong-thread calls return `operation scope used on the wrong thread`. Retired
context calls return `operation scope belongs to a retired context`. They do
not poll the supplied future. Cancellation of an already tracked operation
returns `operation cancelled`. Reset clears operations but preserves the serving
context and its registration scopes; retirement invalidates those scopes.

The panic catcher now shares one dispatcher across overlapping scoped catches,
with a per-thread nesting count. Its mutex protects hook installation/restoration,
not adapter code. A builder can join a catching child without deadlocking. Only
threads inside a catch suppress their panic output; other threads reach the
previous hook. This matters for wrong-thread track refusals whose supplied
future may itself panic on drop. The previous hook is restored after the last
active catch. Existing builder refusal and other-thread panic gates still pass.

## Execution boundaries and failure

- Session/eval/worker advance the internal generation after compilation, just
  before VM execution. Finish runs after conversion/publication or failure and
  before prompt/protocol settlement. Compilation/admission failures have no active
  generation to cancel. Renumber changes no lifecycle state.
- Run advances before driving its VM and finishes after the VM drops, before
  reporting the outcome. A caught Result error remains ordinary control flow.
- Reset revokes all tracked operations before clearing bindings. Normal REPL
  quit/EOF, main returns, and terminal exits close the context. Explicit
  `process::exit` reaches terminal cleanup; worker transport failures also pass
  through it. Drop provides a fallback for normally unwound context ownership.
- A destructor panic records a named lifecycle failure, attempts other independent
  revocations and retires the context. A session does not return to its prompt.
  The worker settles with category `runtime` and `state_lost: true`, observes its
  normal acknowledgement rule, then exits unsuccessfully. Jupyter fails an
  admitted queued cell with `WorkerDied`.

HTTP is unchanged: interrupt/reset clear all its requests, runtime/budget failures
ordinarily do not. Migration to execution-scoped cleanup remains a later decision.
The PostgreSQL fixture has zero spawned tasks; the existing HTTP check therefore
returns without entering the runtime. Its cleanup measurements are not a drain.

Native code's aborts, explicit exits and blocking destructors remain outside panic
and time guarantees. A native future that explicitly exits while on its own poll
stack does not return for Rust destruction; the OS ends that process. Server-side
transaction outcomes are not decided by client resource revocation.

## Gates and observations

1. Eight lifecycle unit tests exercise lazy construction, final drop, successful
   and Err completion, exact context/thread lookup, stale handles, non-Send Rune
   registration, runtime failure, budget exhaustion, unrelated pending work,
   compile refusal, successful select, returned Err, renumber, reset, teardown
   and registry reclamation. The repeated-operation test completes 5,000 futures
   with no weak entries left. An external one-input loop completes 1,000 calls.
   A ninth new test exercises nested and overlapping panic catches.
2. All seven original 0052 ownership cases pass again through Rune: success,
   caught deadline, conversion refusal, direct interrupt, unpolled-after-select
   failure, the real worker budget, and laziness. The source, open socket inode,
   activity observation, settlement, task count and drop trace are retained.
3. The retained-binding counterexample now closes: its socket count is **0 → 0**
   at settlement, before ack or another input. It was observed open and active
   first. Repoll returns `Err("operation cancelled")`; no replacement socket opens.
   The private cluster is stopped and removed. The only driver-code change is
   registration through with_lifecycle and `scope.track(query(...))`.
4. Rune unit gates retain an older future, poll it in a later input and end by
   panic or budget halt. The resource is gone before eval returns. Unrelated
   failed/invalid inputs preserve an older pending operation; successful select
   and returned Err preserve it too. Reset closes it. Published unrelated bindings
   survive ordinary cancellation.
5. The external fixture exercises run, eval, session, parked worker, interrupt,
   reset and shutdown. Its destructor-failure case revokes both the failing owner
   and an unrelated owner, reports state loss and exits 1. Quit and EOF destructor
   failures each emit one lifecycle diagnostic and exit 1. All observations of
   worker Drop output precede acknowledgement.
6. The unchanged installed Jupyter kernel runs the lifecycle app. It observes the
   native operation start before interruption, receives the cancellation value,
   preserves `kept = 42`, and reports WorkerDied for an admitted queued request
   after a destructor failure. Temporary kernelspec/config/runtime directories
   are removed; the user's Jupyter directories are untouched.
7. The public inventory is exactly main_with, Extensions, Scope, and rune.
   Generated docs and external compile failures verify private modules/fields.
   The existing 0051 assembly, help/reset, settings ordering, no-hook fast paths,
   installation refusals and panic-hook behavior still pass. Stock before/after
   and the lifecycle app produce identical bytes/statuses for the 22 comparison
   cases, including CLI refusals, single-file faults and returned values.
8. Both full root suites pass under `TERM=xterm`: **369 default, 410 test-support**.
   Kernel default and transport-probe configurations each pass **23** tests.
   Formatting, release selfcheck and notices check pass. Clippy has the same **35**
   inherited diagnostic occurrences, including its two existing denied Unix-mode
   literals; no new diagnostics. Actual extension/lifecycle sources type-check in
   the isolated Windows fixture. This is not a full Windows build or execution.

Two fixture corrections are recorded rather than hidden. Direct `q.await` consumes
Rune's value, so a repoll gate must retain it through select, as the accepted
ownership fixture does; restoring a consumed Rune value is not this interface's
contract. The notebook driver initially discarded replies for other requests while
waiting for one id. It now retains them by id, so out-of-order terminal replies do
not become a false timeout. Its queued failure assertion must actually receive
WorkerDied; it does not accept a disconnected socket as success.

## Cost

Core 4, 10 warmups, 100 processes per row; default release binaries. No speedup
is claimed. Startup deltas are within the run-to-run drift observed on this
machine. A separate JSON workload shows a small regression in this build.

| case | stock before | stock after | lifecycle app |
| --- | ---: | ---: | ---: |
| version | 0.552 ms | 0.538 ms | 0.551 ms |
| eval 42 | 4.079 ms | 4.014 ms | 4.014 ms |
| bare run | 3.731 ms | 3.673 ms | 3.675 ms |
| 10k JSON | 11.597 ms | 11.839 ms | 11.454 ms |

The JSON difference was repeated in before/after/after/before order: 11.611,
11.814, 11.798, 11.589 ms (standard deviations 0.110, 0.133, 0.149, 0.096 ms).
Thus stock after is about **0.2 ms / 2% slower on this workload**; its cause has
not been isolated. It is not described as zero cost or explained away as drift.
No tracked operation is used by that script. The startup gates still hold.

Stock size grows from 15,134,384 to 15,187,704 bytes, **53,320 bytes**. A disabled
lifecycle allocates no registry; serving apps opt in through with_lifecycle.
The fixture app is 15,285,360 bytes. Hashes for stock, app, PostgreSQL probe and
unchanged kernel accompany the raw evidence. This measures lifecycle assembly,
not the unfinished adapter's full query contract or connection cost.
