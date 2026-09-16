# 0056 gate 4: extracted server, public execution boundary

Status: accepted and pushed 2026-09-16 at rnx `3f8f6af` and rnx-bench
`dede5d3`. Gates 5 and 6 are recorded in the separate acceptance evidence. Gate 3's accepted commits `1aa600e` / `181a444` were pushed before
this work. Root source, root manifest/lock/notices, kernel and PostgreSQL adapter
are unchanged from that base. New runtime code is confined to the independent
`servers/http-postgres/` workspace.

## What moved

The coordinator, workers, wire validation and pool came from rnx-bench's
accepted `server-shutdown` prototype. No archived private-root module is needed.
One public `Program::compile` builds the immutable program; every dispatched
handler uses `prepare`, `run`, bounded host response conversion, then explicit
`close`. Only afterward does its lease COMMIT or ROLLBACK. A repeated ordinary
VM/cancellation error from close stays a request failure; category `cleanup`
forbids COMMIT, finishes the lease unsuccessfully and fails the worker/server.

The factory builds an independent `app` extension for the schema and each
handler. The schema owns no client. The handler's `app::record` operation is
tracked through its context's public Scope. Request construction and response
field access use only the Rune re-export. No private renderer, context builder,
lifecycle or HTTP cleanup entry is imported. The final whole-runtime drain
occurs after handler and pool owners end, outside the long-lived block_on.

The standalone binary owns SIGINT/SIGTERM and hard exit. Driver-retirement
failure reports worker failure; a blocked native poll reports the remaining
credits, requests, workers and sockets at the overall deadline. It makes a
bounded nonblocking write to stderr, then `_exit(1)` without destructors or
stdio flushing. No clean-close event precedes failure. Optional regular-file
logging remains host I/O, not a wall-clock guarantee against an OS stall.

The numeric limits and clocks are unchanged. Two small startup/cleanup details
are explicit: dropping the parent readiness sender and timing its receive makes
a failed worker startup observable; final ownership assertions now precede the
clean-close event. Response validation, routing and parser controls retain the
accepted behavior, including bare-LF tolerance.

The package disables rnx's optional counting allocator and declares its own
private allocator, copied from the same counter implementation. This preserves
live allocation request bytes and peak measurements without a new root export.
It is process-wide accounting, not RSS or a ceiling. It obeys 0051's requirement
not to declare a second allocator while rnx's counting feature is enabled.

## Binaries, graph and reruns

All commands are reproducible in rnx-bench `probes/server-extraction/README.md`.
`build.py` builds both locked/offline release variants and records their exact
source, program, lock and graph hashes in `build.json`. The tested binaries are:

| Variant | SHA-256 | Bytes |
| --- | --- | ---: |
| test-support | `3bddd776c7508ce3d67fe0128e72312eb79a5ac3202b0d98140ab51c09aecaf2` | 16,294,376 |
| ordinary | `259e8b0b0f4de7a5570bb5d467c8c22875f5d5d44865b1d3c896e0beb4874076` | 16,277,720 |

The package pins Hyper 1.11.1, hyper-util 0.1.20, http-body-util 0.1.5 and
tokio-postgres 0.7.18, with its own lockfile and resolved graph. The deterministic
Linux normal/build/procedural-macro notices cover 178 packages and 129 distinct
texts. The inherited `syntree 0.18.0` missing text remains named, not fabricated.
Package formatting, clippy with warnings denied in both feature configurations,
and its notices check pass. Root suites and matched stock measurements are gate
6's work; they are not claimed as newly rerun here.

Raw events, independent PostgreSQL observations, socket/owner counts and
conditions are in rnx-bench `results/server-extraction-0056/`. The only changes
to existing regression harnesses are an executable/program override and an
optional stderr descriptor. Original assertions and limits are unchanged; the
archived invocation remains the default. Every case records the extracted
binary's hash, distinguishing the old build as reference metadata.

| Rerun | Result |
| --- | --- |
| Raw HTTP wire corpus, pool alive | 57/57 |
| Transaction/lease regression | 8/8 |
| Shutdown matrix, SIGINT | 13/13 |
| Shutdown matrix, SIGTERM | 13/13 |
| HTTP scheduling | six rows, three samples each |
| COMMIT classification through extracted owner | four SQLSTATE cases |
| Full, unread stderr at native-poll shutdown | exit 1 at 5,027 ms |
| Ordinary build | echo, commit, rollback, inherited stdin socket, test-hook refusal |

All cluster fixtures create private Unix-socket clusters and reap their
postmasters. Successful shutdowns have zero owned sockets, active credits and
connection permits, builds equal retirements, both pools empty and drivers
joined. The shutdown observer checks backend disappearance separately. The
normal build additionally launches with a Unix socket on fd 0: the original
socket identity remains in the baseline and zero *owned* sockets are reported.

## COMMIT outcomes in the extracted owner

The accepted gate 3 classifier is integrated unchanged: an actual driver
DbError from COMMIT, parsed severity ERROR, class 23 or exactly `40001`/`40P01`.
Everything else is ambiguous. Both classes of failure retire and replenish;
neither retries. `tx_finish_error` reports classification and optional SQLSTATE.

The integration fixture raises `23503`, `40001`, `40P01` and control `40003`
from a deferred trigger. It proves the HTTP/owner path chooses the first three
as rejected and the last ambiguous, sends exactly one COMMIT through the proxy,
and serves the next same-worker borrower from a different backend. A second
connection observes zero victim rows in all four cases. These are deliberate
SQLSTATE injections, not new serialization/deadlock mechanism evidence; the
real mechanisms are gate 3's accepted private-cluster probe. The eight-case
unchanged regression separately reproduces lost-COMMIT-reply ambiguity with a
committed row and failed-rollback retirement.

## Scheduling observations

The final coherent three-sample run used four pinned CPUs with no other
probe/build running. All
latencies are measured through real HTTP. No matched old binary was timed in
this gate, so the differences from older prototype tables are not a speedup or
regression attribution.

| Scenario | Healthy median ms |
| --- | ---: |
| Alone | 4.33 |
| Awaiting on spare worker | 5.75 |
| CPU on spare worker | 8.73 |
| Both workers CPU-bound | 68.51 |
| Mixed await and CPU | 4.96 |
| Behind awaiting batch | 104.24 |

Healthy context preparation medians span 3.08–7.86 ms. The awaiting batch
contributes about 95.99 ms of central queue time. CPU saturation still delays
work until the budget fault; a sleeping handler's timer cannot progress while
its worker is in a CPU poll. The coordinator clocks remain independent. These
are the accepted scheduling properties, not a new fairness promise.

A preceding repeat failed the saturated-row overlap assertion. The first CPU
handler faulted at 84.283 ms, the second started at 86.347 ms after a 68.77 ms
prepare, and healthy arrived at 89.126 ms. That was not a saturated arrival;
the unchanged fixture correctly refused to count it. Its raw trace and failure
are retained in `scheduling-overlap-miss/`; the cause of the preparation delay
is not isolated. The final fresh-directory run passes every original assertion.
An earlier complete summary is retained separately and is not used for this
table because the failed repeat had overwritten some of its per-case files.
No sample or assertion was silently omitted or relaxed.

## Shutdown and normal-build separation

Each signal's thirteen rows includes idle, partial reads, awaiting, CPU, active
SQL, queued awaits/SQL, pending rollback/commit, acknowledgement deadlines,
driver-retirement failure and a blocking native poll. Active SQL rolls back at
the server statement timeout. Pending COMMIT keeps its committed outcome.
Acknowledgement deadlines retire the lease but can still end resource-clean.
The driver and overall failure rows exit 1 and emit no clean-close event.
Overall-deadline exit was 5,034 ms for SIGINT and 5,031 ms for SIGTERM. Neither
reports the still-blocked worker as joined.

An additional fixture fills a 65,536-byte stderr pipe before launch and does
not read it until after exit. The native-poll case exits 1 at 5,027 ms, leaves
those original bytes intact, and records one remaining request/worker in the
separate event log. The backend observer subsequently sees zero connections.
This proves a blocked stderr pipe does not postpone containment; it does not
claim a graceful end for the native poll or synchronous PostgreSQL notice.

Both failure injections are compiled only with the package's `test-support`.
The ordinary executable succeeds at echo, commit and rollback with the driver
stall variable set, then closes cleanly. Compiling `examples/fixture.rn` fails
on its missing test-only function. The supplied ordinary `examples/app.rn`
needs no test hooks. An initial smoke-fixture assertion expected lowercase
`missing item`; Rune's diagnostic uses `Missing item`. Correcting that fixture
expectation, without changing the executable, made the check pass.

Gate 4 is ready for review. This evidence does not close gate 5's example
journey or gate 6's stock behavior/performance comparison. Only Linux execution
is claimed. No context cache, pool API or additional root public surface was
introduced during extraction.
