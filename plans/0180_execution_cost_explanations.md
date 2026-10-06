# rnx 0180: explain the stopped startup candidate's execution costs

**Status:** plan for Claude's review. The user approved the next cut. Codex owns retained-artifact analysis and independent reconstruction; Claude reviews and owns any subsequently accepted measurement driver. This is an explanation study, not another optimization candidate or an adoption decision.

## 1. Two questions and fixed boundaries

0179 closed as STOP. Its startup-only source change reduced registration work, but unrelated script execution regressed. Keep these questions separate:

- **Q1, cycles:** numeric, signed-range and negative-range whole-process instruction medians decreased about 0.8%, while cycles increased about 8.3%, 16.1% and 16.4%. What differences in the retained generated code could account for the extra cycles, and what evidence could distinguish them from host effects?
- **Q2, instructions:** fib's whole-process instructions increased 2.283%. Can changed generated code identify additional work on its execution path, separately from Q1's per-instruction cost?

All subjects include the changed startup registration. The script loops do not invoke its new helper; saying the entire workload never executes changed code would be false. A whole-process instruction decrease need not mean a loop executes fewer instructions: startup savings can offset loop growth. Static instruction counts are not retired instruction counts. Resolve that scope before attributing Q1 to instruction efficiency.

No engine or harness edits, new candidate, rebuild, profile substitution, source-level padding/alignment experiment, dependency change, gate relaxation or resurrection of 0175/0176. Rune fork main remains bb8e69372353c50e271c9f115bc771c77aa6b83e; shipped rnx remains on 0.14.2. 0179's decision and raw evidence are immutable. The campaign's regression gates and product decisions belong to the user; this study changes neither.

## 2. Inputs and provenance

Start from pushed rnx 4c2cc6aa25ddb2d72f480f7300e88b48aa3504f7 and bench 2110c0b201db670f5aee4a4718f9533847717e79. The retained six executables are at `/home/me/work/rnx-bench-w-0179d/probes/startup-0179/bin/`; this local location is not a new build identity.

Pin `probes/startup-0179/subjects.json` SHA-256 `d3552638c3e0fbc8d9cfe538a3488a35edf0d432454b6c4e9d1359363e1fd8a1` and `results/startup-0179/official1/p0-cand/raw.jsonl` SHA-256 `d0643c0fc1ca42f6c4e9e5d9748c351da546b0baed3d1226349d6e5fab848b03`. Resolve scripts, source/build receipts, profile/features, primary/counter/allocation role hashes and all selected raw sample rows through that manifest; missing or mismatched evidence stops, never rebuilds silently.

Primary executables are the deciding subjects for both PMU and wall: 0179 compare() and wall() stage the primary artifact. Counter executables supply only base historical-reproduction READY/GO windows; their comparison is a secondary feature-set cross-check, not an explanation of deciding samples. Analyze the primary pair as authority and label the counter pair separately. Do not transfer machine identities across roles. Allocation executables are provenance inputs, not extra performance subjects. No historical 0176 machine address or same-demangled-name guess substitutes for a 0179 identity.

The base is 4e84cfacc0278a4f1645c14310453bdc30ac77b4; candidate b28f8cb3345448914ef4e35514d802d1795f16ac. Read the exact frozen source diff and build receipts only to bind generated regions to source and feature/profile settings. No new compilation to obtain debug information. Missing line information remains a limitation.

## 3. Step 0: retained files only

After plan acceptance, commit analysis tooling and controls before the official static analysis. Reading ELF, running nm/readelf/objdump on files and parsing retained raw receipts are allowed; executing a subject, perf, Valgrind, a load generator or a compiler is not. Do not inspect new counter outcomes before a reviewed measurement checkpoint.

Retain full symbol tables, ELF/program/section headers, relocations and disassembly of executable sections, plus exact tool versions/hashes, argv, safe environment, status and output hashes. Use bounded subprocesses and record failures. Full outputs are the authority for excerpts. No smallest-looking function or unrecorded exploratory result selects an explanation.

The initial named surface for both roles is Vm::run, op_call and return/pop_call_frame paths, iterator/range next and conversion/native-call handlers, plus Value/Repr drop paths reached by identified direct call sites. Include transitive direct callees relevant to those paths and explicit unresolved indirect calls. Resolve aliases by ELF address/size and raw mangled identity; demangled names are labels only. Multiple same-named clones must remain separate. Missing standalone symbols can mean inlining, not absent work. Any expanded scope must be justified by an observed call/branch in the retained code and recorded, not by a speculative unrelated cost.

First reconstruct the instruction medians and whole-process deltas from retained raw rows. Use run-empty as a startup-only reference and report each script workload's delta minus run-empty's delta, alongside the unsubtracted values. This is a retained-data diagnostic contrast, not a newly measured phase or exact process-floor correction. Bind iteration/call counts to the frozen scripts; distinguish loop iterations, recursive invocations and returns, derive fib's recurrence, and state unmeasured setup/compilation residuals. Report the remainder from predicted integer instructions per operation without rounding it away. Do not accept a small remainder alone as causal proof.

Claude's pre-analysis predictions are numeric approximately -12 instructions per iteration, signed/negative range -10, while -7, range-while -5 and calls +10 per call. Fib's adjusted increase is approximately +20,975,687 instructions, with the exact call-count binding to be derived rather than guessed. These are predictions to check against raw medians, script counts and disassembly, not facts copied into the result. If the binding and residual checks hold, they support fewer loop instructions beyond startup and weaken pure placement of identical code as a complete account. Generated-code changes may still coexist with placement effects. Record contradictions explicitly.

For each compared region report:

1. Raw symbol identity, address, extent, section and aliases; bytes and decoded instructions, including padding. Distinguish a symbol's extent from a validated function boundary.
2. Instruction-level differences and a conservative control-flow comparison: opcode, operands, registers, widths, branch structure, direct/indirect calls, memory operations, spills/reloads and inlined versus outlined work. Exact bytes and normalized views are both retained. Normalization may remove only resolved relocation/address differences, with the resolved target identity still compared; it may not erase immediates, register choices, memory dependencies, branch direction, call identity or padding. An unmatched target is unresolved, never normalized to equality.
3. Function-relative basic-block entries, backward branch targets and fall-through boundaries; their virtual-address residues modulo 16, 32 and 64, instruction/block crossings, and relevant ELF file mapping. These are static candidate loop heads, not measured hot addresses. Do not infer runtime branch frequency or decoded-uop-cache occupancy from residues.
4. Which source/harness path can reach each region, with scope and uncertainties. No 0179 instruction-pointer profile exists to establish exact execution frequencies. Source and disassembly provide possible paths, not dynamic edge counts. PIE load bias/page offsets and indirect-call uncertainty are stated explicitly.
5. The relationship to the retained whole-process data: instructions, cycles, wall medians and base bands for numeric, signed/negative range, fib and calls; while/range-while/manual-next and context/run-answer as retained contrasts. No new arithmetic gate or cherry-picked sample subset. Explain startup/loop accounting limits and report cycles per retired instruction without treating it as a diagnosis.

For Q2, enumerate concrete additional/removed call, drop and instruction sequences on fib's possible return/dispatch paths. If exact dynamic counts are unavailable, do not claim these explain all 2.283% or reuse 0176's Callgrind counts as 0179 counts. For Q1, classify each relevant region as exact code with changed placement, changed code with comparable structure, or unresolved. A mix is allowed; do not force a single label on the binary.

Tool controls use synthetic fixtures and copied retained files, never execute an experiment subject. Require refusal on wrong role/hash, truncated disassembly, missing requested region, duplicate/ambiguous identities claimed unique, unresolved target claimed equivalent and altered raw instructions masked by normalization. Positive controls cover relocation-only changes, same-name different-address symbols, instruction/register/immediate differences, boundary crossings and unsupported formats. Independently verify key identities and excerpts with stock binutils rather than trusting one parser. Machine-specific performance interpretations require a cited primary vendor source at the checkpoint; residue tables alone need no architectural performance assumption.

## 4. Alternatives registered before analysis

| Explanation | Evidence that would support it | Evidence against or insufficient evidence |
| --- | --- | --- |
| Placement/alignment or frontend delivery | Corresponding execution regions have equivalent instructions/control flow but changed boundaries; a reviewed subsequent frontend-event comparison changes coherently with cycles. | Address changes alone are only compatible evidence. Changed sequences confound pure placement. Equal inspected residues weaken that specific alignment story but cannot exclude other layout/cache effects. No universal 32/64-byte benefit is assumed. |
| Generated code, inlining or dependencies | Changed spills, calls, drops, instruction ordering or dependency chains on reachable paths; Q2 may locate additional work, Q1 may show a different cost per operation. | Different whole-file hashes or static function sizes alone do not establish executed work. Equivalent inspected sequences weaken that local explanation, not every uninspected path. |
| Cache or branch prediction | A reviewed native event comparison reports reproducible changes in relevant misses/stalls, with denominators and event semantics tied to this CPU. | Generic cache events cannot identify instruction-cache or uop-cache causes; counters can be correlated symptoms. Unavailable events mean untested, not zero misses. |
| Frequency, scheduling or other host effects | Effective frequency/reference-cycle relation, migrations/context switches or host drift accounts for the cycle/wall difference in the reviewed paired run. | 0179's ordering and cycles agreeing with wall reduce some timing-artifact concerns but do not exclude host effects. Process-level cycles are not a direct core-frequency reading. |
| Startup/loop aggregation | Saved startup savings offset extra loop instructions; role-matched generated paths differ, leaving whole-process totals insufficient to locate the change. | Resolve the retained run-empty contrast and iteration binding first. If it holds, the observed reduction beyond startup supports fewer loop instructions and resolves the hidden-loop-growth concern within that diagnostic scope; remaining setup/compile differences and causal attribution are still named. |

These explanations can coexist. Do not conclude compiler CGU partitioning caused a finding merely because source/profile settings were unchanged or an earlier cgu=1 result differed. No current physical microarchitecture detail is guessed from a generic x86 label.

## 5. Reviewed checkpoint and conditional single measurement

Deliver the static report, source/binary/tool hashes, controls and a table of supported, weakened and untested alternatives for Q1 and Q2. Claude reviews before any subject execution. Step 0 may close **INCONCLUSIVE** if identities fail or no specific feasible measurement can distinguish the remaining alternatives. Static code changes may support a limited explanation while leaving cycle causality or its magnitude unproven.

Only if Step 0 identifies a discriminating question, propose one immutable measurement protocol as a plan amendment, with exact subjects/workloads, CPU/event definitions from primary documentation, supported-event discovery procedure, predicted directions, missing-event policy, sample order/counts, running-percentage requirement, noise comparisons and interpretation rules. A discovery/preflight itself requires this review; do not run perf to explore available outcomes under Step 0. No replacement event after outcomes or extra candidate/profile is allowed.

The preferred shape, if feasible, is the same retained role-matched binaries with additional native PMU events on the frozen numeric/signed/negative-range and fib/calls workloads and explicit controls. At least instructions and cycles anchor reproduction; limited programmable counters may require pre-registered separate nonmultiplexed groups. Exact grouping, reference-cycle/frequency observations and wall scope must be reviewed rather than assumed safe or available. No sampling, Callgrind or dynamic-edge attribution is implicitly approved; if needed instead of that shape, justify it in the checkpoint. One measurement design at most proceeds, not a sequence of diagnostic variants until an explanation fits.

Claude commits and rehearses the driver and corruptions for Codex review before the official run. Correctness/output, role/hash identity, E0 environment, affinity and process cleanup remain required. Reproduction/noise tolerances are set before any new outcome. An unsupported event, failed reproduction, counter scheduling/retention failure or unexplained artifact change stops and is reported; no silent event substitution, longer run, rebuilding or tolerance widening. One official run only; infrastructure repairs/replay require prior review and every attempt is retained. New diagnostics cannot revise the scientific 0179 STOP.

## 6. Meaning for the campaign and closure

This study has no WIN/adoption category. It closes with bounded explanations supported by the observed evidence, or **INCONCLUSIVE**, or a named provenance/tool/measurement **STOP**. State Q1 and Q2 conclusions separately, and distinguish located cost, compatible mechanism and demonstrated causality.

- Identifiable generated-code changes would motivate a separately reviewed compiler/code-generation investigation; they do not make a real regression irrelevant.
- Placement-related evidence would motivate a separately reviewed controlled layout experiment, not a claim that every future VM change is harmless or that gates should be waived.
- Host-dominated or nonreproducing results would limit confidence in attributing this saved regression, without deleting historical results or relaxing future gates.
- Inconclusive results are a useful boundary: no guessed remedy, automatic next variant or new performance promise.

Any proposed net-benefit gate, profile adoption or engine integration is a separate user decision and future record. The current gate remains binding throughout; none of these outcomes retroactively accepts stopped candidates.

All compilation/subject execution/measurement, if subsequently approved, holds `/tmp/rnx-runtime-bench.lock`. Static tooling also uses the lock to avoid competing with the campaign; deadlines begin after admission. Use fixed safe environments, bounded process groups/deadlines/reap and survivor checks, no inherited-environment dump. Preserve failed outputs and commands. Replay existing fake-secret retention controls before a measured stage and scan new tracked/decompressed artifacts in memory with positive controls; credential rotation remains separately unconfirmed. Do not print or retain secret values in the ledger.

Publish generic plan/evidence in rnx and analysis/tooling/receipts in rnx-bench, preserving authors and serial integration. No fork source commit is needed for this record. Send the plan for review now; no Step 0 disassembly or new diagnostic has been run while drafting it.

## 7. Amendment after the accepted Step 0: one paired native-event study

**Status:** protocol for Claude's review, not permission to execute yet. Step 0
rnx ee2826b / bench 751ab29e is accepted. Documentation research alone has been
done while drafting this amendment; no local CPU/event discovery, perf open,
subject execution or new timing. Claude owns the driver after acceptance;
Codex reviews discovery, controls and rehearsal before the one official run.

### 7a. Exact subjects, host and documentation

Use only the two retained PRIMARY binaries, staged and hash-checked before and
after every sample by 0179's existing path:

- base SHA-256 af06e8b3b1621b7f763dfb39f967fab1cca0b9b615b145152634463cefbb0c87;
- candidate SHA-256 e4a5f207c38a8ae59bc311e3611a6bdd3d542fe82665a8e12b201f61eaf9a787.

CPU 4 only. Claude reports an i7-14700 and cpu_core CPU mask 0-15; this is an
input to verify after acceptance, not a host probe already performed here.
Discovery must bind vendor/family/model/stepping, core type, cpu_core type/mask,
CPU 4 online/affinity and SMT sibling topology. Require GenuineIntel family 6
model 0xB7 and CPU 4 in cpu_core's mask; a mismatch stops for review, not a
silently selected different Intel table. Record kernel/perf identity, relevant
PMU formats/aliases, NMI-watchdog state, SMT sibling availability and CPU 4's
frequency-driver/governor data where readable. No disabling SMT/watchdog,
locking frequency, changing governor, privileges or perf_event_paranoid.

Primary documentation, pinned independently of the machine's installed perf:

- [Intel processor/event mapping](https://github.com/intel/perfmon/blob/78eb739dafa28c1b296f7b4d5fb7e1a7e81b1537/mapfile.csv), SHA-256 4fbadd7b948634d311b63be6f3da792790cb8f3f5fd225073cb6e5d8c90c51db. Its 6-B7 Core entry maps to the table below, not the Atom table.
- [Intel Core event definitions](https://github.com/intel/perfmon/blob/78eb739dafa28c1b296f7b4d5fb7e1a7e81b1537/ADL/events/alderlake_goldencove_core.json), SHA-256 d588ba821297c4097705214707b2f7d52ed173ec6a4ad9acf0fc3649e6e43a0f. Instructions/cycles/reference-clock/slots definitions and programmable event encodings below come from this exact table, including counter restrictions.
- [Linux perf modifiers/group semantics](https://github.com/torvalds/linux/blob/22430ae5d90ab288b0ee2ad99ae941f4a666b694/tools/perf/Documentation/perf-list.txt), SHA-256 d41c182377abd1088972e6f94563119bb3a06d61022cf8ee8338f7bbe8f40d4b. Strong groups, user filtering and PMU pinning are explicit; weak fallback is forbidden.
- [Linux Topdown interface](https://github.com/torvalds/linux/blob/22430ae5d90ab288b0ee2ad99ae941f4a666b694/tools/perf/Documentation/topdown.txt), SHA-256 dc323906046c38e8686d0db37442c80b29fc2aa4a51f35ddd6e76f916755b4e8. Its pseudo-events represent slot counts; slots must lead the group. This documents an interface to verify on the installed kernel, not proof of local support.

Retain retrieved documentation hashes and the selected definitions in a source
receipt. Do not introduce a different event table if the installed perf's names
or encodings disagree. Driver event semantics come from the pinned definitions,
not name similarity or an unlabelled generic cache event.

### 7b. Frozen groups and discovery policy

Every hardware group is user-only, explicitly cpu_core, strongly grouped and
pinned (D modifier), with instructions and cycles as anchors. Use perf stat
JSON and --no-scale; no weak-group fallback, --metric-no-group, multiplexed
scaling or auto-selected metric expansion. All collected hardware rows must
report 100% running and valid runtime/counts. If exact enabled/running times
are provided, require equality too. Printed 100% alone is rounded evidence;
pinned strong-group scheduling and no scaling are part of the nonmultiplexing
contract. A parser cannot invent absent exact timing fields.

Freeze these six groups, in order. R is required. A-E are optional as WHOLE
groups; one absent/incompatible event makes that group untested, not slimmer.
The basic anchor remains in every optional group that survives discovery.

| Group | Members beyond instructions/cycles | Meaning / binding |
| --- | --- | --- |
| R, reproduction | none | Same user instruction/cycle meanings as 0179, bound explicitly to cpu_core. |
| A, frequency/host | cpu_core reference cycles | Reference-clock count alongside actual cycles (the discovered cpu_core/ref-cycles alias is event 0x3c/umask 0x01, CPU_CLK_UNHALTED.REF_TSC_P, a documented programmable encoding; no fixed-counter allocation is assumed); no APERF/MPERF or MSR substitute. Collect per-task context-switches, cpu-migrations and task-clock outside the hardware group in the same invocation. These software counts are scheduler observations, with their distinct domain labelled. |
| B, top-level slots | slots (leader), topdown-retiring, topdown-bad-spec, topdown-fe-bound, topdown-be-bound | Four slot-count fractions over the SAME slots count. Kernel aliases must resolve to the documented slots/metric pseudo-event interface; anchors remain members of that slots-led group. |
| C, branches | BR_INST_RETIRED.ALL_BRANCHES (event 0xc4, umask 0x00); BR_MISP_RETIRED.ALL_BRANCHES (0xc5, 0x00) | Completed branch count and completed mispredicted-branch count. Two programmable counters. |
| D, frontend delivery | IDQ.DSB_UOPS (0x79, 0x08); IDQ.MITE_UOPS (0x79, 0x04); ICACHE_DATA.STALLS (0x80, 0x04) | Delivery from decoded-uop versus legacy-decode paths, plus code-fetch stall cycles related to the instruction cache. Three programmable counters, each restricted to counters 0-3. The last event is NOT a count of L1 instruction-cache misses. |
| E, backend loads | MEM_LOAD_RETIRED.L1_MISS (0xd1, 0x08); LD_BLOCKS.STORE_FORWARD (0x03, 0x82) | Completed loads missing L1 data cache; blocked forwarding cases. Two programmable counters restricted to 0-3. Forwarding blocks are not spill counts or all backend stalls. |

Raw programmable spellings are fixed as
cpu_core/event=EVENT,umask=MASK,name=FROZEN_NAME/u, with no edge/cmask/invert or
sampling additions. Anchor/reference-cycle aliases and the five B aliases must
be verified against perf details/sysfs definitions before use; the accepted
receipt pins their exact resolved encodings and full argv. Alias spelling
resolution is mechanical only, not a choice between different events. Missing
B support or inability to group B with its anchors means B is untested. Do not
compute an approximate Topdown decomposition with replacement raw events.

After amendment acceptance only: capture filtered CPU/PMU identity and perf
list/details for this shortlist, then perform exactly two bounded open checks
per group (where names/format are available), using the existing affinity probe
`/usr/bin/grep Cpus_allowed_list /proc/self/status`, pinned to CPU 4. These are
availability/format/affinity controls, not trials on the experiment workloads.
No check's event magnitudes select a group. R failure stops; an optional group
is eligible only if BOTH checks meet parsing, user domain, grouping/pinning,
100% running, exit/output and cleanup requirements. Zero diagnostic-event
counts on this short probe are allowed; they mean neither unavailable nor no
stalls in an experiment. No repeated open check to get a preferred outcome.

Retain every check, including absent and denied events. Freeze an availability
manifest before rehearsal/official samples and have Codex review it. A-E
unavailable is allowed and reported untested. With no optional group available,
close INCONCLUSIVE at discovery; do not run a new anchor-only study as if it
could explain the mechanism. Once eligible, any group failure during the
official run stops; it is not reclassified as optional after seeing outcomes.

### 7c. Workloads, order, anchors and deadlines

Exactly seven unchanged 0179 script windows: numeric, range_signed,
range_negative (Q1), fib, calls (Q2), while and run-empty (contrasts). Preserve
their source hashes, arguments, budgets and exact outputs, including empty
stdout for run-empty. No hot-region instrumentation, source transformation,
compile-once window, longer loop or new input. All remain complete fresh
processes on staged retained primary binaries.

One official run. R executes first, then eligible A-E in their frozen order.
Within each group, five repetitions, workloads in the order above, each as
base/candidate/candidate/base. This gives ten samples per side per workload
per group, at most 840 subject invocations over six groups. No warm-up subjects,
added samples, reruns or adaptive order. Raw records precede parsing/checks;
check each sample immediately for safety/retention and each completed group
for its reproduction anchors before interpreting or starting another group.

Expected wall duration is several minutes (approximately 5-10, not a promise)
including perf startup. Set a reviewed 1,800 s official-phase outer deadline,
120 s per sample and five-second kill grace; clocks begin after admission to
the shared lock. Discovery and rehearsal each have their own 600 s outer limit.
No running limit is extended on expiry. Claude adapts 0179's staged perf/sample
path, changing group/event handling only; no native timer rewrite or separate
resident-wall study. Perf's elapsed/task-clock observations are diagnostic and
are not passed off as 0179's independently clocked wall measurements.

Before new measurements, bind the exact 0179 raw medians and references in a
manifest, reproduced from static2 arithmetic.json and original raw rows. For
EVERY collected group and all seven workloads, each side's instruction median
must lie within 2% of its corresponding 0179 median. For numeric/ranges/fib/
calls/while, the new instruction change (candidate/base - 1) must also be within
0.5 percentage points of that workload's 0179 change. This is a reproduction
criterion, not a modified optimization gate.

For the six nonempty workloads, each side's cycle median must lie within 10%
of its 0179 median, and the new candidate/base cycle change must be within 3
percentage points of the corresponding 0179 change. The three Q1 workloads
must therefore retain a positive excess cycle cost; no disappearance is
silently called a frontend/backend finding. run-empty's short cycle window is
descriptive and exempt from these cycle reproduction tolerances, while its
instruction/hash/output anchors remain required. Its original/new cycle
ranges are still reported.

Failure of those anchors closes **NONREPRODUCING / STOP** and stops mechanistic
interpretation (and later groups). Retain the failure and any already collected
host diagnostics. It is a new observation consistent with time/host variation,
not automatic proof of a frequency cause or grounds to revise 0179's STOP.
No tolerances widen and no subsequent group gets a fresh chance to reproduce.

### 7d. Predictions and interpretation, frozen before events open

Report raw per-process counts, same-sample ratios and source-bound per-operation
views. Summaries use medians of sample ratios (not ratios of unrelated group
medians). For each group/workload also form five paired contrasts: each ABBA
repetition's median candidate metric minus median base metric. A directional
change is called resolved only if all five contrasts have the same strict sign
AND the pooled median difference exceeds that metric's base p10-p90 width.
Otherwise report mixed/unresolved. Do not add sample sizes or select another
statistic. This is a descriptive noise rule, not a causal confidence interval.
Report raw event counts alongside normalizations so fewer retired instructions
cannot turn a constant event count into a falsely described absolute increase.

- **Host/frequency:** A yields same-sample actual/reference cycles (effective
frequency proxy) and reference cycles per instruction, plus task-clock and
context switches. A resolved frequency-ratio shift or scheduler-time change
supports a host contribution; similar frequency ratios with reproducible
excess reference cycles per instruction weakens frequency-only explanation.
Different frequency can also be a workload consequence; do not assert direction
of causation. Any cpu migration is an affinity/protocol STOP, not a useful
frequency datum. Hardware references do not include descheduled time; software
counts have a different domain, explicitly retained.
- **Frontend:** B frontend-bound fraction increasing coherently with excess
cycles supports frontend delivery pressure. D reports raw DSB/MITE deliveries,
DSB/(DSB+MITE), and instruction-cache stall cycles/cycles. Falling DSB share or
rising cache stall burden supports a delivery mechanism; neither identifies
one 32/64-byte boundary, cache conflict, nor all frontend stalls. DSB+MITE is
not total uops (other delivery paths exist). Zero denominator is undefined,
reported, never silently zero.
- **Speculation:** Rising B bad-speculation fraction with rising C branch-miss
counts/rate (misses/branches), consistently in Q1, supports speculation cost.
Branches per instruction can move because the instruction denominator moves;
report both raw counts and misses per source-bound iteration. No event proves
one particular indirect target mispredicts.
- **Backend/dependencies:** Rising B backend-bound fraction with stable A and
no resolved frontend/speculation increase supports backend pressure. E reports
raw forwarding blocks and L1-miss loads, plus per-iteration/per-instruction
views. Increased forwarding blocks is compatible with the altered stack/data
accesses, not proof the changed spill offsets caused them. Stable E cannot
rule out dependencies, execution-port pressure, other cache levels or all
backend causes. No store-block count is added to stalled cycles.
- **Q2 fib/calls:** Compare their fractions/counts separately from Q1. Retiring
slot fraction can increase while total instructions/cycles worsen. These
counters do not dynamically bind the +33-per-fib-invocation instruction delta
to pop_call_frame or its drop target. A dynamic-edge attribution is not added.
- **Contrasts:** while tests whether a broad interpreter/host shift also
appears without the Q1 regression; run-empty exposes startup-dominated event
scope. Their counts cannot be subtracted from fractional metrics or treated
as identically compiled setup for every script.

B fractions are reconstructed from metric slot counts over its same slots
count; preserve perf's raw count and displayed metric fields. All fractions
must be finite in [0,1], sum within 0.02 of one (8-bit metrics/rounding allowed),
and not be confused with percentages or multiply by cycles as if disjoint
elapsed-time components. Violation during the run is a measurement STOP,
never clamped/repaired. Raw negative/non-finite counts or scaled/unsupported
markers in any eligible group also STOP.

Q1 remains unexplained if no coherent resolved metric shift distinguishes the
alternatives, if relevant groups are unavailable, or if only correlated
changes with contradictory controls remain. Report multiple supported costs
when mechanisms coexist. This study can discriminate broad execution-cost
categories and host variation; it CANNOT explain why the compiler emitted
different VM code after a context.rs-only source change. Build/CGU causality
and the user's gate question require a separate record. No name such as
"DSB" converts a correlation into a proven compiler/layout explanation.

### 7e. Driver controls, security and approval sequence

Claude commits the exact driver, availability receipt, parser/event definitions
and untimed rehearsal for Codex review before the official run. Rehearsal
executes affinity and synthetic-output/retention checks, not trial Q1/Q2
workloads; any required exception returns to review first. Controls require:
wrong primary/role hash; changed source/count; mixed cpu_atom event; absent,
extra, duplicate or non-finite counter; missing eligible group; incomplete
ABBA; 99.9% running; weak/pinned-group configuration drift; percentage/slot-unit
confusion; bad B sum; zero denominators; both reproduction tolerance sides and
exact boundaries; scientific nonreproduction versus infrastructure failure;
and fake-secret success/failure/archive retention. Preserve all partial runs.

After accepted discovery and controls, one official run. Source/build/profile/
subject changes remain forbidden. Any plumbing repair/replay needs prior
review and disclosure; a semantic/event failure cannot be relabelled a
completed explanation. Existing safe environments, process groups, deadlines,
kill/reap/survivor/affinity controls and credential scans are retained. No full
machine or inherited-environment dump, secret-bearing paths/values or active
credential rotation claim. No push/adoption precedes joint review of results.
