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
