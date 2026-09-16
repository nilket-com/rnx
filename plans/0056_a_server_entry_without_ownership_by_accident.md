# rnx 0056: a server entry without ownership by accident

Status: plan committed 2026-09-16 after acceptance of record 0054 gates 2 through 6.
Gate 1 is accepted and pushed at rnx `abc9bdd` and rnx-bench `99f5830`;
evidence is in `0056_a_server_entry_without_ownership_by_accident_evidence.md`.
Gate 2 is accepted and pushed at rnx `a12fd06` and rnx-bench `32ad598`, with
evidence in `0056_a_server_entry_without_ownership_by_accident_host_evidence.md`.
Gate 3 is accepted and pushed at rnx `1aa600e` and rnx-bench `181a444`, with
evidence in `0056_a_server_entry_without_ownership_by_accident_commit_evidence.md`.
Gate 4 is accepted and pushed at rnx `3f8f6af` and rnx-bench `dede5d3`;
its extraction evidence is in
`0056_a_server_entry_without_ownership_by_accident_extraction_evidence.md`.
Gates 5 and 6 are accepted after the README packaging fix and final suite
reruns at `bd3dc04`; evidence and the independent timing counter-measurement
are in `0056_a_server_entry_without_ownership_by_accident_acceptance_evidence.md`.
Record 0056 is closed on Linux. Non-Linux execution, context caching and
framework ergonomics remain deferred; the stated native/CPU limitations remain.
This specifies the supported entry and its first assembled application. The HTTP,
scheduling, transaction and shutdown evidence lives with 0054. This record
turns the private assembly into an external caller's contract. Review precedes
implementation; the API assembly gate precedes extraction of the server.

## Context

The prototypes compile one program and construct an independent battery and
extension context for each handler. They use private rnx APIs through an
archived test build. Publishing those internals would expose considerably more
than an application needs. Conversely, `main_with(Extensions)` owns CLI parsing,
signals, presentation and execution; it cannot serve as a handler entry.

Two existing details make a separate entry necessary. The core installer arms
the process interrupt handler. The registered `process::exit` consults a global
script flag, so merely declining to set that flag is not an embedding policy.
Neither behavior may leak into an execution API that promises not to take over
its host process.

Gate 6 also settled a limit, not a cleanup mechanism: a worker inside a blocking
native poll cannot be joined by the five-second deadline. Only the standalone
executable may choose process termination. A library cannot make that choice
for its caller or call an unjoined thread cleanly shut down.

## Decisions

### 1. Execution support in rnx; HTTP and PostgreSQL outside its stock build

Add an opt-in `server-runtime` feature exposing a small `rnx::server` module.
Its job is compilation, one handler execution and explicit context retirement.
It owns no listener, worker thread, process signal handler or PostgreSQL pool.
The stock binary and existing `main_with`, `Extensions` and `Scope` contracts
remain unchanged. No server feature, PostgreSQL dependency or new default
runtime path is added to stock rnx.

Extract the accepted coordinator, workers and pool into a separate workspace
at `servers/http-postgres/`, with its own lockfile and notices. Its first
executable is an assembled application, not a new stock-rnx command. The Rust
application declares exact method/path/function routes and the extension
factory; the Rune entry file provides the handlers. The checked-in example
includes a healthy request, an awaiting request, a CPU-bound request and a
transaction that fails. No route-definition language, package manager or
generic public pool/transaction trait is introduced in this record.

This intentionally makes the root execution boundary reusable before making
all of the server machinery an embedding library. A general in-process HTTP
server handle is deferred. An embedding host can use the execution boundary
under its own scheduling and shutdown policy, or supervise the standalone
executable. Neither route promises safe cancellation of an arbitrary native
call inside the host process.

### 2. Compile once, prepare locally, execute once, close explicitly

The proposed execution surface has three opaque types: `Program`, `Invocation`
and `Failure`, inside the feature-gated module. Rune remains accessible through
rnx's existing re-export. The assembly gate type-checks this shape before it
becomes a supported interface:

- `Program::compile(entry, extensions)` reads and compiles the entry and its
  modules once. It returns owned diagnostics on failure, never prints them.
- `program.prepare(extensions, handler, argument, budget)` constructs a fresh
  runtime context on the calling worker and returns an `Invocation`. The
  argument is one Rune value created on that worker. The budget must be in
  `1..=usize::MAX - 1`; zero and
  `usize::MAX`, Rune's no-budget sentinel, are refused as in the runner.
- `invocation.run().await` executes once and returns a Rune value or `Failure`.
  Its future borrows the invocation. Abandoning that future ends this execution;
  the invocation remains closeable and close observes that failed finish, not
  a fresh successful execution. Calling run again refuses, including after a
  budget halt. No resume API exists.
- `invocation.close()` consumes the invocation and returns a cleanup result.
  It retires lifecycle operations and releases the HTTP owner synchronously;
  it neither enters a runtime nor waits for unrelated tasks.

`Failure` provides a stable category, a message and optional owned source
location/excerpt accessors. Categories distinguish preparation refusal, VM
failure and cleanup/state loss. Callers must be able to treat state loss as
fatal without parsing a sentence. No compiler context, battery installer,
lifecycle registry or HTTP state becomes public.

Compilation uses 0050's entry-root rule and aggregate 8 MiB read allowance.
Diagnostics use the actual source identity, including missing-method recovery
against only the faulting source. The program retains the source information
needed after compilation. Share the immutable unit and source data; do not
share a runtime context or native closures. `Program` must be demonstrably
Send and Sync; `Invocation`, its values and execution stay on their owner thread.

The caller supplies and drives the runtime. Run applies one whole instruction
budget, as in 0054, with no claim of time slicing or interruptibility inside a
native poll. Cancellation by dropping the run future finishes the lifecycle
as failed. A returned catchable error value is not a VM failure. Closing always
retires all remaining operations, including those retained after a successful
execution. Cleanup failure is sticky and returned by close.

Drop is a safety release, not an acknowledgement of successful cleanup. The
server must explicitly close every prepared invocation on every outcome and
must not convert a destructor failure into an ordinary HTTP 500 and continue.
It first converts a successful response into bounded owned host data and drops
the Rune result, then closes the invocation, then completes the transaction.
An explicit-close failure fails the server and forbids COMMIT. Gate 1 must
prove this ordering, including an abandoned run future, without unsafe lifetime
workarounds or an extra runtime turn to dispose tracked operations.

### 3. Fresh extensions and a context-specific host policy

The assembled application owns a factory that creates a fresh `Extensions`
value for schema compilation and for each handler preparation. Existing FnOnce
builders are not cloned, rerun or made Send. The factory constructs worker-local
captures on the worker; it never moves a concrete Scope from another context.

Registration names, types and signatures must agree between the schema and
handler contexts. This is a trusted-adapter obligation, not a property Rune's
public API lets rnx exhaustively verify. The schema factory registers the same
functions without acquiring a database lease or starting maintenance tasks.
Handler factories may bind a request's lease. Count one schema build and one
builder set per attempted handler context; rejected and expired queued requests
build none. Failed construction is counted and all partial owners are retired.
The first external fixture proves these counts and cross-handler isolation.

Split CLI interrupt initialization from internal battery registration. Existing
CLI/session/worker callers retain their initialization order. The new execution
entry installs no signal handler, reads no presentation config, resets no
process interrupt flag and writes no diagnostic to standard streams.

In these new contexts, `process::exit` always returns a catchable refusal
naming the server/embedding context, even if another code path set the global
script flag. It must not reach the CLI's process-exit implementation. This is a
registration policy, not a change to run/eval/session behavior.

`env::args` is empty for the first server application. Environment reads retain
their existing live-process semantics. Script printing and stdin retain their
existing stream behavior; this is not a sandbox. Synchronous native functions,
including process supervision and stdin, can occupy a worker. Native adapters
remain trusted and can themselves install signals or terminate a process.
Concurrent use of `main_with` inside an embedding application is not supported;
this entry does not turn rnx's process-global facilities into isolated VMs.

### 4. Carry the measured server boundary without expanding it

Use 0054's exact limits and clocks, with no tuning during extraction: two
executor workers, four active handlers per worker, one sixteen-entry central
queue, thirty-two connections, the recorded header/body/response allowances,
and separate head, body, admitted, write and shutdown clocks. The coordinator
owns network work and timers on its own runtime. It never polls a handler VM.
Retain exact routing and documented bare-LF parser tolerance. No public bind,
TLS, keep-alive, proxy interpretation, HTTP/2 or streaming response is added.

Each route declares whether it uses the transaction fixture path. The first
application's database extension exposes the bounded operations needed by that
example, not unrestricted transaction control. Its private per-worker pool
retains two connections and separately counted driver owners. The existing
`adapters/postgres` one-query API and its lifetime contract are unchanged.

An active credit lasts through response validation, invocation retirement,
transaction completion and lease retirement. Client disconnect or a coordinator
504 does not free that credit while the worker still owns work. In particular,
a CPU or native poll can outlive the response deadline. No response implies
that the handler stopped. Worker shutdown performs the whole-runtime drain
only after handler and pool owners have ended; there is no nested block_on.

The package pins the measured Hyper stack and PostgreSQL driver, records its
resolved graph/licences, and builds independently of the root default graph.
The first supported execution platform is Linux. Existing type checks on other
platforms are not execution evidence and do not pass the server shutdown gates.

### 5. Transaction outcomes name what is known

Preserve exactly one COMMIT or ROLLBACK, with no automatic retry. Once COMMIT
is dispatched, cancellation cannot change that command into a rollback.

Separate completion outcomes:

- Acknowledged COMMIT: committed; the lease can return to the pool.
- Definitive COMMIT rejection: the server rejected the commit, with SQLSTATE;
  report rejection rather than uncertainty. Initially retire the connection.
- Lost, malformed or timed-out COMMIT completion: outcome unknown; report
  ambiguity, retire, and neither retry nor reconcile with another query.
- Acknowledged ROLLBACK: rolled back before reuse. Any other rollback outcome
  is unacknowledged and retires the connection.

Do not classify every driver error with a SQLSTATE as definitive rollback.
Gate 3 establishes which response identifies a rejected COMMIT, distinguishing
it from connection termination. The definitive cases are SQLSTATE class 23
(integrity constraint violations raised at COMMIT by deferred constraints),
and class 40 codes `40001` (serialization failure) and `40P01` (deadlock
detected). Each requires an observed rejection of the transaction and a second
connection observing the pre-transaction state. Class 40 as a whole is not an
allowlist. All other failures remain ambiguous. The driver exposes an
ErrorResponse before a later ReadyForQuery
is necessarily observed, so this first policy does not reuse rejected leases
on the strength of that error alone. Retirement drops the client and awaits its
driver under the existing deadline; replacement has a fresh backend identity.

### 6. A standalone hard exit is an executable policy

The standalone binary installs SIGINT/SIGTERM handling once. Either requests
shutdown: stop admission, discard queued requests without building contexts,
cancel admitted executions between polls, finish/retire leases, close pools,
join drivers and workers, and dispose transport. The five-second overall clock
starts before transport disposal, as in the accepted shutdown prototype.

Exit zero only after resource-clean shutdown. Individual request or uncertain
transaction outcomes can coexist with resource-clean shutdown; they are not
reported as successful transactions. Startup, owner retirement or shutdown
failure exits nonzero. Do not copy the test harness's exit 101 as a product
contract; the standalone failure status is 1.

At the overall deadline, report active credits, requests, unjoined workers and
owned sockets, suppress the clean-close event, and terminate the standalone
process with status 1. The final report is bounded and best effort: blocked
stderr must not postpone this termination. A native poll is reported as still
owned, never described as joined. Process termination is containment, not proof
that PostgreSQL has already noticed the connection loss.

Only the executable owns this policy. The execution library returns failures
and never calls process exit, kills threads or installs an automatic kill timer.
An embedding caller with a stuck native poll must retain its ownership and
choose its own process policy. This record promises no bounded in-process
shutdown API that silently detaches such work.

## Gates, in order

1. Build an external assembly fixture using only the proposed public feature.
   Compile a multi-file program once; execute concurrent handlers with separate
   contexts and tracked operations; assert schema/context/builder counts and
   cross-worker and same-worker failure isolation. Exercise cancellation,
   budget halt, zero and sentinel budget refusals, repoll refusal, preparation
   failure and destructor panic. Construct request values through the public
   Rune re-export and read response fields through Rune's public API, with no
   private JSON writer. A drop observer must prove that abandoning run leaves
   the invocation closeable and its failed finish observable, without a new
   execution or runtime turn. Prove source-accurate diagnostics, explicit close
   failure and no nested runtime.
   Stop for review if the proposed borrowing/ownership surface cannot express
   these without exposing internals or weakening 0051/0053/0055.
2. Prove the host boundary in subprocess fixtures. A preinstalled signal handler
   remains installed; no config is read; process::exit refuses after the script
   flag has been set; caller output is not polluted by library diagnostics.
   Show native blocking is the caller's unresolved work, not a hidden library
   hard exit. Generated docs and compile-fail tests pin the public surface.
3. On a fresh private PostgreSQL cluster, observe COMMIT rejection from a deferred
   constraint (class 23), serialization failure (`40001`) and deadlock
   (`40P01`). For each, a second connection must observe the pre-transaction
   state. Assert one COMMIT with no retry and replacement backend identity.
   Re-run lost-reply ambiguity and rollback-failure cases unchanged. Record the
   precise pinned-driver evidence behind the classification; stop if a claimed
   known rejection cannot be distinguished from transport loss.
4. Extract the server against that API, with no private-root test module. Re-run
   all 57 wire cases, the scheduling rows separately, transaction regression and
   the thirteen shutdown states under both signals. Keep independent backend
   observations, inherited-descriptor baselines, owner counts and no-leftover
   checks. Exercise blocked stderr at the standalone hard-exit deadline. Both
   failure injections remain test-only and must never emit a clean-close event.
5. Run the example through real HTTP from a separate client: healthy traffic
   beside awaiting and CPU work, a failed transaction and subsequent borrower,
   then shutdown. Record actual commands, binaries, graph, licences and measured
   context-construction cost. No private framework API is needed by the example.
6. Run root suites, formatting, notices and byte-identical CLI comparison cases;
   measure matched stock startup and execution against `032579a`, reporting
   regressions as such. Confirm default dependency graph and optional build
   isolation. Report platform checks exactly as performed. Update 0054 with
   implemented versus outstanding gates only after these results exist.

## Guardrails and risks

No change to the nested-async budget limitation. No automatic replay of a
handler or uncertain transaction. No runtime task count used as a per-handler
cleanup count. No global lifecycle scope or ambient fallback. No process-wide
memory or wall-clock safety promise for trusted native code, schema builders,
file reads or host I/O. The counting allocator remains process-wide, including
its restriction on applications declaring another global allocator.

The public execution surface is additive but becomes a compatibility promise;
keep the first gate ahead of extraction. The first assembled server is narrow
and its pool does not promise hygiene for arbitrary user-issued SQL session
state. General web-framework ergonomics, reusable context caches, unrestricted
pooled SQL, an in-process server handle, deployment behind proxies and package
resolution remain later decisions driven by this application's use.
