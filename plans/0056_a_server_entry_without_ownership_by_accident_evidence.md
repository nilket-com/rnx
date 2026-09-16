# rnx 0056: gate 1, external assembly

Status: accepted and pushed at rnx `abc9bdd` and rnx-bench `99f5830`,
2026-09-16. Gate 2 follows in the host-evidence companion; gates 3–6 remain open. No
HTTP server, pool or adapter was extracted. No PostgreSQL service was used.

## Artifacts and reproduction

The optional root `server-runtime` feature exports `Program`, `Invocation` and
`Failure` under `rnx::server`. The external workspace is rnx-bench
`probes/server-entry/`; run `python3 probes/server-entry/run.py`. Its locked
build accesses only the public execution surface and Rune re-export. The two
captured runs and conditions are in `results/server-entry-0056/`. Conditions
record source, lock and executable SHA-256 hashes; initial measurements were
made on the implementation worktree, so its parent revision alone is not the
measured source identity. This is a debug-profile correctness gate, not timing
evidence. No startup or performance conclusion follows.

## What passes

Both runs produce eight PASS lines and empty stderr:

1. A real Rune handler is polled until its tracked native operation has started.
   Started=1 and dropped=0 precede dropping the run future. Dropped=1 follows
   synchronously, before any runtime turn; close returns category `cancelled`.
2. A destructor-panic variant reports `cleanup`, overriding cancellation.
3. Caller unwinding through a polled run preserves the caller's panic, disposes
   the tracked operation synchronously and leaves close reporting cancellation.
4. Two simultaneous invocations on one worker start separate operations. One
   fails; the survivor stays pending and undropped, then returns its original
   value through an owned response object. Its request is constructed publicly.
5. The same isolation holds on separate current-thread runtimes on two workers.
   Barriers establish that both operations started before the failure and keep
   the survivor from completing before the failing execution has settled.
6. Zero and the no-budget sentinel refuse before any extension builder runs.
   A CPU loop exhausts its whole budget; a second run refuses rather than
   resuming, and close remembers the original VM failure.
7. Runtime attribution names the imported file and correct source line, with
   the known-type missing-method recovery. Exit returns an ordinary Err value.
8. Partial construction failure releases the already installed extension's
   context capture; compilation failure returns an owned module diagnostic.

The fixture asserts one schema builder invocation and one captured context-owner
retirement, and exact builder/capture-retirement counts for both isolation axes,
cancellation and failed preparation. These counters are native registration
captures, not process allocation or runtime-task counts. Each root context is
constructed by the single `context` helper; each invocation has a separate
Lifecycle and HTTP State. The schema context is retired after compilation, while
the retained Program holds only the unit, sources and bounded-loader snapshots.
Program's Send+Sync property is a compile-time assertion in the external crate;
Invocation's non-Send property has a compile-fail doc test.

Request construction uses Rune's Object and Value APIs. Response inspection
uses public `borrow_ref::<Object>()` and `as_integer`. No rnx JSON writer, test
module injection or private import participates. Context creation and close run
inside an already-entered runtime, so a nested runtime would fail this fixture.

## Implementation details that matter

A run guard is established before VM construction. On abandonment the VM-held
values are dropped and lifecycle finish runs as failed. Cancellation is stored
on the invocation; close cannot turn it into success. Normal VM failure is
similarly remembered. Cleanup failure takes precedence. An unpolled async run
is inert; that is not the cancellation proof. Owner drop retires as a fallback,
but callers must use explicit close to observe the result.

The new context installer omits CLI interrupt setup and registers its own exit
refusal through a crate-private process-install helper. Existing CLI installation
still arms interrupts first and uses the original exit function. Stronger host
policy tests, including the global script flag and a preinstalled signal handler,
belong to gate 2 and are not claimed passed by the catchable-exit check above.

## Finding during fixture development

The first isolation harness changed its trigger flag without waking the future
inside Rune select. Its premature readiness assertion was a fixture error.
Unwinding that assertion exposed a real existing bug: `build_catching` attempted
to replace the panic hook while the caller was already unwinding. Rust forbids
this and aborted through a second panic during destruction.

The catcher now uses catch_unwind without changing hooks when the thread is
already panicking. A focused root regression preserves the original caller
panic while handling both successful cleanup and a second cleanup panic. The
external fixture additionally unwinds with a genuinely started tracked operation.
On this exceptional path the caller's hook owns any nested panic diagnostics;
the usual temporary silent hook is not installed. Normal, nested and overlapping
catch behavior retains its existing tests.

## Validation and limits

- Default root suite: 375 passed, zero failures.
- Root suite with `server-runtime,test-support`: 419 passed, zero failures,
  including the feature's compile-fail documentation test.
- External locked fixture: two complete eight-group runs, empty stderr.
- Formatting and notices checks pass; root lockfile/notices are unchanged.
- External fixture clippy passes with warnings denied. Root strict clippy reports
  13 findings in existing files and none in the new execution module; no clean
  root-clippy claim is made. Toolchain is recorded in bench conditions.

An initial concurrent default/support test run reused the same binary path and
made the default worker gate see test-support's read-counter file. The default
suite was rerun serially and passed. This was harness interference, not a product
fix. Do not run these two configurations concurrently in the same target dir.

Only Linux execution was measured. API diagnostics currently return the first
fatal compile diagnostic. Trusted adapter schema consistency is a contract, not
an exhaustive runtime check. Native blocking, global process facilities and
whole-runtime cleanup remain as stated in the plan. No HTTP extraction starts
until this assembly result has been reviewed.
