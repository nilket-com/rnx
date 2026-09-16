# 0052 ownership evidence: a retained future is not dropped

Status: **stopped at the ownership gate**, not implemented. Codex,
2026-09-16, Linux/nano. Plan `cb4f519`, rnx implementation `aee1d83`.
Reproducer and raw exports: rnx-bench `fbc08a9`, `probes/postgres/` and
`results/postgres-0052/`. Prototype driver pinned to tokio-postgres 0.7.18.
Exact tool versions, executable/lockfile hashes and resolved package/licence
inventory are in ownership-versions.json and ownership-dependencies.json.

## What was built and measured

A separate bench workspace assembles rnx with one prototype native function.
Its async call connects with NoTls, sets a per-connection statement timeout,
and polls the driver Connection future alongside the statement future inside
the call. It never spawns the connection and never uses a private rnx API.
A synthetic conversion refusal after receiving a real row tests destruction;
this is not an implementation of the adapter's complete decoding contract.

The Python parent starts a private PostgreSQL 18.6 cluster in a temporary
0700 directory, Unix socket only, and launches the actual assembled rnx worker.
Every tested query runs in a Rune script. For started calls, the fixture first
observes the socket and the active server query. It then inspects the worker's
`/proc/<pid>/fd` at settlement, before acknowledging the worker or sending any
further input. The protocol keeps the worker waiting at that point.

Trace instrumentation records Tokio's task count and the process's socket count
when the host future acquires the connection and when its locals finish dropping.
The trace guard holds a runtime Handle only for metrics; it does not poll or
schedule anything. Both counts are zero after the successful drop cases. This
matters because session interruption calls the existing HTTP cleanup: its
`drain_http` returns without another block_on when there are zero tasks. The
prototype cannot accidentally pass by having that cleanup abort/drain a spawned
connection. Source contains no spawn or extra cleanup turn.

## Seven agreed cases pass

| Case through Rune | Socket before / started / after | Observation |
|---|---|---|
| Successful query | 0 / 1 / 0 | Returns 42 |
| Caught deadline | 0 / 1 / 0 | Rune match catches deadline error |
| Conversion refusal | 0 / 1 / 0 | Err after receiving a row |
| Interrupt while awaiting | 0 / 1 / 0 | Worker settles interrupted |
| Pending bound future, failed input | 0 / 1 / 0 | select releases its borrow; observation sleep; panic discards new binding |
| Budget halt | 0 / 1 / 0 | Same held-future window, then real two-billion-instruction exhaustion |
| Lazy future | 0 / never opened / 0 | Successful input retains an unpolled future |

The failed-input and budget cases prove the socket still exists during the
sleep after select, before the failure. Only after the cleanup check do they
send an input proving q was not published. The final budget case takes about
12.8 seconds; it is the stock worker's real budget, not a replacement Rust
execution harness or injected halt.

An initial fixture mistake is recorded here rather than mistaken for a product
failure: the stream collector retains a possible barrier prefix, so a short
stderr observation marker was not exposed until the end-of-input barrier.
Padding that marker makes it visible during the observation sleep. Descriptor
inspection then proves the intended unpolled-but-open state. No extra runtime
turn was added to cleanup.

## The missing case contradicts the broader cleanup promise

Input 1 succeeds and publishes a lazy future:

```rune
let q = pg_probe::query(url, 5.0, 600, false);
```

Input 2 starts it through select:

```rune
let timer = time::sleep(120000);
select { _ = q => (), _ = timer => () };
```

The parent observes the query active, then sends SIGINT. The current input
settles as interrupted, but the descriptor table still contains the **same
socket inode**. No `dropped` trace event occurs. The session's earlier binding
still owns the partially polled Rune future, and dropping the current VM's
mutable borrow does not drop that owner.

In the final run, after another 800 ms of passive waiting by the parent, with
no acknowledgement, input or worker runtime turn, the socket is still open.
The server-side 600 ms timeout has fired: pg_stat_activity changes from active
to idle. It does not disappear, because the client still owns the open socket.
The host future's client-side deadline is unpolled too. There are zero spawned
tasks, so draining tasks cannot solve this case.

An earlier complete run with a 90-second timeout reproduces the same retained
socket immediately after interruption. Both runs are saved. The final fixture
exits **2** to report this stop, rather than reporting the seven narrow passes
as complete ownership acceptance.

## Why this stops implementation

Call ownership proves cleanup **if the host future is actually dropped**.
It does not prove that every interrupted execution drops the future. The
session intentionally retains previously published bindings after a failed
input. This is existing semantics, not a Rune or rnx regression introduced by
the prototype.

The HTTP battery already addresses this distinction: its private State::clear
cancels active requests even when a Rune future is retained by an earlier
binding. A native extension receives only a registration module. It has no
corresponding execution-cancellation or session-reset hook and cannot reach
the HTTP mechanism. Clearing all session bindings would change the session
contract and is not a permissible workaround.

The next design needs an explicit extension lifecycle contract: which execution
owns a pending native operation, how cancellation revokes its resources even
if a Rune value remains reachable, what happens if that value is polled again,
and what reset, worker shutdown and failure mean. A synchronous cancellation
hook/resource registry is a candidate, not an implemented API decision.
It must work without extra runtime turns for resources owned this way and must
not silently cancel unrelated calls. Alternatively the user could explicitly
accept retained pending calls surviving interruption, but that weakens 0052's
current promise and is not assumed here.

Record 0052's final guardrail says to stop and write up any needed rnx lifecycle
change separately. This is that stop. No product adapter, reset workaround,
background thread, forced session reset, or rnx library change was introduced.
The remaining adapter gates have not been run or marked passed.

## Verification and cleanup

The prototype builds locked in release mode, passes rustfmt and its own clippy
with warnings denied. Root rnx, its manifest/lockfile/notices, and the kernel
are unchanged; broad root/kernel suites were not rerun for this bench-only
prototype. Windows execution/type-check is not claimed by this Linux probe.
The fixture stops its private postmaster and verifies its process and temporary
directory are gone; ownership-cleanup.json records both. Worker processes are
closed after observations, and no system database or user kernelspec was used.

All commits remain unpushed pending review of the stop and the lifecycle choice.
