# rnx 0181: previously uncollected execution-cost diagnostics

**Status: plan for review.** No new discovery, subject execution or measurement
is authorized until the appropriate review checkpoint below. This is a new
study selected after seeing 0180's measurement STOP, not a replay or completion
of 0180. No engine/profile/product changes, builds, gate relaxation or adoption.

## 1. Why this record exists

0180 stopped when group B's Topdown fraction sum exceeded its frozen tolerance.
Its completed R/A findings reproduce excess cycles and weaken a frequency-only
explanation for numeric/range loops, without identifying a cause. Groups C/D/E
never executed an experiment workload. We now select those previously
uncollected branch/frontend/load diagnostics explicitly after that outcome.
Original B stays failed and uninterpreted; original R/A and all 0180 samples
stay immutable. No B retry, wider tolerance, replacement Topdown event or new
interpretation of the valid-looking prefix is permitted.

The questions remain Q1 (fewer instructions but more cycles on numeric/range)
and Q2 (fib instruction growth). This record can narrow compatible mechanism
categories; it cannot locate an executed instruction sequence, attribute costs
to a particular spill, or establish why the compiler changed generated code.

## 2. Exact retained subjects and fixed method

Start from rnx c5ab6b6640ae4f9dc81d0dc3a5896bfd422d55a3 and rnx-bench
0180's reviewed closure, d3feef3389ef48695b21465f0f20ae2954694dbd. No rebuild.
Use only the retained 0179 PRIMARY artifacts:

- base SHA-256 af06e8b3b1621b7f763dfb39f967fab1cca0b9b615b145152634463cefbb0c87;
- candidate SHA-256 e4a5f207c38a8ae59bc311e3611a6bdd3d542fe82665a8e12b201f61eaf9a787.

Preserve the stage path, script hashes, arguments, outputs, E0 environment,
CPU 4 affinity, instruction budgets and complete fresh-process windows from
0180. Keep workload order numeric, range_signed, range_negative, fib, calls,
while, run-empty. No warm-up subjects, hot-region instrumentation, transformed
source, longer loop, native timer rewrite or new input.

One official run only: R first, then eligible C, D and E in that order. Each
completed group has five repetitions, each workload base/candidate/candidate/base,
ten samples per side per window. At most 560 subject invocations (140 per group).
Do not fill in discarded/incomplete groups or repeat a sample. R is a fresh
anchor measurement, not a reuse of 0180 R counts.

Reuse the reviewed 0180 event definitions exactly, including anchors in EVERY
diagnostic group, strong pinned cpu_core/user-only grouping, --no-scale and the
same raw encodings. No weak fallback, multiplexing, metric expansion or alias
substitution. A and B are neither discovered nor measured in this record.

| Group | Fixed hardware events in addition to instructions/cycles |
| --- | --- |
| C | BR_INST_RETIRED.ALL_BRANCHES (0xc4/0x00); BR_MISP_RETIRED.ALL_BRANCHES (0xc5/0x00) |
| D | IDQ.DSB_UOPS (0x79/0x08); IDQ.MITE_UOPS (0x79/0x04); ICACHE_DATA.STALLS (0x80/0x04) |
| E | MEM_LOAD_RETIRED.L1_MISS (0xd1/0x08); LD_BLOCKS.STORE_FORWARD (0x03/0x82) |

D's stall event is cycles, not an instruction-cache miss count; DSB/MITE counts
omit other delivery sources. E's events are load/block observations, not total
memory latency. Zero diagnostic counts are allowed; undefined ratios stay
undefined, never zero. No semantic change from 0180's pinned Intel/Linux source
revisions, event restrictions or interpretation limits.

## 3. Fresh discovery and admission

Do NOT carry 0180's old host identity forward as the current admission receipt.
After plan and driver-controls acceptance, observe a fresh filtered identity:
vendor/family/model/stepping/microcode, CPU 4 online/core/SMT topology, cpu_core
PMU type/mask/formats/required aliases, kernel/perf, watchdog/paranoid and
readable frequency driver/governor. Retain statuses and lifecycle for identity
commands. GenuineIntel family 6/model 0xB7 and CPU 4 on cpu_core remain required;
a different CPU/event-table mapping stops for review. An OS/perf/microcode
change relative to 0180 is disclosed, not silently treated as unchanged.

Use exactly two bounded availability/format/affinity opens for each of R/C/D/E
on the frozen grep affinity probe, no experiment workload. Eligibility uses
only status/output/lifecycle/format/domain/running validation, never magnitude.
Each whole group is eligible only if both opens pass. R failure stops globally;
an unavailable C/D/E is reported untested and omitted whole. With no eligible
diagnostic group close INCONCLUSIVE at discovery without subjects. No repeated
open to achieve availability and no partial event replacement.

Retain every discovery row. Send fresh identity/availability for review BEFORE
rehearsal. Freeze reviewed receipt byte hashes, exact group definitions and the
order derived from ALL eligible diagnostics. Admission must bind the manifest,
companion identity and current filtered identity before staging any subject.
Recheck that identity before each diagnostic group; drift stops globally.
Loadavg may be reported but is not an identity. No governor/watchdog/SMT/root or
perf-permission changes. Missing PMU/enabled fields in perf output remain named;
argv is the cpu_core binding, and output facts are not invented.

## 4. Stop policy: independent diagnostics, global validity prerequisites

This policy is set BEFORE discovery or any new outcome. It deliberately differs
from 0180's global-first-failure rule. C/D/E address separate fixed mechanism
questions; a counter-validity failure in one need not consume the others. Their
individual results must not be pooled into a supposedly complete study.

Global stops, with no later group:

- any R failure, including unavailable counters or failed reproduction anchors;
- any failed instruction/cycle reproduction anchor in a completed diagnostic;
- subject/source/receipt/hash or host/affinity/E0 drift, wrong subject output,
  nonzero subject/perf command status, timeout/interruption, missing retention,
  surviving/unreaped process, sentinel hit or scan failure;
- an unexpected exception, unclassified failure or outer deadline.

Group-local counter validity failure: missing/duplicate/unexpected/non-finite
counter, unexpected units/runtime/domain, not-counted or running below 100%,
as reported by the diagnostic-row parser. Zero diagnostic counts are allowed;
a zero denominator is reported as an undefined quantity, not a group failure.
There is no derived range gate for C/D/E. Retain the failing raw row, mark the entire diagnostic FAILED/INCOMPLETE, discard no rows, create
no scientific summary for its valid-looking prefix, and stop its remaining
samples. Do not retry or reclassify it as unavailable. Proceed to the next
originally eligible group only after global lifecycle/status/output/hash/
affinity/host checks have passed. If a failure cannot be isolated to this
explicit counter-validity class, it stops globally. A command status failure
stops globally even if counters appear usable.

The failing sample's instruction/cycle fields must still be parsed and checked
for valid counts/domain/runtime before treating an error as diagnostic-local;
an invalid anchor counter is global. Completed diagnostics additionally must
pass the unchanged 0179 reproduction anchors before interpretation or moving
on. Because an incomplete group cannot establish full reproduction, its
prefix cannot support any mechanism claim. Later groups stand only on their
own complete valid samples and anchors, not on the failed group's prefix.

Classification order is fixed: first lifecycle/status/exact output/stage hashes
(any failure global), then JSON counter-line parsing (malformed lines global),
then independent validation of the instructions/cycles rows (exactly one of
each, finite positive counts, correct domain/units, valid runtime and 100%
running; any failure global). Only after those prerequisites pass may a full-
group diagnostic-row failure be group-local. Negative diagnostic values,
unsupported/not-counted markers and enabled/runtime inequality where enabled
is supplied are included in that local parser class. A simultaneous anchor
and diagnostic defect must never be classified local. R has no diagnostic
rows, so every R failure is global. These clarifications were accepted before
any new driver discovery or measurement.

Report overall COMPLETE only if all eligible diagnostics complete and reproduce.
Otherwise report PARTIAL COUNTER-VALIDITY STOP if a diagnostic failed locally
and others completed, or GLOBAL STOP if a global stop occurred. All-unavailable
and all-failed diagnostics support no mechanism conclusion. No success label
may erase a failed group. The original 0180 STOP is unchanged in every case.

## 5. Anchors, quantities and interpretation

Keep 0180's frozen reproduction against the pinned 0179 raw evidence: each
side's instruction median within 2%; instruction effect within 0.5 percentage
points for the six nonempty windows; each side's cycle median within 10% and
cycle effect within 3 percentage points for those windows. Run-empty cycles
are descriptive; its instruction identity stays gated. Inclusive rounding EPS
remains 1e-12. Failure is NONREPRODUCING/GLOBAL STOP, not automatically a
frequency diagnosis. No updated reference or historical-value cherry-picking.

Same-sample ratios and per-operation quantities stay exactly as in 0180:
C branch misses/branch, branches/instruction and counts/operation; D DSB share
of DSB+MITE and cache-stall cycles/cycle; E missed loads/instruction and
store-forward blocks/instruction, plus counts/operation. Raw counts remain
alongside metrics. Report all seven windows, both sides, five paired ABBA
median contrasts and the base p10-p90 width. A direction is descriptively
resolved only when all five contrasts have one strict sign AND the pooled
median difference exceeds that width. This is not a confidence interval or
causal test. No new significance criterion or optimized workload selection.

A resolved C/D/E change may support a mechanism category compatible with the
static differences and cycle excess; it does not prove the altered generated
code caused that counter change. Unresolved or zero events do not establish
equivalence or refute every frontend/backend/branch explanation. No inference
from a failed/incomplete group. Compiler partition/layout/inlining causation
and optimization adoption remain separate, unapproved questions.

## 6. Driver, controls, lifecycle and review checkpoints

Claude adapts the reviewed 0180 driver into a NEW 0181 driver, leaving 0180
source/results unchanged. Record source and shared-helper hashes. Keep raw
records before parsing, fixed E0 children, phase clocks after shared-lock
admission, min(sample limit, remaining phase time), bounded process-group kill,
reap and survivor checks. Official phase 1,800 s; discovery/rehearsal each
600 s; samples at most 120 s; existing bounded cleanup with five-second live-
survivor settling remains disclosed. No timeout extension or optional
work after a global stop. Sentinel scan on success/failure and its outcome
must be retained; scan failure prevents COMPLETE.

Before fresh discovery: synthetic controls prove independent-group classification,
invalid anchor/global precedence, group-prefix retention, no retries, exact
eligible order, no commands after global stop/deadline, and COMPLETE versus
partial/global status. Replay parser/units/100%-running, hashes/identity drift,
output/affinity, summary/anchor boundaries, compressed sentinel and scan-error
controls. Nothing synthetic executes an experiment subject. Receipt and
source tampering must stop before samples, including omitted eligible groups.

After the fresh availability review, one untimed rehearsal checks admission,
both staged hashes without execution, successful CPU-4 affinity probe and
clean deliberate failure status 1, retained references, sentinel scan and
planned order/count. Send receipts for official approval. No official subject
runs until that approval. Then exactly one locked official attempt; report its
registered outcome and send retained evidence for closure review. Repairs or
replays require prior review and every attempt is retained; no tolerance,
method, event/window or stop-policy change after observing outcomes.

The shared lock is /tmp/rnx-runtime-bench.lock, including discovery, rehearsal
and official measurement. No companion build/timing competes with it. No
secret-valued environment dump or new credential persistence. No engine edit,
profile change or product-suite rerun is needed for this measurement-only cut.
