# rnx 0166: bounded web helpers — evidence

**Status:** revised implementation; performance stop bound triggered, awaiting review. Plan: 2a51b79.
Current implementation code measured at 89c9ce5; the review commit adds evidence.
The original submission is retained under round0 and the revision under round1. No application or
production files were changed. Retained probes/results live in rnx-bench at
`probes/web-0166` and `results/web-0166`.

## 1. Behavior and boundary

The six pure `web` functions are installed in stock core contexts and the
caller-owned server context, with catalogue descriptions. They work without
http-server. `web` is reserved against extension/source alias collisions.
No dependency, adapter, callback, native resource or lifecycle state was added.
Escape HTML returns a String directly through VmResult: cap failure aborts
the VM, which the host maps to its existing redacted empty 500. This is for
inline template use without Result plumbing. Parsing and response constructors
keep Result because malformed data and status/body/header errors may be handled.
String/Bytes body and header values retain their input type in independent output.

Borrow guards keep input strings, byte buffers, objects and vectors alive;
preflight scans finish before owned payload copies. Escape expansion is exact;
decoded UTF-8 uses a four-byte stack scalar rather than copying a component.
Forms count raw pairs and validate all decoded components (including discarded
duplicates) before building the object. Responses preflight status/body,
content-type, names and every value before building independent containers.
Header metadata is bounded separately: at most 64 normalized HeaderName records,
each name at most the remaining 16 KiB header allowance, even for empty vectors.
Form table metadata is bounded by 1,024 pairs. These are not payload-copy claims
of zero allocation. Response caps, status and grammar come from `web.rs` for
both constructors and the HTTP host; the host still checks hand-built outputs.
A 256-byte exhaustive fixture matches the shared value grammar to HeaderValue.

The host accepts one outer Result, including hooks. Bare and Ok-wrapped
responses agree; nested Results and Ok(non-response) refuse. Returned Err
values are rendered under the existing bounded error preview into the private
failure object, then replaced with a fixed redacted public diagnostic. The wire
body is empty 500. Result route tables still refuse before readiness.

## 2. Controls

Eight web unit tests cover exact escape/decode behavior, malformed percent
compatibility, Unicode/invalid UTF-8, form duplicate and empty-pair semantics,
String/Bytes parity, independent outputs, input reuse, wrong types and response
refusals. Boundary controls include 1 MiB escape input/output, expansion,
1,024/1,025 raw pairs, header bytes exactly at/above 16 KiB and 63/64 extra
names or values with generated content-type included. The host unit control
covers the Result layer and private/public diagnostic distinction.

`tests/web_alloc.rs` runs a real Rune VM after inputs exist. Additional peak
allocation for the five named refusals is 956, 698, 675, 842 and 930 bytes:
escaping above the expanded cap; 131,072 raw pairs; a 900,000-byte valid form
value followed by invalid UTF-8; a 1 MiB body with a later invalid header;
and a 1 MiB body with a 131,072-value header vector. All are below 256 KiB.
Escape is checked as a VM error; its close result retains that failure while
returning the slot for disposal. Other refusals remain Rune Result errors.

`preflight_controls.py` independently inserts a copy before each of four
preflights (escape, form component, body, header vector), runs that same test,
and requires the allocation assertion to fail. All four fail; a finally block
restores the source and all five controls pass again. Complete failure and
restored logs are retained. No mutation was in the measured binary.

Actual command controls pass: bare/Ok wire parity, implicit HEAD, Err secrecy,
invalid/nested Ok, every reserved hook, hook Errs and routes() Result refusal.
Inherited 0164 host and 0165 route command controls pass, including limits,
deadlines, disconnects, admission, shutdown, keep-alive, startup refusals and
route-table execution exactly once. Both ported sources pass all 26 unchanged
wire fixtures and three complete responses on one connection before load.

## 3. Terseness

The Rune AST/lexer census is replayable using its committed Cargo.lock. Counts
are declaration-span counts; total physical lines include comments and blanks.
Strings, whitespace and literal content count as characters, not readability.
The content category includes page-rendering expressions whose helper calls
change; routes and constants are identical. No formatting credit is claimed.

| Category | Physical lines before/after | Tokens before/after | Characters before/after | ? before/after |
|---|---:|---:|---:|---:|
| Helpers | 53 / 9 | 559 / 75 | 1,626 / 225 | 2 / 0 |
| Content | 25 / 25 | 235 / 239 | 1,682 / 1,702 | 0 / 0 |
| Handlers | 20 / 19 | 282 / 312 | 1,103 / 1,292 | 0 / 0 |
| Routes | 5 / 5 | 48 / 48 | 202 / 202 | 0 / 0 |
| Constants | 2 / 2 | 8 / 8 | 707 / 707 | 0 / 0 |
| Whole source | 127 / 74 | 1,134 / 684 | 5,985 / 4,602 | 2 / 0 |

The remaining helpers are `join` and `field`; this record does not bind them.
Handler tokens/characters still grow due to inline error handling, qualified
helper names and header object syntax, but all template ? tokens are gone; helper removal reduces the whole file. This is not Flask equivalence.

## 4. Verification and disclosures

Core test-support: 510 passed, 3 ignored (including doctests). Feature-off
`--no-default-features --features count-allocations`: 418 passed. Project:
105 passed, 2 ignored. Formatting and diff checks pass. Clippy for --lib --bins --test web_alloc completes with 12 inherited library
warnings, none on changed expressions/new code. The all-target run fails on
two unchanged from_mode(0) permission literals in tests/config.rs and tests/fs.rs
(non_octal_unix_permissions). The first submission wrongly said that run
completed: its own retained log already contained both errors. Corrected here.
The all-target run with only that inherited lint allowed also completes with
14 inherited lib-test warnings (12 duplicates). No unrelated code was changed.
Full failing and successful logs are retained in round1/checks/.

An initial core run failed three exact inventory assertions because they still
expected the old function set; their expected lists/counts now include web.
Another run raced my feature-off build, which replaced target/debug/rnx while
a test-support terminal test used it: it correctly reported the missing
rnx_test item. The final core run was serialized and passed. This is not a
product failure or a claim that the tests are immune to concurrent feature
builds in one target directory.

An initial load run used the implementation before the final reserved-name/help
and invalid-status-check-order changes. It passed and is retained separately as
preliminary-performance; the final run uses the exact committed 865a994 binary.

## 5. Accepted initial launch and size gate

The initially submitted release binary at 865a994 is 20,101,600 bytes versus b4cc0a6's 20,037,000:
+64,600 bytes (+0.32%). `provenance.json` records both SHA-256s, compiler,
platform and oha version. Startup uses the inherited 0068 method: warm,
interleaved on one pinned CPU, 100 launches per mode/binary per repeat and
200 persistent cells per repeat. All output checks pass; 2,400 samples retained.

| Mode | Median delta repeat 1 / 2 | Percent repeat 1 / 2 |
|---|---:|---:|
| version | +0.002 / -0.008 ms | +0.12 / -0.43% |
| eval | +0.138 / +0.092 ms | +2.59 / +1.71% |
| JSON script | +0.037 / +0.065 ms | +0.29 / +0.50% |
| first prompt | +0.079 / +0.037 ms | +1.93 / +0.89% |
| persistent cell | +0.002 / +0.004 ms | +0.58 / +1.00% |

No mode reproducibly exceeds the 5% stop bound. These are local measurements,
not deployment-host or universal performance claims.

## 6. Initial submitted HTTP performance (round0)

Initial submitted run: old versus new helpers on the same 865a994 binary, two workers,
server CPUs 2,4 and load CPUs 8,10,12,14. Three interleaved repetitions per
condition, each with a 3-second warm-up and 10-second measured swarm.
`conditions.json` binds source and executable hashes. The install receipt binds
the separate startup baseline to b4cc0a6. All 30 measured rows and 60 full
warm/measured artifacts retain argv, exit, raw output and parsed oha fields.

| Route / concurrency / keep-alive | Case | RPS median [range] | p50 ms median [range] | p99 ms median [range] |
|---|---|---:|---:|---:|
| `/` / 1 / yes | old | 11666 [11665–11689] | 0.085 [0.085–0.085] | 0.090 [0.090–0.090] |
| `/` / 1 / yes | new | 13307 [13304–13326] | 0.074 [0.074–0.074] | 0.079 [0.078–0.079] |
| `/` / 32 / yes | old | 15930 [15923–15940] | 1.751 [1.751–1.754] | 2.644 [2.641–2.654] |
| `/` / 32 / yes | new | 21836 [21822–21862] | 1.221 [1.220–1.221] | 1.988 [1.988–1.993] |
| `/hello/world` / 1 / yes | old | 31899 [31385–32216] | 0.030 [0.030–0.030] | 0.049 [0.040–0.054] |
| `/hello/world` / 1 / yes | new | 32834 [32686–33044] | 0.029 [0.029–0.030] | 0.035 [0.034–0.036] |
| `/hello/world` / 32 / yes | old | 76492 [76305–76719] | 0.416 [0.415–0.417] | 0.603 [0.585–0.606] |
| `/hello/world` / 32 / yes | new | 79980 [79876–80229] | 0.398 [0.397–0.398] | 0.591 [0.588–0.599] |
| `/hello/world` / 1 / no | old | 17066 [16975–17934] | 0.051 [0.051–0.051] | 0.210 [0.163–0.217] |
| `/hello/world` / 1 / no | new | 16533 [16182–17045] | 0.051 [0.051–0.051] | 0.242 [0.230–0.291] |

New/old median throughput ratios are 1.141, 1.371, 1.029, 1.046 and 0.969:
all clear the 0.95 frozen stop bound. No-keep-alive throughput is 3.1% lower;
it is not presented as an improvement. Every swarm passes the inherited
fail-closed status, error, latency and applicable occupancy checks. The retained
summary is recomputed exactly from all measured rows by validate_saved.py;
15 saved-artifact mutations (including forged/missing summary) refuse while the
unmodified run passes. The initial warm process RSS sample is 23,056 KiB old
versus 22,480 KiB new; this is one local sample, not a memory regression gate.

## 7. Publication boundary

The implementation and benchmark record are local pending Claude's review.
There is no after-push :dep obligation: web is built into stock rnx, and no
adapter changed. Application adoption and deployment remain a separate team's
reviewed work. Source formatting follows in record 0167.

## 8. Review round 1

D1: escape_html uses VmResult, so it produces a String directly and aborts the
VM on its preflighted cap. Typed non-string arguments fail normal Rune
conversion. The plan records this exception for inline templates; html/text
keep response validation's Result contract. The ported example has zero ?
tokens throughout, versus seven in the first submission. Its all-five escaping,
String-argument preservation and non-string VM error controls pass. An actual
HTTP request with over-cap escaping and private body text returns empty 500,
its log contains no body text, and the next request succeeds.

N1: copying preserves String versus Bytes for bodies and header values. VM
controls mutate returned String body, String header and Bytes header, proving
the original inputs unchanged; prompt results no longer turn strings into byte
lists. N2: errors wrap the shared grammar failure with web::response, a name
escaped under a 64-byte display budget and its failing value index. Controls
prove later value 1 is named, a private CR/LF value is absent, and an oversized
control-character name is escaped/truncated into a diagnostic below 200 bytes.

Both sources again pass 26/26 unchanged wire fixtures and keep-alive, and the
new Result/escape controls plus inherited 0164/0165 command controls pass.
All four copy-before-preflight allocation mutations still fail their gate,
and restored source passes. A first attempt adapting the allocation test
incorrectly unwrapped close() after the expected VM error; the test now checks
and disposes the returned (failure, slot), rather than treating it as success.
No production workaround was required.

The full throughput gate is repeated because D1/N1 alter the hot path. The
initial startup gate remains accepted as Claude specified; it is not reported
as a measurement of the revised binary.

### Revised performance: STOP

One full revised attempt, exact 89c9ce5 binary (8ec20aca…), three interleaved
repetitions per condition. Binary size 20,100,888 bytes (+63,888 / +0.32%
versus b4cc0a6). The run exits 1 at the frozen throughput gate. No replay,
tolerance change or unrelated tuning was made after seeing this result.

| Route / concurrency / keep-alive | Case | RPS median [range] | p50 ms median [range] | p99 ms median [range] |
|---|---|---:|---:|---:|
| `/` / 1 / yes | old | 11632 [11613–11663] | 0.085 [0.085–0.085] | 0.091 [0.090–0.092] |
| `/` / 1 / yes | new | 13950 [13894–13955] | 0.071 [0.071–0.071] | 0.076 [0.075–0.076] |
| `/` / 32 / yes | old | 16110 [16110–16110] | 1.731 [1.731–1.732] | 2.613 [2.612–2.614] |
| `/` / 32 / yes | new | 23065 [23056–23066] | 1.160 [1.160–1.161] | 1.911 [1.908–1.913] |
| `/hello/world` / 1 / yes | old | 32008 [31790–32337] | 0.030 [0.030–0.030] | 0.038 [0.036–0.045] |
| `/hello/world` / 1 / yes | new | 33022 [32844–33378] | 0.029 [0.029–0.029] | 0.035 [0.034–0.040] |
| `/hello/world` / 32 / yes | old | 76758 [76509–76771] | 0.415 [0.415–0.415] | 0.594 [0.592–0.608] |
| `/hello/world` / 32 / yes | new | 82261 [82189–82423] | 0.387 [0.386–0.387] | 0.545 [0.542–0.550] |
| `/hello/world` / 1 / no | old | 18144 [18123–19155] | 0.051 [0.050–0.051] | 0.137 [0.070–0.141] |
| `/hello/world` / 1 / no | new | 16901 [16178–17168] | 0.050 [0.050–0.051] | 0.242 [0.235–0.250] |

Ratios: 1.199, 1.432, 1.032, 1.072 and **0.931**. The last condition
(hello, c=1, no keep-alive) is 6.85% lower, beyond the 5% stop bound.
The cause is not established. Gains in other conditions do not waive that gate.

Both sources pass 26 wire fixtures and three-response keep-alive before load.
All 30 measured rows and 60 complete oha artifacts exist; inherited field,
exit, status/error, latency and applicable occupancy checks pass. Saved validation
recomputes the summary and gate, then fails at the same throughput criterion.
Its complete failure output is retained. The initial 15 corruption controls
remain round0 evidence; they were not rerun against this failing-gate baseline.

Changes D1/N1/N2 and functional controls are ready for review, but this record
cannot close or publish under the accepted performance rule. Resolve this
checkpoint with Claude before any further profiling, replay or code changes.

## 9. Pre-registered slot diagnostic: STOP remains

Claude's round-2 review (chatd 01a1086d-76ca-724b-a6a7-bee3a8e81068)
registered SWAP and A/A before either was run. Both use the same exact
8ec20aca… binary, server CPUs 2,4, load CPUs 8,10,12,14, hello c=1
without keep-alive, three repetitions ordered AB / BA / AB, and the same
3-second warm-up / 10-second measurement. SWAP places the new source in
slot A and the old source in slot B; A/A places the old source in both.
No product changes were made. Round0 and round1 remain retained separately.

| Diagnostic / slot / source | RPS median [range] |
|---|---:|
| SWAP / A / new | 16204 [16045–16569] |
| SWAP / B / old | 18392 [16703–19181] |
| A/A / A / old | 18431 [16782–18952] |
| A/A / B / old | 16821 [16403–17428] |

The SWAP old-B/new-A ratio is 1.1350: the deficit follows the new source
in that run. The A/A B/A ratio is 0.9127: identical sources also have an
8.73% slot gap. Therefore neither registered attribution applies: (a)
requires SWAP old-B/new-A <= .95 with an A/A gap of the same sign; (b)
requires SWAP new-A/old-B <= .95 with an A/A slot gap no greater than 2%.
The deterministic decision is **(c), STOP and report**. These measurements
do not establish whether the round1 deficit is a source regression or a
harness effect. No corrected ABBA gate, profile, tuning, or further replay
was run after this decision.

Each diagnostic retains six measured rows and twelve complete warm/load
artifacts. Saved validation checks the pinned binary metadata, source
hashes and slot mapping, ready/load commands, exits, exact artifact set,
parsed stdout and fields, error/status/latency/applicable occupancy checks,
and recomputed summaries. Both pass. The final strengthened validator was
replayed over the saved artifacts after measurement; it did not rerun load.
The driver, validation output, formulas and decision.json are retained in
rnx-bench results/web-0166/round2. Record 0166 remains unpushed pending
review of this checkpoint.

## 10. Registered user-space work gate: decision A

Claude registered round3 in chatd 01a10874-81b1-71c8-893b-567487c0fba7
and confirmed its schedule and operational definitions in
01a10875-80a2-761c-b8b6-c27c6a9af191 before measurement. The same
8ec20aca… binary and source bytes are used, without product changes. For
each hello c=1 condition (no keep-alive and keep-alive), three repetitions
of four ABBA blocks alternate old-source slots A,B,B,A and source orders
old/new, new/old, new/old, old/new. This gives twelve windows per source
per condition. Server CPUs remain 2,4; load CPUs remain 8,10,12,14.

After the separate 3-second warm-up, perf stat attaches to the existing
server process and its threads with counters initially disabled. FIFO
enable/disable acknowledgements bracket the measured 10-second oha process.
The retained window includes the process launch/exit envelope around oha;
it is not claimed to equal oha's internal active interval exactly. The
successful-request divisor comes from the validated all-200 status
distribution. User-space counters exclude the kernel network path.

Operational detail disclosed before the run: this hybrid machine's server
CPUs 2,4 belong to cpu_core (0–15). --cputype core selects the active PMU;
the inactive cpu_atom otherwise reports not-counted. Raw counter rows,
running percentages, attachment task IDs, command/exits and window
timestamps are retained. FIFO acknowledgements carry a trailing NUL here;
SIGINT after disabling counters emits the final counts (exit -2), whereas
SIGTERM does not. Those details were verified on a disposable process
before any server measurements and sent to Claude. Each accepted counter
has positive finite counts/runtime and at least 99% running.

| Hello c=1 | Old instructions/request | New instructions/request | New/old instructions | New/old cycles |
|---|---:|---:|---:|---:|
| No keep-alive | 142850 | 130563 | 0.9140 | 0.9595 |
| Keep-alive | 123659 | 111125 | 0.8986 | 0.9503 |

Instruction spread (max−min)/median is 0.154% / 0.050% for old/new
without keep-alive and 0.270% / 0.104% with keep-alive, below the 5%
inconsistency bound. Cycles and instructions agree in direction in both
conditions. The registered rule first checks consistency, then accepts A
when both instruction ratios are <=1.03: **decision A**. The new source
performs less measured user-space work per request in both conditions.

Round1's no-keep-alive RPS STOP and round2's case (c) remain reported
unchanged. The revised deciding metric accepts the performance checkpoint;
the earlier RPS deficit remains an unresolved environmental effect rather
than an attributed 0166 code regression. Identical-source A/A showed an
8.7% slot gap, demonstrating roughly a 9% observed noise floor for this
condition on this machine. That is a lesson for future gate design, not
a retroactive edit to 0162–0165 or a claim that all RPS variation is noise.
The new metric does not diagnose the environmental cause or measure
kernel work.

Saved validation passes all 48 measured rows, 96 complete warm/load oha
artifacts and 48 raw perf artifacts, with source/slot/command/exit/window
bindings and exact counter/request arithmetic. Eight parser corruption
controls (missing, duplicate, not-counted, non-finite, zero, low-running,
zero-runtime and wrong-PMU) are refused; the original passes. Final saved
validation is replayed without rerunning measurements. All earlier rounds
are preserved. No profiling, tuning, product change or extra measurement
was made after decision A. The completed record is sent for final review
before publication.
