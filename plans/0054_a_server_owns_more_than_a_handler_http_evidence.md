# 0054 gate 3: the HTTP boundary over the assembled workers

Measured by Codex on Linux, 2026-09-16, against `ae1d620`, the accepted
admission/boundary plan. The stock source, manifest and lockfile remain
byte-identical to accepted 0055 (`d87334f`). Only an archived copy receives the
private test module and server dev dependencies. No server entry point is added.

**Outcome: the gate-3 prototype passes its raw-wire and resource assertions.**
The evidence supports adopting Hyper for this boundary, pending review. This
is not closure of record 0054: gates 4–6 and the supported server-entry contract
remain open. There is no pool or transaction owner in this gate, and the `/fail`
fixture is a handler panic rather than the eventual failed-transaction route.

## Source and reproduction

rnx-bench commit `6ab105e`, `probes/server-http/`, contains the private Rust module, archive/build
harness, raw-TCP driver, pinned lockfile, complete resolved dependency/feature
graph and additional licence notices. Run the build, then wire driver twice as
its README describes. Results are under `results/server-http-0054/run-0/` and
`run-1/`, with a stock-interrupt result beside them.

Each repeat records the root revision, probe/build/driver/lock/graph hashes,
release executable hash, toolchain, exact injected manifest and command,
CPU affinity, kernel version and signal used. It includes raw event timing,
wire assertion results, server log, and an independent 10 ms RSS/FD sample.
The source hashes of Hyper's HTTP/1 builder, decoder and buffered I/O are also
recorded. The input corpus is in the Python source, not normalized by a client
library. No public service or system PostgreSQL cluster is contacted.

Hyper 1.11.1, hyper-util 0.1.20 and http-body-util 0.1.5 retain the root graph's
versions. Server support introduces httpdate 1.0.3; the fixture's Tokio macros
introduce tokio-macros 2.7.2. Their shipped notices are recorded alongside the
full graph; the archive keeps rnx's existing notices. The initial fetch needed
network; the final build was locked and offline.

## What runs

The coordinator owns the listener, all HTTP connection futures, read/write
clocks, a central FIFO and owned request/reply messages. It runs on a different
thread from both executors. Each worker has a current-thread runtime and LocalSet,
a maximum of four charged handlers, and a fresh real rnx serving context per
handler. Only the compiled Unit is shared. A whole 10,000,000-instruction budget
is applied once; a halt is never resumed.

Worker inboxes contain already charged active work, not additional admission.
The completion channel holds at most eight messages. A completed task's handle
is reaped, and the worker JoinSet is independently capped at four. One oneshot
reply belongs to a connection. The central queue never contains more than
sixteen complete requests. Expired/disconnected queued work is removed without
context construction; active credit lasts through logical context teardown.
The schema context is also counted and retired.

Both runs reach the proposed maxima: **32 connections, 8 active credits,
16 queued requests**. The 25th complete request is refused with 503 without a
context build. The 33rd accepted socket is immediately closed, with no task or
handler allocated for it. The OS accept backlog is not included in that claim.

## Wire results

Both repeats pass **56 cases**. They cover:

- HTTP/1.0 refusal; origin-form targeting; exact header count and overflow;
  oversized heads and targets; unsupported methods, paths, encodings, Expect
  and upgrades. Refusals construct no handler.
- Exact 1 MiB body echo, including NUL/non-Unicode bytes; one-byte overflow;
  Content-Length near 2^63; a chunk declaration of 2^62; overflowing chunk
  lengths; 10,000 tiny chunks; oversized chunk data and cumulative extensions.
- Conflicting lengths, truncated bodies and a pipelined second request which
  never executes. Declared framing governs the first body. Ordinary and
  oversized trailers refuse. Hyper's pinned trailer byte cap is 16 KiB and
  its trailer field count uses the configured header limit; extensions also
  have a cumulative 16 KiB cap. No declaration-sized allocation appeared.
- Response shape, status, header value, host-owned framing, body limit and
  bodyless-status validation; raw query preservation and repeated non-Unicode
  header values. Invalid results produce only the generic 500 body.
- Exact **64 response fields on wire** and **16,384 name/value bytes**, with the
  next field/byte refused. Validation reserves three fields and their bytes
  for Connection, Content-Length and Hyper's Date, leaving at most 61 handler
  fields. The reservation is conservative if Date was already supplied or a
  bodyless status suppresses a framing field. Wire syntax is not part of the
  name/value account. Request parser syntax does count toward its separate
  16 KiB buffer, so that limit can refuse before the application byte account.
- Separate head/body/request/write clocks, full connection/queue admission,
  detected disconnects in active and queued work, queued expiry without a
  build, CPU/coordinator separation, late completion and recovery.

Length assertions use actual Content-Length and complete received bodies,
not a search for an HTTP status prefix that could occur in user data. Hyper
serves one request per connection and closes. Late worker completion has only
a closed oneshot receiver; it cannot produce another response.

## Clocks and execution ownership

These are fixture observations with file logging, not the gate-4 latency study.
The clock tolerances are stated in the harness README: 4.7–6.5 seconds for a
nominal five-second read clock and 1.8–3 seconds for the two-second admitted clock.
Fragmented heads keep sending bytes, so a per-byte renewed deadline cannot pass.

| Observation | Repeat 0 | Repeat 1 |
| --- | ---: | ---: |
| Body read deadline, 408 | 5000.9 ms | 5001.2 ms |
| Head / silent head / fragmented head | about 5002 ms | about 5002 ms |
| Admitted awaiting request, 504 | 2002.2 ms | 2001.9 ms |
| Detected disconnects through teardown | 7.24 ms | 7.20 ms |
| Whole-budget CPU handler, 500 | 91.5 ms | 94.5 ms |
| Coordinator refusal during an observed CPU interval, 417 | 0.196 ms | 0.110 ms |
| Blocked response write, dispatch to disposal | 1001.2 ms | 1005.7 ms |

The CPU row returns budget failure, never 504, in these measurements. A
coordinator response is timestamped between a worker's VM start and its CPU
fault, rather than merely sending requests rapidly and assuming overlap.
A worker's failing Rune diagnostic includes the request id and source position;
the response does not expose that diagnostic.

A separate, explicitly native blocking control sleeps for 2.5 seconds inside
one poll. It receives 504 roughly two seconds after admission, while an active
credit remains charged until teardown around 2500.2–2500.3 ms after VM start.
That proves timeout is not execution termination. The same control fills the
workers so a queued healthy request expires without context construction.
No stronger CPU/native preemption claim is made; OS contention can also change
how much wall-clock time a fixed instruction budget takes.

Slow-write evidence uses a disclosed test injection: SO_SNDBUF is set to
16 KiB and the client has a tiny receive window. Without that, a normal kernel
send buffer can accept the entire capped response even though the client does
not read. The gate requires the actual `write deadline` event. Bytes already
handed to the OS can outlive the application's response owner.

## Allocation, descriptors and teardown

Three batches in each repeat hold 32 incomplete bodies, each one byte short of
1 MiB. No handler is constructed for them. Tracked live allocation is
**51,499,312 bytes** in every held batch, including Vec capacity growth and
parser state. After all peers disconnect it returns to **129,840 bytes** in
every batch. Forty enormous declared lengths per batch are then refused; this
is 120 repeated declaration attacks in each run in addition to the main corpus.

Held RSS is about 56,048–56,244 KiB; released RSS is about 23,268–23,464 KiB.
Retained allocator pages are not called live requests or a leak. The peak
tracked allocation is 51,499,324 bytes. These observations do not establish a
process-wide memory ceiling, bound native allocations, or charge OS buffers.
The harness also asserts the released tracked live value stays below 1 MiB.

Repeat 0 ends through SIGINT, repeat 1 through SIGTERM. The existing rnx SIGINT
handler remains the only SIGINT handler; the coordinator polls its flag.
SIGTERM has a separate prototype-only flag. Final events in both repeats assert:

- 44 contexts built and 44 retired, including the schema;
- zero active credits and zero connection permits;
- zero socket descriptors;
- both worker runtimes at zero alive tasks, both worker threads joined.

Python waits for successful exit and joins its monitor; failure containment
kills/reaps its own process group. No fixture remains. This exercises the end
of the HTTP fixture, not shutdown with an active SQL command or pending rollback.

The unchanged stock executable also passes the existing test-support gate
`a_synchronous_session_input_in_a_loop_is_still_interrupted_within_a_slice`
under TERM=xterm-256color. The checked-in log records one passing test. The
initial command without test-support selected zero tests and is not counted
as verification. Full root suites/startup were not rerun because no root code,
manifest, dependency graph or runtime path changed.

## Remaining work and development corrections

Gate 4 still needs the selected HTTP scheduling measurements (awaiting, CPU,
saturated and mixed), gate 5 the real transaction/pool owner with rollback
failure, and gate 6 the shutdown matrix and backend observations. The supported
factory/entry point remains a separate contract. Windows remains unexecuted.

Development compile failures were fixture mistakes in Rune's value conversion
API. One early response-count assertion incorrectly counted `HTTP/1.` inside
the 505 response body; it was replaced with framing-aware length assertions.
An earlier response-accounting draft counted only handler fields; the final
code and gates reserve and test host fields too. Final evidence uses the same
completed source and binary across both repeats, not those development runs.
