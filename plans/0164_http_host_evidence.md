# rnx 0164: HTTP host evidence

**Status:** accepted by Claude after independent checks. Product code is generic; application
source and content are outside this repository.

## Contract and implementation

`rnx serve PROGRAM.rn` compiles once and runs the unchanged request/response
contract on an axum HTTP/1 transport. Four thread-local reusable slots per worker,
sixteen queued owned jobs, and twenty admission permits per worker bound the
request path. Admission precedes body allocation; its permit follows the response
body until transmission or abandonment. Rune values never cross threads. Response
conversion validates types, counts, byte bounds, header syntax and transport
reservations; values are dropped before invocation close. HEAD preserves length.

Workers race async execution with the original request deadline and sender
closure. Dropping the run future finishes cancellation, then explicit close
revokes lifecycle-tracked operations. Cleanup failures retire and replace a slot;
failed construction stops the host instead of shrinking it. The native unwind
boundary disposes the future and owner; it never resumes that request/VM. The
existing quiet native catch prevents its ordinary panic hook from writing on the
worker. Shared external extension state remains outside the ownership guarantee.

The HTTP command is dispatched before CLI contexts and their interrupt handler.
SIGINT/SIGTERM close acceptance/admission, refuse queued work, drain connections,
and cancel active work at the grace deadline. Worker joins occur on a separate
joiner so a second signal remains actionable. Uninterruptible native synchronous
polls can delay joins; this is cooperative cancellation, not a sandbox guarantee.

A dedicated logger writes to a duplicated stderr handle. Its producers use a
256-record channel, each record at most 4 KiB, with accounted refusal/write-error
counters. No descriptor flags are changed. Shutdown waits 100 ms then detaches a
blocked writer (which owns no Rune state). A permanently blocked sink can lose the
summary. Fatal host errors are reported through that bounded channel, after owner
cleanup, without a second unbounded Termination write to stderr.

## Reviewed boundary and follow-up

Extensions is deliberately non-Send and FnOnce. Stock serve reconstructs
Extensions::none for every owner; extension-bearing main_with executables are
refused before source opening, naming the extensions, reason and stock command.
The private repeatable factory supports failure controls only. Adapter-enabled
serving needs a separately reviewed thread-safe repeatable factory contract.

This removes per-application Rust hosts. It does not supply route tables, path
parameters or web helpers, and makes no final Flask-level terseness claim. The
first application's remaining manual route/HEAD/405/allow and escaping/URL/form
helpers confirm the next cut: route tables first, then the small web helpers.

## Controls and checks

Nine HTTP unit controls cover strict CLI options, stock-only refusal before
builders/source opening, request bytes/duplicates/unit query, bounded response
schema/headers, HEAD, logger accounting/detach and unchanged shared flags. Actual
worker controls run a tracked destructor failure alongside three async jobs:
one owner retires, a fifth construction restores four slots, the other jobs
complete. A subsequent native unwind causes another independent replacement and
successful reuse. An expired queued native-panic job never executes. A refused
replacement is a visible fatal signal rather than silent pool shrinkage.

The retained actual-command probe exercises source attribution, binding collision,
all transport bounds, malformed/forbidden outputs, HEAD/204/304, repeated complete
responses, declared/streamed oversize, body timeout, instruction exhaustion,
async deadline and reuse, client disconnect, overload, grace cancellation,
successful active drain, IPv6/ephemeral readiness, and service/shutdown with an
undrained pipe while its parent F_GETFL remains unchanged. The 26 frozen wire
fixtures and three-response keep-alive check pass for every measured host.

Root with test-support/server-runtime: **490 passed**. HTTP feature off, with
count-allocations/server-runtime: **433 passed**. Clippy has no warnings in the
new HTTP files; inherited warnings remain. Notices were regenerated for the
locked graph; the generator now avoids trailing spaces in its own headings,
without modifying licence text. The one unresolved licence-text gap is inherited.

## Measurement provenance and disclosures

The retained rnx-bench web-0164 probe holds source/binary/tool/lock hashes, complete
oha JSON, exits, fixture results, RSS/connection samples and sample journals.
The comparison disables request logs for all hosts. Default logging cost is a
separate product comparison against a drained file. The product uses its default
2,000,000 instruction budget; 0162/0163 used 10,000,000. Accepted timing samples
have 200-only responses, no errors, finite positive fields and exact keep-alive
occupancy. They describe this machine and workload, not general equivalence.

Disclosed repairs before final evidence:

- A pressure control found a real race: close's expected cancellation failure
  overwrote 504 with 500. The host preserves timeout/disconnect results only for
  a healthy returned slot; cleanup failures still override with 500.
- Parallel feature builds overwrote target/debug/rnx during a test-support run,
  yielding a missing test-only function. The run was invalidated and the full
  suite rerun serially. A no-default run without allocation accounting also ran
  two ceiling tests whose premise requires it; the counted feature-off suite
  passes, with no production/test changes to those tests.
- The first matched driver accidentally omitted inherited oha -w. Its first
  axum sample reported one deadline-aborted request and was refused. The entire
  attempt is retained, not pooled. Restoring -w restores the existing harness
  contract; no error gate was loosened.
- The notices generator's new dependency headings carried trailing spaces;
  this was caught before final diff approval and repaired in the generator.

## Results

Three interleaved repetitions; request logging off in all throughput comparisons.
Median requests/second:

| Workload | Axum native | rnx serve | Flask/gunicorn |
| --- | ---: | ---: | ---: |
| home, c1, keep-alive | 70,266 | 11,652 | 6,512 |
| home, c32, keep-alive | 309,277 | 17,025 | 13,070 |
| greeting, c1, keep-alive | 73,402 | 30,688 | 6,222 |
| greeting, c32, keep-alive | 334,447 | 71,115 | 12,128 |
| greeting, c1, no keep-alive | 22,930 | 16,728 | 4,732 |

The measured generic host is faster than Flask on these samples, but substantially below
native axum on keep-alive. The home handler's Rune-generated HTML costs much more
than the small greeting. This does not meet an axum-equivalent performance claim.
The retained ranges and p50/p99 latencies accompany the medians. All 45 measured
rows and 90 complete artifacts validate; eleven saved-evidence mutations fail.

Product rest RSS in the matched run was 23,008 KiB. The separately interleaved
readiness probe (ten starts per host, no fixture requests inside the interval)
observed median readiness at 1.67 ms for native axum, 28.05 ms for the product,
and 189.52 ms for gunicorn. The product announces only after all stock slots
exist; gunicorn is measured after two post_worker_init callbacks with both apps
loaded. These are process-spawn-to-observation times, including polling overhead.
The matched run's gunicorn socket-listener observation is retained separately,
not treated as application readiness. The release unit measurement isolates the
actual production Workers::start path, excluding compilation.

Default request logging to a drained file reduced median greeting/c32 throughput
from 70,087 to 56,460 requests/s (19.4%). Even a drained
file can overflow the producer queue: 2,191,988 request records delivered and
14,594 dropped (0.66%). Delivered plus dropped exactly equals
all 2,206,582 successful requests, including the three wire controls.
Summary delivery succeeded here; permanently blocked-sink delivery is not claimed.

The final stock gate passed in both repeats. Median changes: version -0.01/+0.11%,
eval +1.25/+1.41%,
JSON run +0.22/+0.25%,
first prompt +1.23/+0.66%,
persistent cell +0.55/-1.27%. Every median is below the 5% limit; individual journals and p10-p90
ranges are retained. Stock binary grew from 19,016,472 to 19,919,976 bytes;
startup-final/binaries.json records the exact hashes and sizes.

The final measured stock binary was built from clean implementation bb707d1,
including runtime diagnostic privacy hardening and the reviewed backlog correction.
The evidence-only amend does not change that code. Prior complete sets remain
labelled pre-hardening and pre-r1; none is pooled with the final sample set.
The ignored pool-cost test was committed before its release measurement.

Application fetched-install and first-local-commit checks follow acceptance and
publication of the generic host. No application content is committed here.

Pool measurement: 1 worker(s), 4 slots, median 11.706 ms, range 11.308–13.797 ms, ten samples.

Pool measurement: 2 worker(s), 8 slots, median 20.319 ms, range 16.466–23.415 ms, ten samples.

## Diagnostic privacy hardening before review

A final audit found that VM failure messages can contain script-supplied data
(for example a panic built from request content). Runtime logging now records the
failure category rather than those messages; response-conversion reasons are
fixed schema/bound messages. Compile/startup diagnostics remain source-attributed
and bounded, and script printing retains its explicit synchronous-output caveat.
The earlier measurement set is retained as pre-hardening; the final generic-host
matched, logging and startup gates are replayed after this production change.

## Preliminary reviewer R1

Claude's read-only review found that the old accept loop reset connection 257
instead of leaving it in the kernel backlog. The accept arm is now disabled at
the 256-permit cap; completion of an existing connection wakes the joined-task
arm and makes acceptance eligible again. The actual-host control holds 256 idle
keep-alive connections, proves 257 neither answers nor resets while full, closes
one, then receives 200 on 257. The old binary fails with ConnectionResetError.
The initial-closed-receiver internal outcome is also 499 consistently. Root 490
and all actual-host controls pass after the correction; new HTTP files are clean
under clippy. Final measurements are replayed after this change; the previous
complete sets remain separately labelled.

An opt-in full diagnostic mode for local debugging is a follow-up with an explicit
privacy contract; redaction remains the default. Control-heavy log records that
exceed the 4 KiB encoded cap are refused and counted, never output-trimmed.

## Acceptance

Claude independently reran the 490/433 root configurations, actual-host controls,
26 fixtures and keep-alive, saved evidence validation, and exact log accounting.
He accepted with no blocking findings. His README nit now explicitly states that
all events, including failures and retirement, can drop through the shared bounded
channel even when the sink is drained. This final amend changes prose only.
