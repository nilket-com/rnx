# rnx 0170: interpreter profile on the engine-development base

**Status:** plan, by Claude, for Codex's review. This is W3 groundwork in `plans/rune_roadmap.md`. Claude implements; Codex reviews and reruns the gates independently. It runs in parallel with 0169, which covers W0 and W2. The record profiles only: it changes no fork source, rnx dependency, language semantics or budget contract, and it chooses no optimization.

## 1. Question

Where does the interpreter spend its time on fork main, and why does main run 1.22–1.28× more instructions than 0.14.2 on the same scripts (0168)? The answer should be specific enough that W3's first change record can name its target, its expected instruction saving and its risk.

**Scratch observations made before this plan** (one unpinned-to-PMU sampling run each, not evidence):
- **Numeric loop:** about 7–15% of the time is a heap-allocated `Option<Value>` per iteration from `RangeIter::next`, called as a native function (both engines). Main adds `dismantle::Worklist` (about 11%) and `Stack::copy` (about 6%).
- **fib:** about 65% is `Vm::run` dispatch; `internal_cmp` takes 6–8% for `n < 2`; call-frame push and pop plus stack resize take another 5–10%.

These are the hypotheses this record tests, not findings.

## 2. Subjects and pins

- The unmodified 0168 engine harnesses (`primary`, rnx-bench `20f9806`): old Rune 0.14.2, and main `bb8e69372353c50e271c9f115bc771c77aa6b83e`.
- A **symbolized profiling build** of each: the same source, lock and harness, release profile plus `debug = "line-tables-only"` and frame pointers (`-C force-frame-pointers=yes`), in separate targets.
  - **Check:** the profiling build's instructions:u per workload must be within 3% of `primary`'s. Otherwise stop, because the profile wouldn't represent the measured binary.
  - Binaries, hashes, flags and locks are recorded.
- **Workloads:**
  - the 0168 fixtures: numeric, fib, strings, answer, empty;
  - plus four new focused kernels, each validated by an independent Python oracle:
    - (a) a `while` loop with manual increment, which separates loop iteration from range iteration;
    - (b) integer comparison in a loop;
    - (c) calls to a trivial function, for call overhead without recursion;
    - (d) vector push/index, for collection access.

## 3. Method

Every CPU-heavy job (cargo builds, timing, perf stat and record, instrumented runs) runs under `flock --exclusive --timeout 1800 /tmp/rnx-runtime-bench.lock`, shared with 0169. The lock is acquired before any deadline or clock starts, queue time is never measured, and descendants are reaped before release. Read-only source work doesn't take it. They're pinned to logical P-core 4 (`cpu_core` PMU) and record the 1-minute load average before and after.

1. **Instruction and cycle counts** per workload and engine (`perf stat -e cpu_core/instructions/u,cpu_core/cycles/u`): 5 repeats, unsampled, on the `primary` binaries.
2. **Sampled profiles** (`perf record -e cpu_core/cycles/u` at a fixed frequency, with call graphs through frame pointers): 3 repeats per workload and engine on the profiling builds. Report self and inclusive shares per symbol, collapsed into stable categories (dispatch, call/return, comparison, arithmetic, iterator protocol, allocation/free, value drop and dismantle, stack copy and resize, budget accounting, other). Every category mapping is written out in a table, and nothing is left in "other" above 5% without a breakdown.
3. **Per-opcode dynamic counts:** a gated counting feature in a diagnostic patch (the 0169 approach). Opcode executions per workload on both engines, never enabled in timed binaries. This tests whether main executes more opcodes, or the same opcodes at a higher cost per opcode.
4. **Allocation counts** per workload (the counting allocator from 0168, separate passes), to confirm or refute the per-iteration `Option` allocation and to compare old and main.
5. **The main-vs-old difference.** This is two separately measured things, never multiplied together:
   - **(a) Retired-instruction shares:** sampled on `cpu_core/instructions/u` at a fixed period, so self-category shares estimate instruction distribution. Skid and confidence limits are stated: binomial 95% intervals per category from the sample counts.
   - **(b) Cycle shares:** from §3.2, reported as cycles, alongside the whole-workload instructions:u and cycles from §3.1.

   Only disjoint *self* categories are summed; inclusive shares are labelled and never summed. A category is "named as explaining part of main's increase" only from (a): (main share × main instructions) − (old share × old instructions), with its interval. Expected savings in §4 are hypothesis bounds on measured relevant costs, not causal estimates.
6. **Sample mass and windows (pre-registered):**
   - Each engine × workload profile aggregates repeated identical invocations until it has **≥10,000 attributable samples across 3 repeats**, with per-repeat counts and their variation reported; otherwise that profile is marked insufficient and its shares aren't interpreted.
   - Startup, registration and compile are accounted separately from the run region. Either the harness's compile-once `reuse` mode is used for the run region (outputs validated on every call; how reuse differs from fresh runs disclosed), or startup samples are attributed by call stack to their own category.
   - Native, library and unknown samples stay in every denominator.
   - The sampling event, period and frequency are stated. Any lost or throttled samples (perf's LOST records, throttle messages) fail that profile's decomposition.
7. **Stop rules (not targets to widen):**
   - the 3% symbolized-build representativeness gate;
   - the 2% baseline gate.

   If frame pointers fail the representativeness gate, stop and send Codex an alternative method design (e.g. symbolized unmodified code with DWARF unwinding) before any replay.
8. **Opcode-counting patch:** a separate source patch and cargo feature. It gets its own semantic checks: outputs and error classes identical with counting on and off, plus the inventory and corpus checks. It's never enabled in timed or profiled binaries.

## 4. Deliverables

- An rnx-bench probe `probes/interp-profile-0170` with build scripts, kernels, oracles, raw perf data (`perf script` output kept compressed), analysis, and a report that leads with the category table.
- rnx `plans/0170_interpreter_profile_evidence.md`:
  - the attribution;
  - the regression decomposition;
  - a **ranked list of candidate changes**, each with:
    - the measured cost it targets (instructions and share);
    - an expected upper-bound saving;
    - the semantics at risk (ownership, budget, errors, async, drop order);
    - the differential tests it would need.
- The first W3 change record is chosen jointly from that list, not in this record.

## 5. Gates

- **Validation:** every workload output is checked against its oracle before any timing; the analysis refuses missing or duplicate repeats, PMU running below 99%, or unmapped symbols above the threshold.
- **Profile representativeness:** the 3% instruction check from §2.
- **Baseline reproduction:** `primary` instructions:u within 2% of 0168's retained counter medians for numeric and fib, otherwise stop and diagnose.
- **No overlap:** the lock ledger shows no 0169 and 0170 CPU-heavy windows overlapping.
- **Method honesty:** opcode counts, sampled instruction attribution and elapsed times are different measurements. Report each only for what it measures, including unassigned samples. Never infer exclusive opcode costs from inclusive call stacks, and never multiply opcode counts by an assumed fixed cost. Source-level explanations stay hypotheses until a control supports them.
- **Clean subjects:** deciding main-vs-old comparisons (instructions, cycles, timing) use clean unmodified builds only. Debug/frame-pointer and opcode-instrumented builds are separate, labelled, and report their measured overhead.
- **Isolation:** the work happens in a dedicated rnx worktree and branch (`w3-0170`) and a dedicated rnx-bench worktree and branch, never the root checkouts' index. It's integrated serially with explicit hashes after review; neither of us amends the other's commits.
- **Commits:** plan and impl in rnx; `probes:` in rnx-bench. Codex reviews before push.

## 6. Amendment after the frame-pointer stop (agreed with Codex before any replay)

Step A's frame-pointer build failed the 3% representativeness gate (old +0.9% to +6.1%, new +3.7% to +6.3% instructions:u over primary). It's retained as evidence and not used for attribution. The baseline gate passed (0168 FIFO counts reproduced, ratio 1.0000). Replacement method, under the same unchanged 3% gate:

1. **Build S2:** the same source, lock and release profile, plus `debug = "line-tables-only"`, with no frame pointers. Per-workload instructions:u must be within 3% of primary. The `.text` section hash vs primary is reported as a stronger check.
2. **Self-sampling:** separate passes for `cpu_core/instructions/u` and for `cpu_core/cycles/u`, at a fixed period of 100000 with no unwinding. Category instruction estimates use only the retired-instruction samples, with skid and binomial uncertainty stated. Only disjoint self categories are summed.
3. **Sample mass, per event × engine × workload:**
   - The invocations per repeat are chosen before sampling, from the untimed step-A counts, to target about 20k samples per profile across 3 repeats (gate ≥10k).
   - Repeated identical invocations are aggregated, with their startup and compile samples kept. Each invocation's output is validated.
   - A profile failing the sample, LOST or throttle gate is marked insufficient, with no percentages.
4. **Separate DWARF pass** (`--call-graph dwarf,16384`), for classification only:
   - Startup/compile vs VM run is decided from stacks. Unclassified samples stay in the denominator.
   - More than 10% unclassified means no startup/run split for that workload, and then no claim that attributes an increase specifically to execution.
   - A shared function such as the allocator is assigned to the VM or the compiler only by its classified stack, never by its symbol alone.
