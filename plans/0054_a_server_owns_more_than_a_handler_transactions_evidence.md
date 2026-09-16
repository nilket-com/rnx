# 0054 gate 5: transaction completion outlives the handler

Measured by Codex on Linux, 2026-09-16. **Two eight-case repeats pass; pending
review.** rnx-bench `b0d1cf1` contains the prototype and raw results. It builds
an archive of rnx `1b2030d`; stock source, manifest, lockfile, notices, adapter
and kernel are untouched. No public pool or server interface is added.

## Contract and reproduction

`probes/server-transactions/` is an explicit private copy of the accepted HTTP
prototype at bench `6d146e2`, plus `pool.rs`, a database fixture route and a
fault proxy. Its README gives build and reproduction commands and the precise
ownership/ambiguity policy. Results are in
`results/server-transactions-0054/run-{0,1}/`; `wire-regression/` reruns the
57 accepted HTTP cases with the pool enabled. All pass on this binary:

`8110d3d367884f2c02e5396b343197abe413e3988ab13dc8c9ece4c4e7e7a797`

The lock starts from the HTTP prototype's accepted graph. No baseline package
version was removed; 37 packages were added for tokio-postgres 0.7.18 and its
resolved graph, including non-Linux targets. Full features/edges and declared
licences are inventoried. Additional shipped Linux licence texts accompany
the archive's root notices. Final builds were locked and offline. An initial
unseeded graph was discarded before measurement because it selected unrelated
updates; it is not the recorded build.

Each case retains source/build/graph/lock/binary hashes, exact manifest and build
command, server events, proxy frames' command/barrier observations, RSS/FD
samples and asserted outcomes. Repeat results also hash the cluster helper and
Python drivers. All final source hashes were verified against their receipts.
Raw test-runner logs and licence texts retain their original trailing whitespace.

Every repeat creates its own PostgreSQL **18.6** cluster, Unix socket directory
mode 0700, no TCP listener, and a separate observer through psql. The proxy is
inside that private directory. The system cluster is untouched. The shared
cluster guard covers startup, exceptions and SIGINT, and stops/reaps its
postmaster. Proxy reader threads are shut down and joined before the cluster
ends. No public network service is used.

## What the server owns

Each worker has two connections and one driver task per connection, with all
four established before the listener opens. No eviction/reaper task exists in
this minimal pool. The idle-driver count is asserted against Tokio's live-task
metric: two on each worker while there are zero handler leases. Every driver
JoinHandle belongs to the pool/lease owner and is explicitly awaited.

The compiled Unit remains shared; each handler constructs its own real battery
context, runtime context, lifecycle and HTTP state. A lifecycle-tracked native
query borrows the exclusive lease's client. The VM, tracked query and registration
captures end before transaction completion. An HTTP active credit and the lease
stay charged through acknowledged rollback/commit or retirement and replacement.

The first application owns transaction boundaries; its script cannot issue
arbitrary BEGIN/COMMIT. Successful response validation leads to COMMIT; VM error,
budget failure or pre-completion cancellation leads to ROLLBACK. A lost COMMIT
acknowledgement is always conservatively ambiguous, even on a server error in
this first prototype. There is no automatic retry or owner-side query to resolve
it. Disconnect after COMMIT dispatch does not replace COMMIT with ROLLBACK.
The request owner logs the ambiguity and returns the existing generic HTTP 500
if possible. This is not an exception delivered to an already-ended Rune VM.

The acknowledgement deadline is 1.2 s, distinct from the server's 800 ms
per-command statement timeout and HTTP's 2 s admitted deadline. Query drop sends
no PostgreSQL CancelRequest. Lease wait cancellation occurs before connection
ownership; after BEGIN, cancellation requires completing cleanup. Driver
retirement has a separate 1.2 s limit; forced abort-and-join then fails the
fixture, rather than declaring success. That deadline failure is not exercised
here and remains part of the shutdown work.

## Loss is injected at an observed boundary

The fixture-only proxy forwards an actual ROLLBACK/COMMIT, then withholds the
first server response and subsequent reply bytes. The owner is still awaiting
acknowledgement. A second connection reads the actual database state while the
HTTP active credit remains charged and no teardown or acknowledgement event
exists. The fixture either releases the reply or cuts transport; for rollback
loss it first terminates that backend through the observer.

This deliberately tests **failure to acknowledge rollback**, not a claim that
PostgreSQL is still executing ROLLBACK when killed. The database can already
have rolled back. The owner must still retire the unsynchronized connection.
COMMIT uses the same boundary: the fixture observes a committed row, cuts the
reply, and requires the owner to keep its ambiguous answer. The observer's
knowledge is never passed to the owner. Assertions establish the boundary;
there is no race timed by an arbitrary sleep.

## Results

| Case | Repeat 0 backend before → next on same worker | Repeat 1 | Required observation |
| --- | --- | --- | --- |
| Handler failure, rollback acknowledged | 2053120 → 2053120 | 2053425 → 2053425 | No row, acknowledged rollback, same connection reused |
| Rollback acknowledgement lost | 2053149 → 2053167 | 2053459 → 2053471 | Error, retirement, newly connected replacement |
| COMMIT acknowledgement lost | 2053191 → 2053202 | 2053496 → 2053507 | One committed row and ambiguous error; one COMMIT, no retry |
| HTTP disconnect during COMMIT | 2053228 → 2053237 | 2053531 → 2053541 | Cancellation observed before reply loss; same ambiguity policy |
| Cancel active SQL | 2053264 → 2053264 | 2053567 → 2053567 | Query observed active; rollback acknowledged before reuse |
| Backend loss during active SQL | 2053299 → 2053309 | 2053614 → 2053628 | Query/rollback failure, retirement and replacement |
| Whole-budget halt after INSERT | 2053334 → 2053334 | 2053653 → 2053653 | Row absent, rollback acknowledged, same connection reusable |

Every replacement PID is absent from that process's initial four-connection
pool. Subsequent writes are successful and row counts exclude replay of the
failed/ambiguous operation. The proxy records exactly one COMMIT on the lost
commit's backend. Reusing a specific replacement is an assertion of this
sequential fixture, not a general scheduling promise for concurrent borrowers.

The eighth case reaches eight active HTTP credits with four sleeping database
leases and four permit waiters. Exactly four handler contexts exist. Cancelling
all requests refuses those four waiters without a lease/context, rolls back the
four executing transactions, leaves no rows, and a subsequent request succeeds.
The pool wait consumes existing bounded admission; it is not a second unbounded
queue or a reason to open more connections.

Cancellation of active SQL takes **796 / 793 ms** from rollback dispatch to
acknowledgement in the two repeats. That is a fixture observation consistent
with waiting behind the 800 ms statement timeout, not immediate cancellation.
Other barrier timings include observer/proxy coordination and are not benchmarks.
The fixture allows three seconds for observation and five seconds for a result;
no performance claim follows from these numbers.

## Idle ownership, close and remaining gates

After settled handlers and HTTP connections disappear, each of the first seven
cases observes five server socket descriptors: listener plus four pooled
connections. Four tagged PostgreSQL backends remain, deliberately owned by the
pool. Driver tasks survive the handler contexts; that is the new ownership
layer this gate exercises.

Each repeat builds and retires **34 contexts across eight server processes**.
Pool close sees zero leases, closes idle clients, awaits every driver, and
reports zero pool drivers and runtime tasks. The existing server close then
joins workers and asserts zero newly owned sockets, connection permits and
active credits. The independent observer separately waits for zero tagged
backends. Both repeats' private postmasters are stopped and reaped. The 57-case
wire regression also passes with the pool alive throughout and closed at exit.

These exits occur after the transaction cases settle. They **do not close
gate 6**: shutdown during active SQL, pending rollback/COMMIT, CPU/awaiting work
and queued admission needs its own matrix and deadline-failure outcomes.
The public server entry remains undecided. General pool session-state reset,
arbitrary script transactions, a production pool library, pooling performance
and non-Linux execution are outside this evidence.

Rust formatting and Python syntax checks pass. Root suites/startup were not
rerun because no root implementation or dependency graph changed. Gate 5 is
ready for review; no new primitive has been added to Scope or Extensions.
