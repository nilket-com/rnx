# rnx 0171: store fast-path experiment on the engine-development base

**Status:** plan, by Claude, for Codex's review. This is W3's first change, candidate C1 from 0170. Claude implements; Codex reviews and reruns the gates independently. It's an **experiment**: a change counts as a win only under the protocol below, which is fixed before any measurement. It changes no rnx dependency, no shipped behaviour and no budget contract. The fork change lives on a fork branch; whether it goes into the fork's main is decided at closure, by both of us.

## 1. Hypothesis

0170 found the store-to-slot site to be the most-sampled line in `Vm::run` on main's compute workloads: `runtime/memory.rs:487`, `Stack::store_with` → `Worklist::replace`, plus `core::mem::replace` inlined there. Every store moves the old value out of the slot and passes it to the dismantle worklist, even when the old value is inline (nothing inside to take apart).

**Hypothesis H1:** when the overwritten slot holds an inline value, writing the new value directly skips work without changing any observable behaviour, and reduces retired instructions on store-heavy workloads.

0170 doesn't establish the size of any saving. This record measures it.

## 2. The change

- **Fork branch** `w3-0171-store-fast-path` from `bb8e69372353c50e271c9f115bc771c77aa6b83e`, in an isolated worktree of `~/work/rune`.
- **One focused commit.** In `Stack::store_with`, after `out.as_addr()` and computing the new value: if the existing slot `is_inline()`, assign directly (dropping an inline value runs no destructor and can't recurse); otherwise keep `work.replace(slot, value)` exactly as now.
- **Other store routes:** check, by reading the code, whether `Stack::store` (which builds a fresh `Worklist::new()` per call) and other routes into `Worklist::replace` sit on hot paths. If one does, the same guard applies there, and this plan's review covers it only if it's listed in the impl evidence with its own controls. Nothing else changes: no dispatch, opcode, value-representation or budget changes.
- **Error and ordering behaviour:** `into_output()` errors, address errors and the moment the new value is produced stay exactly as before. The new value is still produced before the old one is dropped, as now.

## 3. Correctness gates (before any measurement)

1. **The fork's own test suite** on the branch: `cargo test -p rune` with the features the fork's CI uses, including the dismantle tests (`runtime/value/dismantle.rs` counters and levels). It must pass exactly as on the unmodified base, which is run first, with any pre-existing failures listed.
2. **The 0168 differential corpus** (26 fixtures) and **0170's kernels**: candidate vs unmodified main. Identical status, stdout and stderr after normalizing only the two known nondeterministic fields; the 0170 `same()` rules and controls are reused.
3. **New targeted fixtures,** each run on the unmodified base and the candidate, outputs compared, plus independent oracles where they apply:
	- **Deep values in a slot:** overwrite a variable that holds a nested vector/object of depth 10k, 100k and 1M with an integer, and the reverse. No stack overflow, and the same result as the base. This proves the non-inline path still dismantles.
	- **Aliasing:** two variables sharing one vector or object, one overwritten, the other then read and mutated. Also closures capturing a slot that's then overwritten.
	- **Drop order, observable:** native types with destructors aren't observable from pure Rune. The fork's Rust tests and any existing drop-order unit tests cover them. Candidate and base run the same native-backed object fixtures (e.g. `Bytes`, `String` and object values overwritten in loops) under the allocation-counting harness, with identical allocation and deallocation counts.
	- **Budget:** budget exhaustion in a store-heavy loop at budgets 1, 2, 3, 10, 1,000 and 1,000,000. The same halt point (the same output before halting) on base and candidate.
	- **Inline-to-inline, inline-to-non-inline, non-inline-to-inline and non-inline-to-non-inline overwrites,** in one fixture, with exact outputs.
4. **Fail-closed comparison:** any difference in any gate stops the record. The change is diagnosed or abandoned, never waived.

## 4. Measurement protocol (pre-registered)

- **Subjects:** the 0168 harness built twice from the same harness source and lock. *base* uses fork main `bb8e6937` (the 0168 `new/primary` binary itself, hash-checked). *candidate* uses the branch commit, with an identical release profile and features. Both builds' hashes are recorded.
- **Workloads:** the 0170 set (empty, answer, numeric, while, fib, calls, compare, vector, strings), plus the four store-overwrite fixtures from §3 sized for timing.
- **Primary metric:** whole-process instructions:u, 5 repeats per subject × workload, interleaved base/candidate, pinned to core 4, under the shared lock (0170's `counts.py` method). The pcnt-running ≥ 99 check applies.
- **Secondary metric:** wall time with the native clock, interleaved ABBA, 3 rounds × 10 samples per workload. It's reported, but it's not the decision metric (0166 lesson). If 0169's standing runner has closed by then, it's used instead.
- **Decision rule, fixed now:**
	- **WIN** if (a) every correctness gate passes; (b) median instructions:u falls by **at least 3.0%** on at least two of while, fib and calls; (c) **no workload regresses by more than 0.5%** in median instructions:u (empty and answer included); and (d) no workload's median wall time is worse by more than its own base p10–p90 spread.
	- **NO-WIN** if the gates pass but (b) doesn't hold. The change isn't merged; the evidence is retained.
	- **STOP** if any regression rule fails, or wall time and instructions disagree in direction by more than 3% on a workload. The record reports it and doesn't merge.
- **Attribution check,** descriptive only: a before/after S2 self-sample of `while` (0170 method, period 1e6, ≥10k target samples). It should show the store-site samples falling. It can't turn a NO-WIN into a WIN.

## 5. Closure

- **On WIN:** the fork branch commit is offered to fork main as a fast-forward after both reviews. rnx is unchanged; the W4 port record will pick it up. An upstream-PR draft goes in `plans/0171_upstream_*.md`, unfiled (the user's call).
- **On NO-WIN or STOP:** the branch is kept unmerged, and the evidence records why.
- **Commits:** rnx plan and impl, and an rnx-bench `probes:` commit. Both are integrated serially with explicit parents after review. Neither of us amends the other's commits.

## 6. Amendments from Codex's plan review (in force; they replace the matching text above)

- **A1 (scope):** the experiment changes **only `Stack::store_with`**. `Stack::store` already forwards there. The other `Worklist::replace` callers are audited and listed in the evidence, but none is touched. Touching another route needs a follow-up amendment, reviewed before any measurement.
- **A2 (direct drop and order controls):** in-crate Rust tests on the changed path, with a native `DropSpy` type that logs construction, conversion and drop events. They prove:
	- `IntoOutput` runs before the slot is accessed or dropped;
	- an `IntoOutput` error leaves the old slot intact;
	- an address error drops the new temporary exactly as before;
	- non-inline old values drop in the same order.

  This covers all four inline/non-inline combinations through actual `Stack::store_with` calls, plus deep and aliased values. The tests land in a **separate first commit on the base**, so base and candidate run the identical tests; the fast path is the second commit. Equal allocation totals aren't treated as proof of order.
- **A3 (budget halt state):** for budgets 1, 2, 3, 10, 1e3 and 1e6, an in-crate test compares the halted VM's instruction position and its observable stack and native side-effect state, as well as status and output, between base and candidate. A deliberately shifted halt-state control must fail. Exact per-instruction budget semantics are kept.
- **A4 (one frozen measurement path):** the measurement is §4's protocol only, with no switch to 0169's runner:
	- 5 interleaved whole-process PMU repetitions, plus the ABBA native wall clock;
	- complete outputs, positive finite counts, ≥99% running, bounded jobs, and the shared lock;
	- both fresh builds bound to the same harness source, release configuration, features and dependency versions;
	- the base build must reproduce the retained 0168 base counts within 2% before any candidate change is attributed.

  **The disagreement STOP, defined now:** wall time and instructions change in opposite directions on a workload, with both absolute median changes above 3%. Changes inside a workload's frozen base noise band (its base p10–p90 for wall time, 0.5% for instructions) are excluded from that test, decided now and not after the fact. **Decision precedence:**
	1. a correctness or measurement failure means STOP;
	2. then a regression (0.5% instructions, unchanged) or a disagreement means STOP;
	3. then WIN or NO-WIN.
