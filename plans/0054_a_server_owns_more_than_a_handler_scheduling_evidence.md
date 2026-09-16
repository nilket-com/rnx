# 0054 gate 4: scheduling measured through HTTP

Measured by Codex on Linux, 2026-09-16. **Two repeats pass; pending review.**
The prototype is rnx-bench `6d146e2`, building an archive of rnx `6fb89e7`.
No root source, manifest, lockfile, notices, adapter or kernel changed. This
adds evidence to the selected two-worker, four-active-per-worker admission
shape, not a supported server entry. Gates 5–6 remain open.

## Reproduction and clocks

`probes/server-http/README.md` gives the build and scheduling commands.
`results/server-http-0054-scheduling/run-{0,1}/` contains the two repeats;
`wire-regression/` records all 57 wire cases passing on the same binary.
The release SHA-256 is
`c7a249a7e988255062b6be7d4dcaadd21797dd8bbf88b246b76a2d557edc799e`.
Per-case conditions retain source, driver, graph, lockfile, executable and
upstream parser hashes, exact injected manifest and build command. All final
receipts match the committed sources. The dependency graph is unchanged.

Each repeat has seven samples of six scenarios, with a fresh server process
per sample. Schema compilation and thread startup precede timing. Every
handler constructs its real battery context during the measured interval.
Workers use CPUs 2 and 4, coordinator 6, client 8: distinct physical cores on
this i7-14700. Frequency/turbo are not controlled. Logging and tracked allocation
remain enabled. Scheduling does not inject the wire test's small send buffer.

Latency starts at the client before connect and ends after the whole response
and EOF. Independent reader threads begin immediately. Server request tags,
admission, dispatch, build, VM, phase, fault, response and retirement events
establish overlap and explain the interval. No millisecond performance ceiling
is asserted. Ordering assertions use a three-second observation watchdog and
four-second client watchdog; the server's admitted deadline remains two seconds.

## Observations

Healthy request latency includes network, queue, context construction and reply:

| Scenario | Median, repeat 0 | Median, repeat 1 | Largest sample across repeats |
| --- | ---: | ---: | ---: |
| Healthy alone | 9.16 ms | 8.88 ms | 9.41 ms |
| Other worker awaits 250 ms | 8.55 ms | 8.60 ms | 10.73 ms |
| Other worker runs a CPU loop | 3.87 ms | 6.08 ms | 8.04 ms |
| Both workers run CPU loops | 61.43 ms | 61.30 ms | 61.84 ms |
| Mixed await/CPU, healthy on other worker | 3.52 ms | 3.56 ms | 3.63 ms |
| Behind sixteen 40 ms awaiting requests | 104.26 ms | 104.38 ms | 105.71 ms |

The awaiting and spare-CPU rows assert that the healthy response occurs before
the slow handler finishes. The saturated row observes both CPU polls before
sending healthy work and asserts its VM starts only after its assigned worker's
CPU fault. Its central queue time is only about 0.03 ms: the delay is in an
already charged worker slot, not the central FIFO. Dispatch-to-VM includes that
wait and construction, roughly 61.2 ms in repeat 0. Each CPU loop consumes one
whole 10,000,000-instruction budget, returns 500 and is never resumed.

The mixed case starts a 40 ms await on each worker, then a CPU loop on one of
them. Its timer becomes due inside that CPU poll and completes after the fault:
**63.27–69.61 ms**, compared with **40.62–41.74 ms** on the other worker. Healthy
work on the other worker completes before the CPU fault. Multiplexing permits
I/O overlap but cannot stop a synchronous VM poll starving its own worker.

The batch asserts all sixteen awaits are admitted before healthy work, the
central queue holds at least eight, and healthy is dispatched last. Healthy's
median central queue time is 96.22 / 95.99 ms. The recorded finite seventeen-
request completion rates are not steady-state throughput and are not compared
with gate 2's assembly-only rates.

Healthy context-build medians vary roughly 3–8 ms across scenarios, contributing
most of the spare-worker latency. The cause of that variation is not isolated;
CPU load is not claimed to improve performance. These are fresh processes with
logging, and the spare-CPU difference between repeats is retained rather than
smoothed into a speedup claim.

## Ownership and limits

Across both repeats, **84 processes build and retire 490 contexts**, including
schemas. Each ends with zero newly owned sockets, active credits and connection
permits; worker runtimes reach zero tasks and threads are joined. Inherited
sockets are compared by descriptor and identity against the startup baseline.

Median tracked allocation above ready ranges from about 2.29 MB for healthy
alone to 8.05 MB for the finite batch. It includes conversions, contexts and
transport allocations; it is not an RSS or per-handler memory ceiling. Each
measurement retains the raw peak and the independent descriptor/RSS sample.
No SQL connection or pool exists in this prototype, so these clean exits are
not evidence for rollback, backend lifetime or the full gate-6 shutdown matrix.

Root suites and stock startup were not repeated because the checkout's code
and graph remain untouched. Rust formatting, Python syntax, provenance hashes
and the complete wire regression were checked. A development smoke run used
unsupported Rune match-alternative syntax; separate match arms fixed the fixture
before either final repeat. Windows remains unexecuted. The next evidence is
transaction/pool ownership, then shutdown under all stated work states.
