# 0056 gates 5 and 6: an ordinary example and an unchanged stock boundary

Status: accepted on Linux after review fix F1, 2026-09-16. The final three
configurations pass at fix commit `bd3dc04`. Gate 4 was
accepted and pushed at rnx `3f8f6af` and rnx-bench `dede5d3` before these runs.
No Rust source, manifest, lockfile, notices, kernel or adapter changed in this
step. The changes are reproduction fixtures, raw evidence and status/docs.
All record 0056 gates are now accepted on Linux. Non-Linux server execution
is not claimed.

## Gate 5: the shipped application, separate HTTP client

rnx-bench `probes/server-acceptance/journey.py` launches the ordinary release
server with `servers/http-postgres/examples/app.rn`, not the test program.
It creates a private PostgreSQL 18.6 cluster over a Unix socket, creates the
example's `audit(tag text NOT NULL)` table, and supplies its URL through
RNX_POOL_URL. It never uses the system database. The server command is:

```
rnx/servers/http-postgres/target/plain/release/rnx-http-postgres \
  --program rnx/servers/http-postgres/examples/app.rn \
  --events rnx-bench/results/server-acceptance-0056/journey/events.jsonl
```

The actual paths, binary/program hashes and private environment are recorded
in `journey/conditions.json`. `journey/results.json` records the separate
client command and both process IDs. The ordinary server SHA-256 is
`259e8b0b0f4de7a5570bb5d467c8c22875f5d5d44865b1d3c896e0beb4874076`, the
accepted gate 4 binary, 16,277,720 bytes. The package's pinned graph and licence
texts remain unchanged; gate 4's deterministic check covers 178 packages and
129 distinct texts, with the inherited syntree missing text still explicit.

A persistent client process uses Python's HTTPConnection and concurrent reads.
The parent observes VM-start events before sending the healthy request; it
asserts, from correlated server events, that the healthy request arrives and
finishes while each slow request remains active on the other worker.
All handler code is the shipped Rune example and the public API application.
There is no private-root test module, framework API or test injection.

| Request | Status | Client wall time ms |
| --- | ---: | ---: |
| Healthy beside await | 200 | 7.55 |
| Awaiting handler | 200 | 258.78 |
| Healthy beside CPU loop | 200 | 4.62 |
| CPU loop, whole budget exhausted | 500 | 96.23 |
| Failed transaction | 500 | 5.44 |
| Next borrower 1 | 200 | 4.81 |
| Next borrower 2 | 200 | 4.78 |

The failed transaction leaves zero rows, observed through a second connection.
The next two borrowers exercise both workers and commit `next1` and `next2`.
The failed request's worker reuses backend PID 2258723 only after its ROLLBACK
acknowledgement. The public API close precedes this completion as gate 4 proved.

The four non-database handler preparation spans were 4.33, 6.32, 9.97 and
3.76 ms; these include request-value construction and preparation, not just a
context constructor in isolation. The database spans also include checkout
and BEGIN, so they are not presented as pure context-construction cost. The
cost remains per handler; no cache or admission change is introduced here.

SIGTERM ends the server in 9.11 ms, with zero owned sockets, active credits,
connection permits, drivers, leases and runtime tasks. One schema plus seven
handler builds equals eight retirements. A second connection observes zero
pooled backends; the client exits and is joined, and the private postmaster is
reaped. Raw events, observations and commands are retained. The earlier smoke
journey was rerun only to record actual client/server PIDs; this table is the
final coherent run, not a mixture of samples or a latency guarantee.

## Gate 6: root checks and isolation

Final sequential runs at `bd3dc04`, under TERM=xterm, locked and offline.
The review found that 3dea1cb added a relative README link to a server README
not shipped in the crate, after the original suites had run. Those earlier
counts did not validate that commit. F1 replaces the link with the repository
URL, following 0052's adapter precedent. All README edits precede the final
reruns below; HEAD remained `bd3dc04` through their completion. Each
configuration explicitly passes `the_packaged_manifest_makes_the_same_claims`.
The checks logs are replaced with these reruns and `summary.json` records the
full tested commit. Subsequent closure edits change only excluded plan files.

| Root configuration | Passed | Failed |
| --- | ---: | ---: |
| Default | 375 | 0 |
| test-support | 418 | 0 |
| test-support plus server-runtime | 424 | 0 |

The last total includes the five compile-fail public-boundary doc tests.
Formatting passes. The root notices check is current at 124 packages, 87 texts,
13 verified fetched texts and one previously recorded unresolved text. The
newly built default stock executable passes selfcheck. Complete logs are in
rnx-bench `results/server-acceptance-0056/checks/`.

The stock baseline is exactly `032579a`. `build.py` archives it and builds
baseline and current `3f8f6af` in separate targets using the same compiler and
locked/offline release commands, without RUSTFLAGS overrides. The default
normal/build dependency graph, including enabled features and after replacing
only the checkout path, is byte-identical. The lockfile SHA-256 is identical.
Cargo metadata reports only rnx in the root workspace; the server remains a
separate build. No server dependency or runtime path enters the default graph.

| Stock binary | Bytes | SHA-256 |
| --- | ---: | --- |
| 032579a | 15,182,344 | `faa0f08bce74dcd0f4ca7670cbf70785b07c11a14abe93f65699c3518346ff49` |
| 3f8f6af | 15,182,472 | `4a4be718d371131f6d86023195893f998ee9441487266e831739adea19ec03d1` |

The difference is 128 bytes. All 25 CLI comparison cases match exit status,
stdout and stderr byte-for-byte: file/eval successes and refusals, flags,
arguments, compile/runtime/method diagnostics, structures, returned error,
unit, stream output, budget exhaustion, debug source, JSON, version/help and
a session with renumber/reset. Comparison output stores bytes as hex rather
than relying on text decoding. This is the recorded case set, not a proof of
all possible script behavior.

## Matched startup and execution observations

Two repeats, one CPU (4), eight warmups per binary/workload, then ABBA twice
with twenty samples per block: eighty observations per binary per workload
in each repeat. The clock includes process spawn, communication and wait;
there is no per-sample shell. Builds, tests and other fixtures were finished
before timings. Every sample and block median is retained in `timing-1.json`
and `timing-2.json`, with compiler/build provenance in `build.json`.

| Workload | Before / after ms, repeat 1 | Change | Before / after ms, repeat 2 | Change |
| --- | --- | ---: | --- | ---: |
| version | 1.83172 / 1.88106 | +2.69% | 1.91866 / 1.96778 | +2.56% |
| help | 1.91074 / 1.95873 | +2.51% | 1.95053 / 1.91086 | −2.03% |
| bare run | 4.89211 / 4.90325 | +0.23% | 4.81275 / 4.82193 | +0.19% |
| bare eval | 5.28315 / 5.30907 | +0.49% | 5.30480 / 5.29274 | −0.23% |
| JSON 10k | 12.78764 / 12.88073 | +0.73% | 12.82038 / 12.91160 | +0.71% |
| CPU loop | 9.00478 / 9.00350 | −0.01% | 9.01012 / 9.00459 | −0.06% |
| String loop | 5.99703 / 6.00231 | +0.09% | 5.99120 / 5.99926 | +0.13% |

`version` is about 0.049 ms slower in both Python-harness repeats and JSON
about 0.09 ms slower. These remain recorded harness observations, not an
established product regression. This clock includes Python spawn/communicate/
wait overhead: its version measurement is about 1.9 ms versus about 0.54 ms
with the reviewer's hyperfine clock. The small deltas sit within that broader
measurement overhead; their cause has not been isolated. The independent
counter-measurement did not reproduce a consistent process-level regression.

The gates 5/6 review in the local `reviews/0056_review_claude.md` records a fresh
032579a build, hyperfine `-N`, core 4, 100 runs and ABAB order:

| Workload | Before, two blocks | After, two blocks |
| --- | --- | --- |
| version | 562 / 545 µs | 539 / 541 µs |
| JSON 10k | 11.6 / 11.6 ms | 11.9 / 11.6 ms |

The reviewer reports σ 1.3 ms in the first after-JSON block with an outlier;
the second block matches before. This table transcribes that independent
review, not a Codex rerun or a replacement of the original raw samples. The
review note remains local and untracked, so its reported method and numbers
are preserved here for readers of the committed evidence.

Bare run and strings also have small positive deltas in both Python repeats,
recorded above; help and eval change direction. No speedup is claimed and no
threshold discards unfavorable samples. This comparison is to 032579a as
requested, not to the older 0053 JSON baseline, and does not erase that earlier
observation.

## Scope of completion

Linux is the only executed server platform. No new Windows or macOS check is
claimed in this gate. Root execution code has not changed since accepted gate
4; the public API's no-signal/no-exit policy and the standalone hard-exit
separation remain as previously tested. The nested-async budget limitation,
CPU/native poll behavior and deferred context-cache/framework work remain.

Record 0054's prototype gates are accepted, and its public-entry extraction is
now implemented through 0056. The review accepted gate 5 and conditionally accepted gate 6. F1 and its
required final reruns are complete, so record 0056 is closed on Linux with
the stated open items.
