# 0054 gate 6: shutdown, including the outcomes that fail

Measured by Codex on Linux, 2026-09-16. **Both 13-case matrices pass their
expected outcomes; pending review.** rnx-bench `453e618` contains the prototype,
drivers and raw evidence. It archives rnx `beffe80`; stock source, manifests,
lockfiles, notices, adapter and kernel are unchanged. No supported server entry
or new lifecycle API is added.

## Reproduction and scope

`probes/server-shutdown/README.md` gives the commands and exact assertions.
This is a private copy of the accepted gate-5 prototype at bench `b0d1cf1`.
`Cargo.lock`, `graph.json` and additional dependency notices are byte-identical.
The only Rust changes add explicit shutdown accounting and failure observations,
move the five-second clock before transport disposal, and add two labelled
fixture injections. The transaction policy and all numeric limits are unchanged.

Results are in `results/server-shutdown-0054/`:

- `run-0/`: thirteen rows under SIGTERM;
- `run-1/`: the same thirteen under SIGINT, through rnx's existing interrupt flag;
- `transaction-regression/`: the eight accepted gate-5 cases;
- `wire-regression/`: all 57 accepted HTTP cases with the pool enabled.

All pass on the same release binary, SHA-256:
`9374ca2a320fe378602fb9d531c89eaaa7f14d64d6428b5172c80f44ac3a1c2c`.
Receipts retain source, driver, graph, lock, executable and parser hashes, exact
injected manifest and build command, toolchain, signals and test environment.
All final hashes were checked against the committed sources. Builds were locked
and offline. Rust formatting and Python syntax checks pass; root suites/startup
were not repeated for unchanged implementation. Raw logs and licence text keep
their original whitespace.

Each matrix/regression driver owns a fresh PostgreSQL 18.6 cluster, Unix sockets
in a mode-0700 directory, with no TCP database listener. Each row has a fresh
server and proxy. The system cluster is untouched. Private postmasters are
stopped and reaped by the shared guard, proxies close sockets and join their
threads, and no fixture process remains. The matrix drivers and regressions ran
concurrently in separate private clusters; these are correctness observations,
not isolated performance comparisons.

## Evidence before each signal

A signal is sent only after the relevant event/database state is observed:
await start; CPU poll start; active pg_sleep; a real COMMIT/ROLLBACK reply at the
proxy barrier; or the full active/queued admission state. Server event ordering
proves the CPU fault happens after shutdown begins. Pending completion replies
are released only after shutdown has begun. For acknowledgement-deadline rows,
the proxy waits for the owner's recorded timeout before releasing anything.

Every row proves the listener refuses a new connection after the shutdown event.
Partial-read sockets, active HTTP connections and queued requests are disposed;
EOF/reset is allowed and no late successful response is promised. The queued
rows record the sixteen discarded ids and assert none constructs a context.
The SQL queue row additionally observes four active SQL leases and four pool
waiters; only the four leased handlers have contexts.

## Outcomes

The table uses the server's monotonic event interval from shutdown to its clean
close or explicit failure. Process startup is excluded. Two signals are two
runs, not enough samples for a performance claim.

| State | SIGTERM | SIGINT | Outcome |
| --- | ---: | ---: | --- |
| Idle pool | 3.16 ms | 2.70 ms | Pool drivers awaited, workers joined |
| Incomplete head/body | 2.81 ms | 4.08 ms | Transport disposed, no handler built |
| Awaiting handler | 4.21 ms | 3.97 ms | Handler cancelled and retired |
| CPU handler | 59.95 ms | 57.04 ms | Whole budget halts, then ownership ends |
| Active SQL | 782.83 ms | 786.87 ms | Rollback acknowledged, INSERT absent |
| Eight awaits + sixteen queued | 5.18 ms | 7.68 ms | Active cancelled; queued never built |
| Four SQL + four pool waiters + sixteen queued | 729.87 ms | 743.29 ms | Four rollbacks acknowledged; waiters/queue cancelled |
| Pending rollback acknowledgement | 7.04 ms | 8.63 ms | Acknowledged after shutdown, row absent |
| Pending commit acknowledgement | 7.84 ms | 4.22 ms | Commit preserved and acknowledged, row present |
| Rollback acknowledgement deadline | 1196.54 ms | 1197.83 ms | Request failed, connection retired, resources closed |
| Commit acknowledgement deadline | 1203.27 ms | 1203.71 ms | Ambiguous request, connection retired, committed row present |
| Driver retirement deadline | 1204.94 ms | 1205.27 ms | Forced abort-and-join, failed shutdown, no clean-close event |
| Overall deadline during native poll | 5000.99 ms | 5001.95 ms | Unjoined owner reported, failed shutdown, no clean-close event |

The completion clock starts before the shutdown signal, so its interval from
shutdown can be slightly below 1.2 s. The fixture separately checks command-start
to timeout at 1.1–1.7 s. It checks overall failure at 4.8–5.5 s from shutdown.
Normal rows have a 5.5 s outer allowance, and the process watchdog is seven
seconds. The native-failure outer observation must finish before 6.5 s, when
the deliberately stalled call would otherwise return.

Acknowledgement loss is a request failure, not automatically a resource-cleanup
failure. The owner can retire the uncertain connection and subsequently join
all tasks, producing exit zero without calling that transaction successful.
Both acknowledgement-deadline rows record their errors and retirement; neither
records a late acknowledgement. COMMIT remains ambiguous to the owner although
the independent observer sees its row. No retry/reconciliation was added.

## Negative paths are not clean shutdown

The driver injection parks a task after its actual connection future ends,
leaving its counter live. At the 1.2 s retirement deadline the owner aborts and
awaits that task; the event names worker, backend, cleanup reason and the
remaining driver count. Both workers then fail. Other work disposed by runtime
unwinding is not claimed to have been explicitly joined. The coordinator joins
the failed worker threads, records worker failure, and never emits `closed`.

The native injection sleeps 6.5 s synchronously inside a poll. At five seconds
the coordinator still answers the ownership question: request 1, one active
credit, its unjoined worker and remaining owned socket identities. Connection
tasks have already been disposed. The native-end event never arrives in either
run. The coordinator fails; test-process termination ends the native thread.
There is no claim of thread preemption, a successful join, or arbitrary native
call cancellation.

Both negative rows exit **101**, the Rust test harness's failure status. This
is not a new product exit-code decision and not permission for an embedding
library to kill its host. The independent parent reaps the process and then
requires backend disappearance. Failure containment is kept distinct from a
clean ownership report; no negative row emits `closed`.

## Local and remote owners are observed separately

Every resource-clean row requires equal context builds/retirements, zero
active credits and connection permits, zero newly owned sockets and unchanged
inherited descriptor identities. Each worker first reports its pool at zero
leases, idle clients, drivers and live runtime tasks, then completes the final
runtime drain and joins. The eleven clean rows in each matrix build and retire
30 contexts. Negative rows deliberately do not make those assertions.

A separate thread invokes psql directly against the private cluster, bypassing
both the pool and fault proxy. It records backend PIDs, state and query before
and after the signal, at roughly 15 ms plus psql invocation cost. It must see
zero tagged backends and the expected audit row count before cluster teardown.
Backend disappearance is not inferred from a zero descriptor count or process
exit. It may be observed before the parent's process wait returns.

Raw external signal-to-exit observations include process-wait polling/reaping
and the close helper writing artifacts; they are not exact process-exit timing.
For example, active-SQL outer observations are 832 / 822 ms while the server
cleanup intervals are 783 / 787 ms. Overall-failure outer observations are
5028 / 5031 ms. Both clocks are retained rather than conflated.

## What is still undecided

Gate 6 is ready for review. The supported server-entry contract still needs to
specify assembly, API and failure ownership, especially the standalone versus
embedding-host policy when a native worker cannot be joined. These probes do
not install a server command, change Extensions or turn Scope into a pool owner.

For that contract, retain the review refinement: a definitive PostgreSQL COMMIT
rejection, such as a deferred constraint or serialization failure, should be
distinguished from a lost/malformed reply. The accepted prototype conservatively
labels all COMMIT failures ambiguous. This matrix exercises lost replies and
deadlines, not that future finer classification. General pool session reset,
arbitrary script-controlled transactions, pooling performance and non-Linux
execution remain outside this evidence.
