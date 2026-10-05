# rnx 0173: build-perturbation diagnostic

**Status:** plan, by Claude, for Codex's review before any edit or measurement. Claude implements; Codex reviews. This is a **diagnostic**: it changes no product code, merges nothing into the fork's main, and produces no acceptance rule. Any later change to a regression gate, or any re-evaluation of 0171/0172 (both STOP), needs its own pre-registered, reviewed record.

## 1. Question

0171 and 0172 were each stopped by instruction shifts under 1% on workloads outside the code they changed. Neither record can tell whether such shifts come from:
- (A) rebuilding the same source;
- (B) how the same binary is launched;
- (C) semantically equivalent but different code producing a different binary.

This record measures those three **separately** and doesn't pool them into one "noise" figure.

## 2. Subjects

The 0169 harness (exact sources pinned to rnx-bench `fb56b1d`, as in 0172), built against the fork at `bb8e69372353c50e271c9f115bc771c77aa6b83e` and against edits of it. Workloads and fixtures are 0172's, with the same expected outputs; the whole-process PMU method; the shared lock; the controller and children pinned to {4} with affinity asserted; raw perf JSON retained for **every** sample (0172's retention limitation fixed).

### A. Same-source rebuild reproducibility

Build the base harness 3 times from clean, in 3 separate target directories of **equal path length**.
- Report whether the binaries are byte-identical (`sha256`, and `.text` hash).
- If they are, A is answered by identity, and no PMU run is needed for it beyond one binary.
- If they differ, measure each rebuild on every workload and report the per-workload instruction differences between rebuilds.

### B. Launch sensitivity (one binary)

The byte-identical base binary, launched in pre-registered conditions:
- (B1) from 3 copies at paths of equal length;
- (B2) from paths of different lengths (shorter by 5, longer by 20 and longer by 60 characters);
- (B3) with the environment varied: the current one, `env -i` plus a minimal fixed PATH, and the current one plus one 1 KiB variable.

Every workload, whole-process instructions:u, with the per-condition medians and full sample lists reported.

### C. Semantically equivalent different binaries

Fork edits, each a separate commit on a diagnostic branch `w-0173-null-edits` from `bb8e6937`, never merged:

- **C0 (control):** a comment-only change. Expected to produce a byte-identical binary; if it doesn't, that's reported.
- **C1:** an unused private `fn` added to `compile/context.rs`. Report whether it compiles away (binary unchanged).
- **C2:** two adjacent private methods of `impl Context` swapped in source order.
- **C3:** a private helper renamed (its definition and call sites only).
- **C4:** an unused `#[inline(never)] pub(crate) fn` in `runtime/vm.rs`, referenced only from a `#[cfg(test)]` test, so it must survive in test builds but not necessarily in release.

**For each edit:**
1. The fork suite passes, the 0172 registration-inventory golden matches exactly, and the 0172 corpus plus fixtures are identical to the base (strict `same()` rules).
2. Binary identity is reported (`sha256` and `.text` hash). An edit that leaves the binary byte-identical is reported as such and isn't measured further.
3. For edits that change the binary: every workload, whole-process instructions:u, **5 ABBA repetitions** (A B B A per repetition, as the 0172 plan intended), base vs edit. Paths are equal length; the launch condition is fixed to B1's first path.
4. Attribution, where practical: for any workload whose median moves by more than 0.3%, an S2 self-sample symbol diff (0170/0171 method, line tables, no frame pointers, period 1e6, ≥10k target samples per side) and an `objdump` diff of the most-moved symbols. The 0.3% threshold only selects what gets inspected; it isn't a gate.

## 3. Reporting rules

- **Results by category:** each of A, B and C is reported separately, as per-workload medians, full sample lists and differences.
- **No combining:** no statistical noise interval is built from mixing categories.
- **No tolerance:** no tolerance is derived or applied.
- **The discussion may say whether 0171/0172-sized shifts (±0.2–0.7% on VM workloads, about 2–4% on startup) are within the spread of (C).** It may not re-label either record or propose a new gate. Any gate proposal comes as a separate plan.
- **Stated limitations:** one machine, one toolchain, a small set of null edits. The results don't generalize beyond the edits measured.

## 4. Gates and closure

- **STOP conditions:**
	- a correctness gate fails for any edit (the suite, the inventory, or the corpus);
	- an edit that should be semantically null changes behaviour (that's a finding, and that edit is excluded);
	- a measurement fails (affinity, running below 99%, output mismatch, missing raw record).
- **Closure:** rnx plan and impl commits, an rnx-bench `probes:` commit, and the fork diagnostic branch pushed as a named non-main branch for the record, once Codex accepts.

## 5. Amendments from Codex's plan review (in force; they replace the matching text above)

### 5.1 Frozen null edits

Every edit is a **sibling** commit on the diagnostic branch. Each derives independently from the same parent, `3e7d4da9ce908eeb7e0e1ac119dec24f68d5449a` (base `bb8e6937` plus the tests-only inventory fixture from 0172). They don't accumulate. Each sibling's hash is recorded. Byte-identical binaries are reported as a result in their own right; no edit is swapped for a more disruptive one after the fact.

| ID | File | Exact edit |
|---|---|---|
| C0 | `crates/rune/src/runtime/memory.rs` | a comment line `// rnx 0173 control: comment-only change.` inserted immediately above `pub(crate) fn store_with<O>(` |
| C1 | `crates/rune/src/compile/context.rs` | appended at the end of the file (before the `#[cfg(test)] mod inventory_tests`): `#[allow(dead_code)]` `fn rnx_0173_unused(x: u64) -> u64 { x.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x0173 }` |
| C2 | `crates/rune/src/compile/context.rs` | the two adjacent private methods `Context::install_reexport` and `Context::install_construct` (each with its doc comment) swapped in source order; bodies unchanged |
| C3 | `crates/rune/src/compile/context.rs` | the private method `Context::install_type_info` renamed `Context::register_type_info`: its definition and its two call sites (in `install_type` and in its variant loop) |
| C4 | `crates/rune/src/runtime/vm.rs` | appended: `#[allow(dead_code)]` `#[inline(never)]` `pub(crate) fn rnx_0173_probe(x: usize) -> usize { x.rotate_left(7) ^ 0x0173 }`, plus `#[cfg(test)] mod rnx_0173_tests { #[test] fn probe() { assert_eq!(super::rnx_0173_probe(1), 1usize.rotate_left(7) ^ 0x0173); } }` |

### 5.2 Sample counts, order and conditions

Fixed now. Every comparison uses **5 ABBA repetitions**: per repetition A, B, B, A, so 10 fresh processes per subject per condition per workload.

Each sample records:
- its raw perf JSON, including the running percentage;
- the actual argv, environment, cwd and binary hash;
- the child's affinity;
- its stdout.

Launches are direct `execve` from Python, with `perf stat` as the only wrapper, which is the same for every sample. No shell and no `env` command is in the counted subject.

**A.** A0, A1 and A2 are three clean builds from the parent's source (including the cfg(test) inventory test), in target directories `a0`, `a1` and `a2` under the probe (equal length). If they aren't byte-identical, each of A1 and A2 is compared with A0 using the staging scheme in 5.3.

**B.** The A0 binary only.
- **Paths:** B0 is `<probe>/launch/p00/primary`.
	- B1 and B2 are `<probe>/launch/p01/primary` and `p02` (equal length).
	- B3 is shorter by 5 characters: `<probe>/l/p00/primary`. (Corrected on review: `<probe>/l/p/primary` would be 7 shorter.)
	- B4 is longer by 20 and B5 longer by 60: `<probe>/launch/p00/` plus a padding directory of `x` characters, whose length including its slash is exactly 20 or 60.
	- The full-path byte-length deltas against B0 (0, 0, −5, +20, +60) are asserted before any measurement.
- **Environments:**
	- E0 (the fixed environment for every path comparison) is exactly `{PATH: "/usr/bin:/bin", HOME: "/home/me", LANG: "C.UTF-8"}`.
	- E1 is E0 plus `RNX0173_PAD` = 1,024 × `"x"`.
	- E2 is the controller's full inherited environment, captured **once** before measurement and reused unchanged for every E2 sample.

	Environment comparisons use the fixed path B0. One dimension varies at a time.

### 5.3 Staging for A and C

Distinct executables are compared at **one fixed path**, `<probe>/stage/primary`.
- Before each sample, outside measurement, the chosen binary is copied to a temporary name in `stage/` and renamed into place atomically.
- Its sha256 is verified at the fixed path, and again after the sample.
- The sample's identity (subject id, hash before and after) is recorded.

So argv[0], the executable path and the environment (E0) are identical between subjects. B's intentional path changes are never mixed into A or C.

### 5.4 S2 attribution

- **0170's representativeness gate:** a line-table build of the same source must pass differential correctness, and its instruction counts must reproduce the corresponding measured release binary within **2%** on the workloads being attributed. Otherwise attribution is reported as unavailable; no replacement flags or toolchain without review.
- **Sampling budget:** at most 3 sampling attempts of 10 minutes each per profile. Fewer than 10k target samples means attribution is unavailable.
- **Reporting:** whole-process instruction deltas are reported separately from per-symbol sample proportions. Neither an objdump nor a sampled-symbol difference alone explains a measured delta. "File hash differs" and "`.text` differs" are reported separately.

### 5.5 Correctness

A correctness failure (suite, inventory or corpus) halts that candidate, and it's retained and reported. Nothing is silently replaced.

The results are evidence about these specific rebuilds, launches and null edits, not a measured universal noise floor. 0171 and 0172 aren't reopened.

At closure each C sibling keeps its own named ref (`w-0173-c0` … `w-0173-c4`), pushed to the fork so that every sibling commit stays reachable.
