# rnx 0163: reusable invocation slots — evidence

**Status:** closed after review. Plan: `6f4d9e6`. The implementation is additive: the existing `Program::prepare` and `Invocation` API stays available, including for per-request extension registrations. The new `Program::slot`, `Slot::prepare` and `SlotInvocation` reuse a context/runtime and HTTP pool, with one fresh VM and budget per run.

## 1. Ownership and lifecycle

The invocation consumes its slot. The stage is shared between run, its drop guard and close: ready, begun, done. The generation begins on the first poll. An unpolled run future does not execute its body and leaves the invocation runnable; a polled future dropped before completion finishes as cancelled. A second run is refused. The VM/execution is disposed before the finish guard; close drops unused arguments and clears every lifecycle-tracked operation before returning the slot. Abandoning the invocation retires it.

Cleanup failure takes precedence and returns no slot. An ordinary execution failure returns a healthy slot. A failed preparation drops its arguments before checking health. Health means idle, unretired, able to increment its generation, and without a recorded cleanup failure. Close checks health too; an exhausted or retired lifecycle cannot be returned. Compile-fail doc tests prove both new public types are non-Send.

Clarifications to the plan's sketch: VM construction happens inside run, rather than prepare, and finish happens before close (or on dropping a polled run future). This keeps first-poll semantics and shares one execution helper with the old API. An unpolled run does not finish a generation that never began. Returned values are caller-owned: the host converts them to owned response bytes and drops them before close. Close cannot destroy an arbitrary value retained by a caller. It revokes lifecycle-tracked futures in such a value; other retained values and untracked futures remain the caller's responsibility.

## 2. Gates

Nine `server::slot_tests` cover:

- old/new successful result and exact failure category, message and position, including panic, exhausted budget and missing handler;
- repeat mutation of const-derived vectors, local objects and closure captures on fresh VMs, budget reset, and an explicitly persistent native counter;
- success/failure × unpolled/pending escaped tracked futures, polled during the next generation and refused as cancelled;
- unpolled, cancelled, completed and abandoned invocation lifecycle;
- cleanup failure during run or close, and during unused-argument destruction after a refused preparation;
- retired lifecycle preparation and begin failure, both returning no slot;
- actual held HTTP responses, success/failure × unpolled/pending, revoked before the next generation;
- four interleaved tracked operations, cancellation and retirement isolated to their slots;
- four slots on one LocalSet, each awaiting a timer and held HTTP response: retire one, cancel/clear/reuse another, and the other two complete unchanged.

The additional new test in `lifecycle` checks idle/active, retired and u64-exhausted states. The clear-removal mutation replaces close's clear with finish(false): the escaped-future control fails with `revoked future succeeded`; restoration passes. This demonstrates why generation finish alone is insufficient.

`cargo test --offline --locked --features server-runtime,test-support`: 480 passed across 49 targets, including existing embedding/server tests and seven compile-fail docs. The final slot tests pass 9/9, the subsequently added lifecycle test passes separately, and the final library run passes 188/188. The default-feature cargo check also passes. `cargo clippy --offline --locked --features server-runtime,test-support --all-targets` exits 0 with inherited warnings in untouched code; none in server.rs or lifecycle.rs. The two large-error lint exceptions explain the accepted owned-slot error API. Formatting and diff checks pass. No adapter binding changed.

## 3. Built-in captured-state audit

The inventory is the install sequence in `src/server.rs::context`. Install code and its called implementations were inspected, including every async function/future-returning registration. This is a reuse audit, not a sandbox claim.

| Installed battery | What can persist and why |
|---|---|
| json | Stateless parser and serializer registrations; request values and per-call traversal state are caller/VM-owned. |
| io | Synchronous stdin/eprint. Streams are process resources. `host::STDIN_READ` is a process-wide one-read guard, already shared by the old API; it is deliberately not reset per slot/request. Printing affects shared stdout/stderr. |
| interchange | Type and method registrations, no captured request state. Dense buffers are immutable Arc data owned by supplied/returned values. |
| process | Synchronous supervised child calls; each call waits for supervision to finish. The installed exit-refusal function is stateless and bypasses CLI exit state. Platform interruption is an existing process-wide external flag; this API installs no signal handler or per-request reset. |
| fs | Synchronous calls with per-call buffers; disk effects and open host resources are external, not isolated or rolled back. |
| path | Stateless synchronous path helpers. |
| time | Stateless helpers plus the existing process-wide monotonic `ORIGIN: OnceLock<Instant>`. `sleep` is an untracked async timer, with no side effect beyond waiting/waking and no spawned task. VM-owned sleep futures drop with the execution. A caller retaining one owns it; close does not promise to revoke it. |
| text | Stateless synchronous conversions and helpers. |
| env | Captured immutable empty arguments; args returns independent copies. Reads of the process environment/home are external, not a request snapshot. |
| http | All four request registrations wrap their future in `scope.track`. State captures the client pool, deliberately persistent. Clear revokes tracked requests; retirement also releases the client. Transport shutdown still needs runtime progress; prior external network effects cannot be undone. |
| Rune 0.14.2 defaults | Registration tables and types are shared, while each run gets a fresh VM/stack and const materialization. Collection and closure values belong to the VM/caller. The std future/stream helpers compose supplied futures; they do not independently launch network/filesystem work. std I/O is synchronous external output. The std ops helper's OnceCell hash RandomState and immutable Unicode lookup tables are process-wide implementation data, not script-mutable module state. |
| rnx_test (test-support only) | Test instrumentation, not part of the production battery contract. Fixture extensions deliberately test both persistent native state and tracked cancellation. |

Among rnx's production batteries, only http and time register async work: http requests are tracked; the untracked timer cannot perform a delayed external write. The other listed batteries register synchronous functions. A retained Rune async closure, supplied native future or caller-retained mutable value is not magically isolated; the host must respect the result-disposal contract. Trusted extensions may capture intentional slot-lifetime state, but per-request state must travel in arguments and side-effecting request-owned futures must use Scope::track. Arbitrary captured native mutable state is outside the guarantee. The API documentation says so.

## 4. Matched prototype measurement

The retained probe is in rnx-bench commit `bd031cc`, `probes/web-0163`; outputs are in `results/web-0163`. B is the old fresh-context host; B-prime (`S` in raw rows) builds four slots per worker. Both use the same compiled Rune handler, two workers, active limit four, queue sixteen, and CPUs 2,4. oha runs on CPUs 8,10,12,14. Three repetitions interleave B/B-prime (second repetition reverses order), three seconds warm-up and ten measured, concurrency 1/32, keep-alive, two routes. Compilation and test runs had finished before the measured run. Other host load was not controlled. Hosts were built from the working implementation; binary, lockfile and source hashes are retained in provenance.json. After the run, only test fixtures and a cfg annotation excluding the health helper from unused non-server builds changed; the measured server-runtime production code is unchanged.

Before load, each host passes all 26 exact response fixtures and three wire requests on one socket. All measured oha responses must be 200, no errors, finite positive throughput/latencies, with established connections equal to concurrency. Full warm-up and measured oha JSON, connection samples and row summaries are retained; validate_saved.py checks completeness and replays every measured gate. Instrumented phase runs are separate and retain 660 complete records per host; the original phase validator is reused. Lifecycle host controls show panic, budget exhaustion and invalid response followed by successful reuse.

| Route | Concurrency | B req/s | B-prime req/s | B p50/p99 ms | B-prime p50/p99 ms |
|---|---:|---:|---:|---:|---:|
| `/` | 1 | 308 | 12,799 | 3.248/3.436 | 0.077/0.082 |
| `/` | 32 | 658 | 30,306 | 48.430/53.018 | 0.992/1.451 |
| `/hello/world` | 1 | 325 | 23,951 | 3.047/3.318 | 0.031/0.087 |
| `/hello/world` | 32 | 672 | 88,385 | 47.445/51.687 | 0.360/0.622 |

These are medians of three interleaved samples. The full ranges are retained in matched-run.txt.

| Route | B prepare/run/convert/close µs | B-prime prepare/run/convert/close µs |
|---|---:|---:|
| `/` | 2758.6/80.8/2.1/170.9 | 3.4/158.2/1.7/0.2 |
| `/hello/world` | 2724.2/24.0/1.7/167.0 | 1.9/11.1/1.1/0.2 |
| `/static/site.css` | 2739.4/12.5/1.9/166.8 | 1.2/2.7/0.6/0.1 |

The phases include the host's request-value construction in prepare. Phase instrumentation and different cache/CPU conditions affect run times; they are not isolated VM microbenchmarks.

CPU-2 construction-plus-disposal medians (300 after 20 warm-ups): defaults 2513.0 µs, runtime from a prebuilt context 120.5 µs, old prepare/close 2787.5 µs, slot build/drop 2797.8 µs. Context construction still costs about 2.8 ms; slots amortize it.

The two workers built their four slots in 11,645 and 11,927 µs. Single readiness observations: B 23.2 ms / 15,440 KiB RSS, S 18.4 ms / 20,984 KiB RSS.
B sampled process-tree RSS under the four first-repetition loads: 15,248–17,708 KiB. The eight persistent slots trade several MiB of process RSS for the avoided per-request registrations; this is a process observation, not isolated per-slot accounting.
S sampled process-tree RSS under the four first-repetition loads: 21,004–22,044 KiB.

These measurements remove the dominant measured per-request registration cost; they do not reduce Rune's registration cost itself. There is no current matched axum-only or Flask measurement in this record. Their 0162 rows remain historical context only. The probe is not a product host: retired-slot replacement, generic routes and production shutdown belong to the next host record.

## 5. Review

Claude accepted with no blocking findings and reran the final library suite (188/188). The healthy-helper comment now states all four reuse conditions. For the next product-host record, a retired slot must be replaced and its retirement logged or counted rather than stopping a worker.
