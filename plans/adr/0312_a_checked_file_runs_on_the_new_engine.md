# ADR 0312: A checked file runs on the new engine

Status: Proposed

Base: published 03505d1f97e200a6382e72f7f66ff94fdfb3575c (ADR 0311).

## Problem

An execution session can run `read_csv`, `to_dense`, `Tensor::from_dense`,
`rank` and `elem_count` on the new engine (0306 to 0311). The same
program written as a file does not. A synchronous `pub fn main(args)`
that loads a CSV named on its command line falls back to the old engine
at `read_csv`, with the refusal `OpaqueEntry`. The full `model.rn` and
`sales.rn` files fall back at the same call.

A file cannot reach the checked path because of three facts:
- The cold frontend admits checked native calls only under
  `opaque_session`.
- The synchronous file runner calls `prepared.invoke(budget)`, which has
  no cross-program state.
- The sliced shared-producer carrier (0310) explicitly requires that
  state.

Relaxing the `OpaqueEntry` refusal, or switching the file runner to plain
sliced invocation, therefore cannot run the chain.

The session's checked `read_csv` (0307) also does not fit files. It reads
a bounded snapshot, capped at `i32::MAX - 1` bytes because its scanner
assumes bounded splitter offsets and signed Arrow binview bounds. Today
the old engine reads larger inputs. Measured at 03505d1f, the published
CLI loaded a 2,147,484,175-byte CSV of 33,554,440 short rows on the old
route. Values were correct at four positions, one of them beyond 2 GiB,
in 6.3 s, with a 4.8 GB peak RSS. Automatically routing such a file to
the snapshot reader would turn a working script into an error on its
production-size input. The user ruled on 2026-11-19 that this must not
happen.

Two read-only audits at 03505d1f found the needed pieces already exist.
The first concerns invocation authority.
A one-member, non-session `ProgramDomain` enters `start_cross`, which
prepares once, verifies runtime bindings and epochs, and installs real
cross-program state. It supplies hard fuel, cancellation, Store credit
and source frames, with no edit to any interpreter loop. Admitted
probes showed:
- a located `StoreCapacity` with fuel remaining;
- deterministic fuel exhaustion and cancellation;
- one effect each;
- no live entries after close.

The second concerns the reader. A scratch provider ran the same large
file through the published path reader (`files::csv`), the checked
opaque `Result` bridge and the execution Store:
- One parse produced matching data.
- The frame's 2,046,820,888 descriptor bytes, plus 32 bytes of Store
  overhead, were charged once at adoption.
- An alias added no charge, and the last release restored the credit,
  leaving no live objects.
- At an injected 32 KiB cap, the read completed once, adoption returned
  `StoreCapacity`, and everything was released.

## Decision

1. A sealed checked-file entry runs a synchronous file's `pub fn main`
   inside a private one-member `ProgramDomain`, only when the file needs
   checked native calls and passes admission.
   - Every other file keeps its existing driver unchanged.
2. The entry accepts only the validated public `main` and the argv
   strings, which it builds into a vector in that domain's Store with
   error-safe cleanup.
   - It never takes caller-supplied native roots or alternate entry IDs,
     and never exposes a transferable checked `Program`, `ProgramKey` or
     `DomainRoot`.
   - `CrossFailure::frames` borrows the program, so failures are mapped
     inside the entry to owned diagnostic data, or a view of the source
     records.
   - The ordinary file result renderer is reused through a plain root
     view.
   - `Executor::prepare` and the cross check are not weakened, and no
     session authority is manufactured.
3. Admission runs the exact-local proof under a file policy. It checks
   explicit, tail, container and capture escapes, and implicit `?` error
   returns, so no native value leaves `main`.
   - The three witness producers have `String` errors.
   - The session-only privileges stay session-only: a typed native
     `Err` (0308) and async.
4. The decision to use the new engine is made before any effect. A typed
   admission refusal falls back to the old engine once, before anything
   runs.
   - An admitted file never restarts on the old engine: not on a runtime
     error, a Store cap, fuel exhaustion, a cleanup failure or an
     interrupt.
   - The caller's file work budget stays authoritative across slices and
     host calls.
   - File signal semantics and lifecycle ownership are preserved, and
     the session-only host interrupt marker is not enabled. Interrupts
     are checked at slice boundaries; a joined synchronous host call can
     delay observing one until it returns.
   - File, line and column are resolved while the owning member is
     alive.
   - On a terminal failure or SIGINT, argv and output roots are released,
     the lifecycle finishes once, and the domain closes.
5. Files read CSV with the published path reader, not the session
   snapshot reader.
   - The checked file `read_csv` is the published explicit-schema reader:
     one regular file, header names and order validated, the same handle
     rewound, the schema-bound body read, the frame validated. It runs
     on the joined worker, with the existing parser errors.
   - The owned frame returns through 0306's opaque result bridge.
     `StoreCapacity` stays the distinct execution error.
   - The charge is the logical retained frame: the declared size plus
     Store overhead, charged once at adoption and released at the last
     owner. Parser peak memory and worker buffers are not bounded on this
     path, exactly as on the old route.
   - The 0307 snapshot reader, its `i32::MAX - 1` cap and its checks
     stay unchanged on every path that uses them.
6. The file reader is selected only by trusted registration for a file
   invocation.
   - The CLI run branch registers extensions knowing the invocation's
     purpose. Only a private file context installs the checked file
     reader, beside the legacy handler, which stays available for a
     pre-effect fallback.
   - The checked file proof and invocation are bound to that context.
   - A large remaining allowance, or an environment setting, never
     confers file authority. Sessions and ordinary embedding contexts
     keep the session-safe registration.
7. The data allowance keeps the file default: no finite cap, as ordinary
   file runs have today (`isize::MAX`).
   - `RNX_SESSION_STORE_BYTES` stays session-only. Tests inject a small
     limit directly.
   - Polars diagnostic settings that the checked file path cannot honor
     (`POLARS_BACKTRACE_IN_ERR`, `POLARS_PANIC_ON_ERR`) are re-measured
     on the path reader. Any that still differ from the old route are
     declined before effects, so the file runs on the old engine. That
     policy is owned by the adapter.
8. Automatic file routing changes only after the real CLI, at the
   default file allowance, loads the large witness on the new engine:
   - with exactly one execution selection and no replay;
   - with correct data, on a fixture the file path can actually check.
     `interchange::Dense` holds at most 2^24 values, and `height()?`
     returns 0308's typed native error, which files do not admit.
     Neither rule is widened. The large witness is therefore a generated
     CSV over 2 GiB of 256-byte records, 8,388,610 rows, with an exactly
     representable numeric column. `width()` and the element count of a
     one-column `Dense`/`Tensor` prove the width and the row count. An
     independent reader checks both columns, including last-row spots,
     on that same input;
   - with a runtime path from argv;
   - with a parse failure reported;
   - with the session cap unchanged.
   If isolating the file-only binding needs a broader redesign, the new
   path ships as explicit opt-in, and automatic selection keeps today's
   behavior.
9. Scope: the checked synchronous file-main prefix. The witness is a file
   whose `main` runs `read_csv` (path from argv), `to_dense`,
   `Tensor::from_dense`, `rank` and `elem_count`, and prints scalars.
   - The full `model.rn` (`Mlp::load`, a two-native `forward`,
     `Tensor::to_dense`, `with_dense`, sort, column reads) and `sales.rn`
     (lazy queries) stay on the old route, and no claim is made for
     them.
   - No old-engine output is pinned.

## Constraints

- No edit to any interpreter loop and no protected item: all 40 items
  stay at their published source, and the published guard runs on all
  40 first.
- An early complete fixed-route screen runs on the entry before the
  witness is wired in. Stop on either band failure, and never widen a
  band.
- rnx root-tests run on the signed commit before the complete final.
- No new crate. Tabs.

## Controls

- The large witness: a committed generator writes the decision-8 CSV
  (over 2 GiB, 256-byte records, 8,388,610 rows) to an ignored build
  directory. The audit's 33,554,440-row read stays as evidence. A
  `read_csv` + `width()` CLI read can witness that exact shape if
  wanted. An explicit test,
  outside the default suites, runs it through the real CLI at the
  default file allowance, with route tracing, data checks and peak RSS
  recorded. The input is deleted afterwards. Its pass gates decision 8.
- Binding isolation: a session given a large allowance, and a plain
  embedding context, both still get the snapshot reader and its cap.
- The witness through the ordinary combined demo CLI, with a real CSV
  path in argv. Explicit route tracing shows the execution engine was
  selected and admitted, not a fallback. Output and exit match the
  session's `(2, 128)`.
- A visible sentinel `println!` followed by a later refused operation:
  one pre-effect fallback, and exactly one observed effect.
- Admitted failures each select execution once, show one effect, give a
  truthful file diagnostic and exit status, and release resources:
  - a runtime error;
  - a Store cap at an injected finite limit, one byte short;
  - work-budget exhaustion;
  - a SIGINT after the file prints its readiness line.
- Native escapes from `main` are refused at admission: tail return,
  explicit return, container, capture and implicit `?`.
- Ordinary files, with and without host values, keep their driver,
  result formatting and exit codes.
- The inventory admits exactly the new sites, each with a live witness.
- A mutation per rule fails its control.
- One complete final.
