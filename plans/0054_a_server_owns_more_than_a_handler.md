# rnx 0054: a server owns more than a handler

Status: proposed 2026-09-16; revised after the first review. The boundary
probes are accepted and pushed in rnx-bench at `8b97577`. This is the step-four design draft, not an implemented server or a
claim that the existing extension interface can already assemble one. The
remaining HTTP gates and separate server-entry contract must be settled before
implementation is ready.

Gate 2's original assembly evidence is in the companion
`0054_a_server_owns_more_than_a_handler_assembly_evidence.md`. Its HTTP cleanup
stop is now closed by accepted record 0055 (`d87334f`) and the two assembly
replays in rnx-bench `92388a5`. Request ownership stays isolated, cleanup works
inside a running runtime, and final teardown reaches zero tasks after all
owners end. Decision 1 now selects multiplexed admission. Decision 5 specifies
the proposed gate-3 HTTP boundary and selects a library for its next probe;
those new details are for review, not claims of an implemented HTTP server.

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

Choose multiplexed admission: **two executor workers, at most four active
handler VMs per worker**, for eight active handlers in the first HTTP prototype.
Four is the measured limit, not a discovered optimum or an automatic function
of CPU count. A worker's four handlers each own a distinct serving context,
RuntimeContext, lifecycle and scope. They share the immutable compiled Unit
and their worker's runtime/local task set. No concurrent executions share a
0053 generation counter. Serial admission remains a measured comparison only.

The real-battery assembly measured awaiting-batch healthy latency near 330 ms
with serial admission versus 125–131 ms with four-way multiplexing; finite-batch
completion rates were 51.5/s versus 108.5–115.9/s. These are dispatch measurements,
not HTTP throughput. The price was 14.9–15.3 ms versus 93.4–103.7 ms of aggregate
context construction for the batch, and about 4.57 MB versus 7.64 MB of peak
tracked allocation growth. These are whole-fixture allocation figures, not a
per-context size or RSS. Fresh construction is on the handler's latency path.

Awaiting handlers can share a worker; a CPU-bound handler still blocks that
worker until its whole budget ends. Multiplexing does not improve CPU preemption
or saturated CPU behavior, and construction makes the spare-worker case more
expensive than the serial reused-context comparison. Context reuse is deferred:
it may reduce this price later, but must preserve independent execution ownership,
retirement, clean bindings and the same active limit. It is not assumed here.

Keep **one central FIFO of at most sixteen queued complete requests**. It feeds
workers with available active slots, choosing the least occupied worker and
alternating ties. There is no hidden per-worker backlog. Queue capacity is a
new proposed bound, chosen to hold two active batches; it is not inherited from
the assembly measurement. A full queue with no active slot produces 503 without
constructing a context. All eight active credits and all sixteen queue credits
are counted explicitly. Dequeue/expiry/disconnect returns a queue credit once.

An active credit covers context construction, execution, response conversion
and logical owner teardown. A deadline response or disconnected client does not
free a worker slot while that VM or its transaction cleanup still lives. Pending
rollback still consumes capacity. Hyper's transport tasks can need further
runtime progress after owner revocation, as 0055 states; no per-handler global
drain is added. The load gate also measures that deferred task/resource backlog.

This promises bounded admission and progress with spare capacity, not a healthy
latency bound when all workers run non-yielding code. All four slots on a worker
can be occupied by I/O; a CPU handler can delay the other three there. Neither
slot accounting nor shortest-occupancy dispatch identifies which handler is
currently using the CPU. Gate 4 must measure this mixed case too. No automatic
retry or replay of a failed request.

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

The accepted assembly prototype uses the actual rnx batteries and a
lifecycle-aware extension. Keep its demonstrated ownership layers when
specifying the still-separate server entry:

| Layer | Ownership to establish in the assembly gate |
| --- | --- |
| Compilation context and immutable Unit | One schema compilation; share the resulting Unit across handler contexts with the same native registrations, as the fixture proves. |
| RuntimeContext and registered native closures | Construct per handler on its worker, with its own captured scopes and state. The selected design does not share one stateful runtime context; sharing requires a later binding design. |
| Tokio runtime and local task set | One per worker thread, with explicit active and queued limits. |
| VM, Rune values and execution budget | One per handler; never transfer these values between threads. |
| Lifecycle state and Scope thread binding | Per handler execution in the selected shape, created on the owning worker. No shared active flag or generation counter between concurrent handlers. |
| HTTP client and request cleanup owner | A separate HTTP State per handler, used with its lifecycle scope. Reuse is within that handler; sharing a client across handlers is deferred. Revocation/release does not drain the worker runtime. |
| Process inputs | env::args captures immutable host strings, which may be shared with fresh Rune values per call. env::var and env::vars read the process environment today, not a context snapshot. Any new server snapshot policy must be stated and gated, not inferred. |

`RuntimeContext: Send + Sync` permits moving registered closures, not moving
the ownership represented by their captures. Today's HTTP registration captures
State and today's lifecycle-aware adapter captures a concrete Scope. Building
one runtime context for all workers/handlers therefore needs a demonstrated
binding design; it is not already provided by 0051. No implicit ambient fallback
or weakening of Scope's wrong-thread/retired-context checks is authorized here.

Builder counts follow the selected design: one schema construction, then one
set of builder invocations for each handler context constructed on its worker.
Rejected or expired queued requests do not construct a context. Count failed
construction attempts too. Serial worker-local registration remains comparison
evidence, not the chosen contract. A different design separating registration from state
binding must prove that separation and count both operations. Record counts for
startup, multiple handlers, handler failure and shutdown, alongside context and
unit build counts. The chosen server entry must document its exact contract;
existing CLI/session/worker entry points keep their once-per-process promise.

A server-specific factory is still a candidate, not an API accepted by this
draft. Gate 2 demonstrated private assembly, not a supported factory API. Stop for review
if assembly requires changing existing 0051 or 0053 promises rather than adding
a separately specified server entry with explicit ownership.

### 5. A small HTTP boundary, then a library to implement it

The next artifact is an isolated loopback HTTP prototype in rnx-bench, using
private assembly as gate 2 did. It adds no server command, public factory or
HTTP-server dependency to stock rnx. The following limits are proposed rnx
choices for that prototype, not properties already measured or upstream defaults.

#### Transport and admission

One dedicated coordinator thread owns the listener, HTTP connection futures,
queue, request timers and response writes on its own Tokio runtime. It never
executes Rune, constructs serving contexts or waits synchronously for worker
threads. Two executor threads run the admission shape in decision 1. Only owned
host messages and bounded response channels cross the thread boundary. This
keeps timeout/refusal processing off a CPU-bound VM's thread; it does not make
the VM itself preemptible.

| Resource | Initial prototype bound and handling |
| --- | --- |
| Listener | Plain HTTP/1.1 on an explicit numeric loopback address; port zero allowed for fixtures. No public bind, TLS or proxy interpretation in this prototype. |
| Accepted connections | 32 permits, held through read, response and socket disposal. If full, accept-and-close rather than allocate a handler/task/queue entry; at most one transient accepted socket sits outside the permits. The OS listen backlog is separate and is not called an application memory bound. |
| Requests per connection | One, then Connection: close. No keep-alive, pipelining, upgrades, CONNECT or HTTP/2 in the first gate. Keep-alive is deferred explicitly; this is a boundary probe, not a performance claim for a Flask replacement. |
| Parser input buffer | Explicit 16 KiB maximum, plus a 64-field header limit. Do not call this a complete process-memory cap or assume it limits response buffers. |
| Parsed target | At most 8 KiB; origin-form path/query only. Exceeding the application target limit returns 414. |
| Parsed headers | At most 64 fields and 16 KiB summed name/value bytes, excluding parser overhead. Repeated fields count separately. Application header-limit refusal is 431. |
| Request body | At most 1 MiB after transfer framing, counted while reading, never preallocated from Content-Length. Over-limit is 413. No content decompression; Content-Encoding other than absent/identity is 415. |
| Queue | Sixteen complete request objects globally, in addition to eight active handlers. No queue of futures waiting for queue permits. Overflow returns 503. |
| Response | At most 1 MiB body and 64 fields/16 KiB summed header bytes, validated before any response bytes. Invalid handler response becomes a small 500. |
| Response waiting/writing | One bounded reply per accepted connection; no unbounded completion channel. Release a response when its client or write deadline ends. |

These bounds cover application payloads and counts. A Rune VM can allocate
beyond the returned body, and the allocator, parser frames, conversions, socket
buffers and background tasks add memory. They are not a 32 MiB RSS guarantee.
Gate 3 measures retained/growth behavior at each cap, including expired handlers
whose clients have gone. They still occupy active credits. Slow clients must
not allow unbounded response accumulation as those credits are reused.

No body enters the queue until it has passed the body cap. A declared oversized
Content-Length may be rejected early; a smaller declaration still governs HTTP
framing, not an invitation to treat trailing bytes as part of that body. No
second request on that connection is admitted. Truncated/invalid framing is
refused or closed. Chunked bodies get the same counted cap. Request trailers
are refused, and their parser allocation must be bounded before that refusal.
Reject Expect with 417 before polling the body, and reject protocol switches
rather than constructing upgrade tasks. Malformed requests the HTTP parser
cannot expose safely may receive its bounded error response or connection
close; the raw-wire gate records which. No universal custom diagnostic is
promised before a valid request head exists.

#### Owned data and routing

The provisional fixture handler takes one object:
`#{method, path, query, headers, body}`. Method, path and query are strings;
query is the original unparsed query without `?`, or unit when absent. Path
keeps percent escapes, with no percent decoding, slash collapsing or filesystem
normalisation. Headers are lowercase names mapped to lists of Bytes, preserving
repeated values in received order and non-Unicode field values without loss.
Body is Bytes. No connection, stream or library-native request object reaches
Rune. This shape is a prototype contract, not an additional published module.

Four exact method/path pairs exercise healthy, awaited-slow, CPU-slow and
transaction-failure handlers. An unknown path returns 404; a known path with an
unsupported method returns 405 and a host-generated Allow field. No implicit
HEAD, redirects, path parameters, middleware or content negotiation. Routing
uses the path only; the query remains data. Ambiguous/duplicate route definitions
are a startup refusal. No SQL text is synthesized from a URL by the framework.

A successful handler returns exactly `#{status, headers, body}`. Status is an
integer from 200 through 599; headers have the same name-to-list shape, with
String or Bytes values; body is String (UTF-8 bytes) or Bytes. Unknown/missing
fields, invalid names/values, disallowed framing headers or an oversized result
refuse the response with 500. Status 204 and 304 require an empty body. The host
owns Content-Length and Connection and refuses handler-supplied Content-Length,
Transfer-Encoding, Connection, Trailer, Upgrade, Keep-Alive, TE and
Proxy-Connection, compared case-insensitively. It computes
framing from the validated body. JSON is explicit `json::parse`/`json::stringify`,
not a second serializer or an implicit conversion of arbitrary Rune values.

Handler failure, budget exhaustion, context-build failure and invalid response
produce a bounded generic 500. The host diagnostic names the request and real
source location where available; it is not reflected into the HTTP body. No
successful prefix is sent before full response validation. Script output still
follows the existing stream APIs; request-attributed logging is not supplied
implicitly by this experiment.

#### Clocks and shutdown

- Head deadline: five seconds from accept, including an incomplete first byte
  or head. Close on expiry; no Rune work has been admitted.
- Body deadline: five seconds from valid head, absolute rather than restarted
  per byte. Return 408 when a response is still possible, then close.
- Admitted-request deadline: two seconds from complete validated body, including
  queueing, context construction, VM work and response conversion. The whole
  handler instruction budget is 10,000,000, as in the accepted probes. Expiry
  removes queued work or requests cancellation of a running handler and sends
  one 504 if possible. A late completion is discarded, never another response.
- Write deadline: one second from response dispatch. Close a slow reader; do
  not let it retain a response indefinitely. This is separate from VM execution.

These are different clocks, named separately in evidence. A CPU loop or blocking
native call cannot be dropped while its poll is on the stack. The coordinator
can time out the client, but the active slot stays charged until worker-side
execution and logical cleanup finish. Cancelling one request never sets rnx's
process-wide interrupt flag. It uses that handler's cancellation state and
lifecycle. A detected disconnect removes queued work or requests the same
cancellation; detection of a silent peer is bounded only by the relevant timer.
An ambiguous remote write is not retried or called rolled back because of 504.

Compile the entry and verify the route schema, start both workers and install
shutdown handling before reporting the actual bound address as ready. Startup
failure is a named nonzero failure, with resources unwound and no ready report.
Ready means the server can admit requests, not that the database is healthy.
SIGINT/SIGTERM in the Linux prototype stop admission and request shutdown; they
are not a per-request interrupt. The prototype must integrate rnx's existing
process-wide interrupt initialization rather than silently replace it with a
competing handler. Pending reads/connections close, queued requests
get 503 where possible, and active requests are cancelled. Transactions follow
decisions 2–3, not a drop-equals-rollback assumption.

Give normal shutdown a five-second overall allowance. Keep running transport
and worker runtimes while cleanup progresses. Only after all handler owners and
server-owned database resources end may each worker perform whole-runtime drain.
Clean exit requires worker joins, connection-task completion and the descriptor/
backend observations in gate 6. On expiry, record which owners remain and report
failed shutdown. Do not detach them and call that a clean join. The prototype's
parent may terminate a stuck process to contain a deliberately blocking-native
fixture; that is failed cleanup, with remote backend lifetime observed separately.
The eventual standalone server's hard-exit policy needs its entry-point record;
this draft does not authorize a library function to terminate an embedding host.

#### Library selected for the boundary probe

Use **hyper = 1.11.1**, server + http1 with defaults disabled, with
**hyper-util = 0.1.20** for Tokio I/O/timer adapters and
**http-body-util = 0.1.5** for body utilities. These versions are already in the
root lockfile through its client graph, but the server-feature graph is new:
the isolated prototype must pin and record its own complete graph and licences.
This selection does not add server features to root rnx or claim those features
have been exercised by the HTTP client battery.

The deciding fit is an owned accept loop and per-connection driver: explicit
parser/header limits, a supplied timer, disabled keep-alive and an independently
polled shutdown path. Hyper exposes those controls. Its connection shutdown
method still requires polling to finish; it is not a synchronous close guarantee.
See the [HTTP/1 builder](https://docs.rs/hyper/1.11.1/hyper/server/conn/http1/struct.Builder.html)
and [connection source](https://docs.rs/hyper/1.11.1/src/hyper/server/conn/http1.rs.html).
The installed 1.11.1 source was checked as well; no unstable default is adopted
as the contract.

Axum remains a viable later routing layer. Its convenience
[serve API](https://docs.rs/axum/latest/axum/fn.serve.html) does not expose these
connection settings; using Axum with a custom Hyper loop is possible but adds
routing machinery the four exact fixture routes do not need. Its Send service
bound is not a disqualification: only owned host dispatch futures live on the
coordinator, while Rune stays on executor workers. A handwritten HTTP parser
would instead create protocol work with no demonstrated need. No performance
comparison between these libraries has been measured or claimed.

Body helpers do not replace accounting. In the pinned http-body-util source,
Limited checks data after receiving a frame and passes non-data frames through.
It does not prove a parser allocation bound, cap trailers, or limit the queue.
The prototype must combine incremental body accounting with the upstream parser
bounds and test oversized lengths/chunks/trailers before asserting boundedness.
Library adoption remains contingent on gate 3's wire/resource evidence.

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
3. Prototype decision 5's HTTP boundary with the selected library and all
   explicit caps. Raw TCP gates must exercise oversized/fragmented heads, slow
   headers and bodies, false Content-Length, excessive chunks/trailers, queue
   saturation, connection saturation, slow response readers and disconnects
   while queued/running. Prove one response at most, no handler for a rejected
   request, bounded credits/queues and continued coordinator responsiveness
   while executor workers are CPU-bound. Record statuses versus connection
   closes, source/graph/licences, allocation/RSS observations and exact clocks.
   Stop if a claimed bound cannot be enforced without an unbounded staging
   buffer, hidden task queue or a second HTTP parser. Available builder methods
   do not themselves pass this gate.
4. Repeat the measurements through real loopback HTTP, keeping awaiting-slow,
   CPU-slow and saturated cases as separate rows for the chosen shape. Add a
   mixed case with a CPU loop and awaiting handlers sharing one worker. Observe
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
