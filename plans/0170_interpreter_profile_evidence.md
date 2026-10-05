# rnx 0170 evidence: interpreter profile on the engine-development base

**Status:** implementation by Claude, for Codex's review. Profiling only: no fork source, rnx dependency, language semantics or budget contract changed, and no optimization chosen. The rnx-bench probe is `probes/interp-profile-0170`; the results are in `results/interp-profile-0170`. All CPU-heavy work ran under `/tmp/rnx-runtime-bench.lock`. The ledger (`probes/interp-profile-0170/ledger/lock.log`) records every acquire/release and the load averages.

## 1. Subjects and gates

- **Engines:** Rune 0.14.2 ("old") and fork main `bb8e69372353c50e271c9f115bc771c77aa6b83e` ("main"). Harness source and locks come from 0168.
- **Timed/counted subjects:** the unmodified 0168 `primary` and `counter` binaries. Their SHA-256s match `results/rune-base-0168/conditions.json` (checked by `counts.py` before any measurement).
- **Workloads:** the 0168 fixtures (empty, answer, numeric, fib, strings) plus four kernels with independent Python oracles (`oracle.py`):
	- while: the numeric loop as a manual `while`;
	- compare: an integer comparison in a loop;
	- calls: 1M calls to a trivial function;
	- vector: 100k pushes, then 10 indexed passes.

  Every invocation's stdout is validated in every step.

**Gate results:**

| Gate | Result |
|---|---|
| Baseline (2%) | **PASS.** The 0168 FIFO counter method on the 0168 counter binaries gives instructions:u ratios to 0168's medians of 1.0000 (old/main numeric and fib). |
| Representativeness, frame-pointer build (3%) | **FAIL**, the pre-registered stop. Old +0.9% to +6.1%, main +3.7% to +6.3% instructions:u over primary. Retained as `step-a`, never used for attribution. |
| Representativeness, S2 build: line tables, no frame pointers (3%) | **PASS.** −1.3% to +0.001% on every workload and both engines. `.text` hashes differ from primary (debug info plus a package rename); counts are the gate. One S2 attempt was refused by the pcnt-running ≥ 99 check (84%, transient multiplexing); retained as `step-a-s2-attempt1`. |
| Sample mass, LOST, throttle | Period 1e5 was throttled on every profile (kernel `perf_event_max_sample_rate` = 32000/s). Retained as `step-b-attempt1-throttled`; no percentages taken from it. At period **1e6**: 18,852–23,335 samples per event × engine × workload across 3 repeats, **0 LOST and 0 THROTTLE** on all 36 profiles. |
| DWARF classification | **Insufficient on every workload.** Unwinding stopped after one or two frames (up to 99.9% unclassified, also at the maximum dwarf,65528 stack copy), and main's fib/answer/compare passes recorded LOST events. **No stack-based startup/run split exists.** Step D below is *not* a substitute for it (see §2). |
| On/off semantics of the opcode-count patch (strict, R1) | 54 corpus runs. 50 byte-identical. All 54 identical after normalizing only two known fields (`full_name: 0x…` in `AnyTypeInfo`; `thread 'main' (N)` in the stack-overflow abort); status, stdout and the rest of stderr must match exactly. Controls refuse a changed stdout, a changed status, an unrelated VmError, and stack text with a different status. Opcode count files must exist and hold positive counts. Round 1's looser check is retained as `step-e-round1`. |
| Target-process identity (R2) | All 108 retained repeats re-extracted by process name `s2`, with the data's build-id bound to the S2 binary. 230 wrapper samples (bash, seq) excluded and reported per profile; all target libc/loader/unknown samples kept. Every profile still has ≥10k target samples. |
| Fail-closed analysis (R3) | `analyse.py` refuses an incomplete 36-profile matrix, repeats other than {0,1,2}, non-finite/non-positive values, rows not summing to totals, LOST/THROTTLE > 0, or fewer than 10k target samples. Eight controls on mutated copies of the real retained inputs are all refused; the real inputs are accepted. |
| Allocation counts (N1) | Unmodified 0168 `allocation` binaries (hash-checked), compile and run modes, run repeated for determinism (identical). |

## 2. Net run-path work vs context-plus-compile (deterministic counts)

Step D measures whole-process instructions:u on the unmodified primary binaries in `compile` mode (context plus compile, no execution) and `run` mode on the same file, 5 repeats each. The max−min spread is under 0.1%. Run − compile is **net incremental run-path work**: VM setup, the call into `main`, native calls, arguments, execution and the different teardown together. It is not an exclusive execution phase. Step E counts dynamic instructions by kind with a separate diagnostic patch, never timed; it's retained in `probes/interp-profile-0170/opcount-patches`.

| Workload | Opcodes executed (old/main) | Net run-path instr old (M) | Net run-path instr main (M) | main/old | net run-path instr ÷ opcodes, old | same, main |
|---|---|---|---|---|---|---|
| numeric | 9,000,016 / 9,000,019 | 1351.0 | 1727.0 | 1.278 | 150 | 192 |
| while | 10,000,011 / 10,000,014 | 992.0 | 1106.0 | 1.115 | 99 | 111 |
| fib | 5,720,591 / 5,720,594 | 651.8 | 800.9 | 1.229 | 114 | 140 |
| calls | 13,000,011 / 13,000,014 | 1468.0 | 1743.0 | 1.187 | 113 | 134 |
| compare | 15,923,089 / 15,923,092 | 1522.9 | 1679.5 | 1.103 | 96 | 105 |
| vector | 10,100,137 / 10,100,150 | 1190.2 | 1278.3 | 1.074 | 118 | 127 |
| strings | 500,041 / 500,053 | 164.0 | 196.9 | 1.200 | 328 | 394 |

Context plus compile: old 27.4–28.0M, main 28.4–30.8M instructions.

**Findings:**
- **Near-equal dispatch counts.** Main dispatches 3–13 more instructions per workload than old (one extra `Allocate`, a few more in strings and vector). So main's 7–28% increase in net run-path work comes with almost no change in how many instructions are dispatched on these workloads. This doesn't establish that the bytecode is identical.
- **Context plus compile** differs by at most about 2.5M instructions, against main's run-path increases of 33–376M.
- **Net run-path instructions ÷ dispatched instructions is about 100–190** on both engines for these kernels. That's a ratio of two measurements; it is not a per-opcode cost, and it includes everything in the net run path.
- **Allocation (N1):**
	- `numeric` (a `for i in 1..1000001` range loop) performs **1,000,010 net run-path heap allocations on both engines**, one per iteration.
	- The equivalent manual `while` loop performs 7.
	- `strings` performs 220,058 (old) and 220,072 (main).
	- The other kernels perform 7–24.
	- Main's context-plus-compile performs about 2.3k–4.6k more allocations than old's (34.4k–37.3k vs 32.2k–32.7k).

  The per-iteration allocation in range loops is therefore measured. That it's the boxed `Option` returned by the native `RangeIter::next` remains a source-reading explanation, supported by the symbol families in §3.

## 3. Self-sample categories (retired-instruction samples; S2; whole invocations)

Target-process samples only (R2). The full tables and the classified symbol inventory are in `results/interp-profile-0170/step-b-report.md` and `step-b-analysis.json`. Family rules are disjoint and ordered (`analyse.py`), and every target sample is counted, including libc, loader and unknown. Families named "mixed" aren't exclusive to one operation (R3). Intervals are model-based (Wilson, independent samples) and descriptive, not calibrated uncertainty for net cost. Unmapped samples are at most 0.6% (`itoa` formatting and iterator/module closures, in the inventory).

**An important limit:** the categories are symbol-level. Main's codegen moved comparison, integer conversion and value-drop code **out of line** from `Vm::run`. For example, on `while`, `Vm::run` self fell 153M while comparison rose 145M. **Shifts between `Vm::run` and those helper categories therefore can't be read as added work.** Only the net per-workload increase (§2) is robust. Symbol-family shares locate exploratory areas of work, but they don't establish how much each family contributes to the net increase.

Main-vs-old movements in target instruction-sample shares, read with the inlining limit above (descriptive only). Old → main:

| Workload | `Vm::run` self | Comparison helpers | Value clone/drop/dismantle | Rune Stack methods | Integer helpers |
|---|---|---|---|---|---|
| while | 93.1% → 69.8% | 0.0% → 12.9% | 3.3% → 9.6% | 0.0% → 0.0% | 0.9% → 5.0% |
| fib | 86.4% → 60.0% | 0.6% → 11.3% | 0.3% → 10.1% | 0.0% → 0.3% | 0.4% → 0.8% |
| calls | 77.5% → 58.5% | 0.0% → 8.3% | 2.1% → 8.8% | 0.0% → 10.0% | 0.0% → 1.4% |
| numeric | 59.6% → 41.8% | 0.0% → 0.0% | 5.1% → 12.1% | 3.2% → 13.9% | 0.2% → 2.9% |

The comparison and integer families moved largely out of `Vm::run` (an inlining change, not demonstrably added work). The value clone/drop/dismantle family and the rune `Stack` methods (`Stack::copy` and similar) are code paths that main runs where old ran none or less. Their share rises in every compute workload, but this table can't say how much of main's net increase they cause.

**Inside `Vm::run` (self samples by source line, from the line tables; `step-b-vmrun-srclines.txt`):**
- **main, while:** 26.2% of `Vm::run` self samples at `runtime/memory.rs:487` and 22.4% at `core::mem::replace`. Both are inside `Stack::store_with` → `Worklist::replace`; every store moves the old value out and passes it to the dismantle worklist.
- **old, while:** 25.0% at `inst.rs:1214`, `*stack.at_mut(addr)? = o.into_output()?`: the store path again, with drop in place. Another 15.4% is at `vm.rs:1320` (`op_drop` writing `Value::empty()`).
- So **the store-to-slot site is the most-sampled line in `Vm::run` on both engines' `while`.** These sample shares don't establish a cost per store, and line attribution is subject to sampling skid of unknown bound for this event.

## 4. Ranked candidates for W3's first change record (hypotheses with sampled areas)

None is chosen here. The "area" column is the sampled mass where each idea would act: **exploratory, not a proven removable upper bound**, especially where inlining moved code between symbols.

| # | Candidate | Exploratory area (samples) | Engines | Semantics at risk | Differential tests it needs |
|---|---|---|---|---|---|
| C1 | **Store fast path.** When the overwritten slot holds an inline value (nothing to dismantle), write directly, skipping replace, take_repr and dismantle. | Store sites: about 48% of main `Vm::run` self on while (≈34% of while's instructions); about 25% of old's | main (also old's drop-in-place) | drop order and timing of non-inline values (must be unchanged); the dismantle guarantee for nested values | the 0168 corpus; nested-value drop order; deep-literal and recursion robustness; refcount/alias tests; budget halts mid-store |
| C2 | **Inline integer/float fast paths for comparison (`Op`) and `Arithmetic` in dispatch,** before the general `internal_cmp`/protocol path | comparison helpers 8–17% and integer helpers 1–5% of main compute samples (largely moved out of `Vm::run`, so overlapping that symbol's former mass) | main | overflow/error classes; mixed-type comparisons; protocol overrides on non-primitives | the corpus int-boundaries/overflow/floats; mixed-type compare errors; user `PARTIAL_CMP` impls |
| C3 | **`Stack::copy` / frame setup cost on main** (`Copy` opcode, call frames) | rune `Stack` methods: 13.9% (numeric), 10.0% (calls) of main target samples, vs 3.2% and 0.0% on old | main | aliasing of copied values, reference counts | vec/object alias tests; closure captures; call/return corpus |
| C4 | **Range-loop specialization:** `for i in a..b` over integers without a native `next` call and a heap `Option` per step | range-iterator + value construction + call families: ~27% of numeric (old), ~25% (main); measured 1,000,010 net run-path allocations (vs 7 for the `while` equivalent) | both | user iterators and `IterNext` protocol; overflow at range ends; budgets | range edge cases (empty, reversed, i64 bounds); user iterator protocol; break/continue |
| C5 | **Per-instruction `budget::take()`** (main reads budget state every opcode; old acquires once per `run`) | 2.0% of main `Vm::run` self on while (`budget.rs:146`) | main | exact budget enforcement: the contract, so not per-block charging | budget-exhaustion corpus; nested async budgets (the 0032 behaviour) |
| C6 | **Fixed per-opcode overhead** (decode via `instruction_at`, result propagation, `vm.rs:0` code) | Old: 18% (while) to 34% (fib) of `Vm::run` self at `vm.rs:0` | both | none per se; structural | full suites; this is the W3 dispatch redesign, not a first change |

The first change record should be chosen jointly. C1 has the largest sampled area with a local, testable change. C2 and C3 are main-specific. C4 is a large single-workload opportunity, with more semantic surface. No optimization win has been measured for any candidate.

## 5. Limits

- One machine (i7-14700 P-core 4, Linux), one toolchain.
- Self-sample categories are symbol-level and inlining-sensitive (§3).
- DWARF unwinding failed on these binaries, so there's no sample-level phase split. Step D's run − compile is net run-path work, not an exclusive execution phase.
- Sample skid for `cpu_core/instructions/u` has no demonstrated bound here, so line-level attribution is indicative only.
- No timing claims are made here. The deciding metric is instructions (0166 lesson); wall time stays with 0169's runner.
