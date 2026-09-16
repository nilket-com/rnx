# rnx 0054: a server owns more than a handler

Status: proposed 2026-09-16; revised after the first review. The boundary
probes are accepted and pushed in rnx-bench at `8b97577`. This is the step-four design draft, not an implemented server or a
claim that the existing extension interface can already assemble one. The
assembly gate below must settle the interface before implementation is ready.

Gate 2 was measured on 2026-09-16 against `c28cf22`; see the companion
`0054_a_server_owns_more_than_a_handler_assembly_evidence.md`. Both shapes
assemble, lifecycle isolation passes and request ownership is separate, but
HTTP cleanup reproduces a stop: nested block_on inside a running server, and
a whole-runtime drain that cannot finish while a healthy peer request remains.
Gate 2 stays open. No admission shape or HTTP library has been selected, and
gate 3 waits for the cleanup boundary to be settled.

## Context

Records 0050 through 0053 proved multi-file programs, executable assembly,
one typed PostgreSQL query and revocable operation futures. They did not
prove a server. Today's serving contexts execute one input at a time on a
current-thread runtime, with lifecycle begin/finish around that execution.
A web application needs a long-lived owner, concurrent requests, bounded
admission, transaction ownership and shutdown that waits for background work.

The proving application remains deliberately small: a healthy request, an
awaiting slow request, a CPU-bound request, and a request that writes inside
a transaction and then fails. It must show healthy progress with spare
capacity, rollback before reuse, and no owned connections or tasks after
shutdown. HTTP routing ergonomics and package management follow these facts.

The nested-async budget defect in pinned Rune 0.14.2 remains an exclusion.
A halted nested async execution cannot be treated as a resumable scheduling
slice. Nothing in these probes resumes after exhaustion. No argument about
Tokio fairness overturns that defect.

## Probe evidence

Source and reproduction instructions are in rnx-bench
`probes/server-boundary/`. Raw results are in
`results/server-boundary-step-four/`: two complete repeats, each with seven
samples of five scheduling cases and four transaction cases. The conditions
file records source and binary hashes, the lockfile, toolchain, machine and
CPU affinity. Rune is pinned to 0.14.2; tokio-postgres to 0.7.18. The harness
starts its own PostgreSQL 18.6 cluster over a private Unix socket and removes
it afterward. It never uses the system cluster.

These are **internal dispatch measurements, not HTTP latency**. An independent
coordinator waits for a native callback from the slow Rune handler before
submitting healthy work. Compilation and thread startup are outside the
measurement. Both worker models run on the same two physical cores (CPUs 2
and 4 here). Every handler receives its own whole 10,000,000-instruction
budget; the awaiting handler sleeps for 250 ms. The accepted probe compiles
once inside each worker, then shares its Unit and RuntimeContext between that
worker's VMs. It does not measure building one context for all workers.

| Model and slow work | Healthy median, repeat 1 | Repeat 2 |
| --- | ---: | ---: |
| One shared thread, awaiting | 0.0055 ms | 0.0061 ms |
| One shared thread, CPU loop | 59.2120 ms | 59.2614 ms |
| Two workers, awaiting, one spare | 0.0137 ms | 0.0352 ms |
| Two workers, CPU loop, one spare | 0.0107 ms | 0.0128 ms |
| Two workers, both CPU-bound | 56.9576 ms | 59.1631 ms |

All awaiting/spare-worker cases require the healthy handler to finish first.
The saturated maximum is 72.4 ms across these repeats; the median is not
a deadline. Routing to the spare worker is explicit in the fixture. A real
dispatcher must implement admission rather than assume that a spare worker
will receive the request. The fixture has at most three jobs; it is not a
production scheduler load test.

The ownership fixture is a minimal single-slot pool actor, not a pool crate.
It owns the client and connection-driver task. A Rune handler writes inside
BEGIN, then panics, exhausts its budget, or is dropped at an awaiting deadline.
A second borrower is already queued. Before rollback, an independent
connection gets SQLSTATE 55P03 when attempting NOWAIT row locking. After the
owner awaits ROLLBACK, that observer can lock the row and sees the original
value and no inserted audit row. Only then does the actor admit the queued
borrower, which uses the same backend PID and also sees no audit row.

An additional case shuts down while the backend is actively executing
`pg_sleep(120)`. Dropping the Rune future leaves the command running. With an
800 ms server statement timeout, acknowledged rollback takes 796.3–798.1 ms;
cleanup plus observation takes about 800 ms. No cancel request is sent. The
short rollback cases take 0.03–0.21 ms. These are observations, not general
latency bounds.

In all eight cases, before leaving block_on, the driver tasks have been
joined, socket descriptors have returned from two to the baseline of zero,
and runtime alive tasks have fallen from two to zero. The LocalSet owner
is joined separately; it is not counted by that runtime metric. Backend disappearance is polled separately.
The Python parent independently sees zero tagged backends and verifies that
the fixture postmaster and its temporary directory are gone. There is no
separate pool-maintenance task in this prototype; the connection driver is
the background task actually tested.

## Proposed decisions

### 1. Bounded workers, with a whole budget per handler

Use a fixed number of executor workers and a bounded admission queue, not a
thread created per request. Each worker owns its current-thread runtime and
its Rune execution state. Request and response data cross the boundary as
owned host data; Rune values do not cross threads.

Do not yet choose one handler per worker as the server contract. Compare
both shapes in the assembly gate:

- Serial admission: one active handler VM per worker, with lifecycle state
  owned by that worker. N workers admit at most N active handlers, including
  handlers awaiting I/O. This avoids overlapping generations but pays for that
  convenience with limited I/O concurrency.
- Multiplexed admission: at most M active handler VMs on each of N workers,
  each handler execution owning a distinct lifecycle context and scope. Awaiting
  yields to another handler on the same worker. N times M is an explicit active
  request limit, separate from queued requests. A CPU-bound handler still blocks
  every other handler on that worker until its whole budget finishes.

The shared-thread awaiting result makes the second shape worth measuring; it
is not proof of zero overhead with real rnx state or arbitrary concurrency.
Neither shape may overlap executions in one 0053 lifecycle state. Select the
shape and M from the assembly measurements, including latency, throughput,
context construction cost, retained memory and failure isolation, rather than
from the convenience of the present lifecycle implementation.

Spare execution capacity, not an unconditional latency guarantee, is the
promise. Pending I/O and a worker actively running a CPU loop are different
occupancy states. When active or queue limits are reached, requests wait only
within the stated bounded queue or are refused. The initial experiment uses
two workers; worker counts, active limits, queue capacities and refusal responses
must be fixed in the HTTP contract before implementation. No automatic retry
or replay of a failed request.

A handler receives one whole VM budget. Exhaustion fails the handler and
does not resume it. An asynchronous wait can be cancelled; a running CPU loop
is stopped by its budget. A blocking native call is not bounded by that
budget or made interruptible by assigning it a thread. This model is not
isolation from native crashes, memory exhaustion or unbounded native calls.

### 2. Transactions have an owner outside the handler VM

An exclusive connection lease remains owned by the server-side transaction
manager until completion is established. Handler failure, budget halt or
cancellation begins rollback; it does not return the connection to the idle
pool. Reuse follows acknowledged rollback and protocol synchronization.
A failed rollback retires the connection instead of admitting another borrower.
The latter is a required integration gate, not a measured claim from this probe.

The first application owns its transaction boundary. Arbitrary script-issued
BEGIN/COMMIT, nested transactions, commit-on-disconnect assumptions and retry
policies are not inferred from the single-slot example. Commit ambiguity and
cancellation during COMMIT require an explicit contract before exposing
transactions to general scripts. The accepted per-call `postgres::query`
contract remains unchanged.

### 3. Shutdown belongs to the server owner

Stop admission, settle or cancel admitted work, complete or retire transaction
leases, close idle connections, and await owned connection/background tasks.
Observe client descriptor closure and backend disappearance separately.
The normal path must not report clean shutdown merely because a runtime was
dropped. A deadline/failure path must say what cleanup did not complete.

The probe proves this order with one connection and an 800 ms server timeout.
It does not set production timeouts or claim that dropping a query cancels its
server command. Pool size, maintenance policy, cancellation protocol and
shutdown deadline remain integration decisions measured against the same gates.

Do not turn Scope::track into an asynchronous drain hook. It revokes operation
futures, not idle pooled connections or server tasks. Keep the prototype owner
private until assembly proves whether any additional public interface is needed.

### 4. Preserve the existing assembly contract

Source review adds a concrete constraint. `Extensions` holds FnOnce builders;
0051 promises one invocation per process for its current entry points. `Scope`
records its creating thread and refuses use elsewhere. Its context has one
active flag and execution-generation counter. Sharing a compiled context
containing that scope across workers is not a supported shortcut.

Before fixing public API names, build an assembly prototype with the actual
rnx batteries and a lifecycle-aware extension. Separate the ownership layers:

| Layer | Ownership to establish in the assembly gate |
| --- | --- |
| Compilation context and immutable Unit | Build the registration schema and compile once where possible; share the Unit only across compatible native registrations. Count builds and verify compatibility. |
| RuntimeContext and registered native closures | Rune permits sharing, but closures capture state. Prove a single shared runtime context resolves the correct owner, or record separately constructed runtime contexts and their cost. Arc cloning does not rebind a captured scope or HTTP state. |
| Tokio runtime and local task set | One per worker thread, with explicit active and queued limits. |
| VM, Rune values and execution budget | One per handler; never transfer these values between threads. |
| Lifecycle state and Scope thread binding | Per worker for serial admission; per handler execution for multiplexing, created on the owning worker. No shared active flag or generation counter between concurrent handlers. |
| HTTP client, active-request state and cleanup owner | Give each worker/handler an explicit owner; prove that failure cleanup cannot cancel another handler's request, including on the same worker. Sharing an idle pool needs separate request cancellation ownership. |
| Process inputs | env::args captures immutable host strings, which may be shared with fresh Rune values per call. env::var and env::vars read the process environment today, not a context snapshot. Any new server snapshot policy must be stated and gated, not inferred. |

`RuntimeContext: Send + Sync` permits moving registered closures, not moving
the ownership represented by their captures. Today's HTTP registration captures
State and today's lifecycle-aware adapter captures a concrete Scope. Building
one runtime context for all workers/handlers therefore needs a demonstrated
binding design; it is not already provided by 0051. No implicit ambient fallback
or weakening of Scope's wrong-thread/retired-context checks is authorized here.

Builder counts follow the demonstrated binding design. With current captured
scopes, worker-local registration needs N lifecycle-builder invocations for N
workers; fresh handler-local registration needs one per handler context created,
not automatically N. A different design separating registration from state
binding must prove that separation and count both operations. Record counts for
startup, multiple handlers, handler failure and shutdown, alongside context and
unit build counts. The chosen server entry must document its exact contract;
existing CLI/session/worker entry points keep their once-per-process promise.

A server-specific factory is a candidate, not an API accepted by this draft.
The Rune-only scheduling probe cannot prove this assembly step. Stop for review
if assembly requires changing existing 0051 or 0053 promises rather than adding
a separately specified server entry with explicit ownership.

## Gates before an implementation record is ready

1. Review and reproduce the boundary probes. No budget slicing, no public API
   added by a benchmark, no throughput or HTTP claim from dispatch latency.
2. Prove actual rnx assembly for both serial and multiplexed admission on two
   workers. Use at least two concurrent handlers per worker for multiplexing,
   and enough awaiting requests to fill each shape's active limit. Record healthy
   latency, completion rate, build costs and memory, with separate awaiting, CPU
   and saturated cases and stated sample counts/tolerances. Inventory and count
   the shared/per-worker/per-handler layers in decision 4. Prove a started
   tracked operation on worker A survives a failure on worker B and subsequently
   completes; also prove survival across a peer handler's failure on the same
   worker in the multiplexed shape. The failed handler's own operation must be
   revoked. Exercise HTTP cleanup isolation as well as tracked operations.
   Retire handler/worker contexts and show no retained operations after teardown.
   Builder and compilation counts are assertions, not inferred from timings.
   Stock entry points retain their prior behavior. Select the admission shape
   only after these results, before committing its implementation contract.
3. Fix the HTTP boundary: request/response types and bounds, routing, queue
   capacity and overflow, request deadlines and interruption, server startup
   and signal handling. Select the HTTP library from those requirements.
   The current probes intentionally choose none.
4. Repeat the measurements through real loopback HTTP, keeping awaiting-slow,
   CPU-slow and saturated cases as separate rows for the chosen shape. Observe
   request admission from outside the executor. State tolerances and include
   queueing in latency. Native blocking remains a stated limitation.
5. Integrate rollback-before-reuse with the real pool/transaction path. Include
   rollback failure and retirement, cancellation during an active command,
   connection loss and the chosen commit-ambiguity policy. A second connection
   must observe the results; a client-returned error alone proves nothing.
6. Exercise shutdown while idle, awaiting, CPU-bound and executing SQL, with
   accepted and queued requests. Require the stated shutdown outcome, descriptor
   and task accounting, and separately measured backend lifetime. No leftover
   fixture processes. OS-blocking/native stalls need their own honest residual.
7. Recheck root behavior/startup only when root implementation changes. The
   present probe changes no root executable, dependency graph or runtime path.
   Windows and other platforms remain unexecuted by this Linux evidence.

## Guardrails and risks

No package manager or generic web framework in this boundary draft. No
PostgreSQL driver added to stock rnx. No new lifecycle primitive merely to
make the prototype convenient. Review notes remain gitignored.

Bounded workers can all be occupied; threads cannot forcibly and safely stop
an arbitrary native call. A connection that is rolling back still consumes
capacity. Statement timeout starts per server command and is not a whole
request deadline. The probe's explicit barriers establish ordering but are
not a production queue implementation. Pool maintenance and recovery behavior
are not supplied by a single connection-driver task. These are the remaining
engineering boundaries, not gates the measurements passed implicitly.
