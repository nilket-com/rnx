# rnx 0174 evidence: build-profile study

**Status:** implementation by Claude, for Codex's review. Descriptive only: no gate, no tolerance, no re-classification of 0171 or 0172 (both stay STOP), no profile proposal. The probe is `probes/build-profile-0174` in rnx-bench, with results in `results/build-profile-0174/run1`. The tooling was committed before any official run. Every job ran under `/tmp/rnx-runtime-bench.lock`:
- fork suites, harness builds and rnx installs: one locked chain, 16:10–16:47 CDT;
- Codex's 0175 test compile in the gap after it (no overlap);
- the official measurement, 7.5 minutes, load 0.65 → 1.20.

## 1. Subjects and correctness

- **Sources:** all three share the tests-only parent `3e7d4da9`.
	- base: `3e7d4da9`;
	- S71: `30c53555`, branch `w-0174-s71`;
	- S72: `7e748d02`, branch `w-0174-s72`.
- **Build method:** one fork worktree path, one harness path, equal-length target directories, `-j 8`, `--locked --offline`, `CARGO_INCREMENTAL=0`. Builds ran on the full CPU set, recorded per build; only the measurements were pinned.
- **Fork suites** (`cargo test -p rune --all-targets --all-features`): base, S71 and S72 each 597/597.
- **Corpus:** all 11 harness binaries against base P0, 36 origin-qualified fixtures each, environment E0, strict `same()` after the two normalizations: **396/396 identical**. All four installed rnx binaries print `42` for `rnx eval 42`.
- **Reproducibility across records:** base P0 is byte-identical to 0173's A0 (`7a66b042e865…`, `.text` `3f829d83686e…`). Every other binary differs, and all hashes are in `build/build.json` and `measure/measure.json`.
- **Release-profile inventory:** still an **untested assumption**. The inventory golden runs in the test profile only.

## 2. Build cost

Clean builds, with a single `cargo fetch --locked` beforehand. That fetch took 0.10 s for the harness and 0.13 s for rnx because every crate was already in the local Cargo cache. **The cost of a cold download was not measured.**

| Profile | Harness clean build (3 runs, balanced order) | Harness binary | rnx `cargo install`, rnx 785557a, 1 run | rnx binary |
|---|---|---|---|---|
| P0 default release | 37.3 / 37.0 / 37.0 s | 9,007,272 B | 49.9 s | 20,360,568 B |
| P1 cgu 1 | 78.6 / 77.8 / 78.1 s | 6,847,416 B (−24%) | 81.5 s | 16,611,584 B (−18%) |
| P2 thin LTO, cgu 1 | 84.8 / 83.9 / 84.0 s | 6,456,944 B (−28%) | 88.7 s | 16,820,048 B (−17%) |
| P3 fat LTO, cgu 1 | 86.8 / 86.1 / 85.6 s | 6,193,776 B (−31%) | 125.7 s | 15,714,920 B (−23%) |

`cargo build -v` and `cargo install -v` logs record every rustc invocation, including dependencies and build scripts. The actual linker command line is **not observable** in those logs and is reported as unavailable.

## 3. Measurements

- **Instructions:** whole-process `perf stat` instructions:u and cycles:u, 5 true ABBA repetitions per workload, so 10 per side. Every raw perf JSON is in `measure/pmu-*.jsonl`.
- **Wall clock:** the 0169 resident driver, one process per 5-sample block, 3 rounds (ABBA/BAAB/ABBA), 30 samples per side. Its calibration against `hyperfine -N --output=pipe` was −0.000 ms on `/bin/true` and −0.005 ms on floor, against the 0.15 ms limit. The harness wall clock had no warm-up; `rnx eval 42` had 5 per side.
- **Launch:** every subject ran from one staged path per family, hash-verified before and after every sample or block, with environment E0 exactly. The controller was pinned to CPU 4 and asserted before every sample.
- **Affinity controls:** 5 before and 5 after, each printing exactly `Cpus_allowed_list:\t4\n`.
- **Sentinel:** the rehearsal (results, a failing command, the ledger, an `.xz` archive and the command log) found 0 occurrences, and the final scan of the 20 official files found 0.

### (i) instructions:u, base P0 → P1 / P2 / P3 (median of 10 per side), and wall (median of 30 per side)

| Workload | P0 instructions¹ | P1 | P2 | P3 | P0 wall ms¹ | P1 wall | P2 wall | P3 wall |
|---|---|---|---|---|---|---|---|---|
| floor | 450,178 | -5.41% | -6.14% | -4.02% | 0.500 | +2.63% | -7.09% | -8.17% |
| empty-context | 450,730 | -5.29% | -6.17% | -4.13% | 0.521 | -3.82% | -5.17% | -7.53% |
| context | 26,868,526 | -1.21% | -8.41% | -9.21% | 3.715 | -4.24% | -9.04% | -9.27% |
| runtime | 28,243,972 | -1.19% | -8.53% | -9.14% | 3.978 | -5.35% | -8.97% | -10.21% |
| compile-answer | 29,898,582 | -1.14% | -8.61% | -9.06% | 4.361 | -5.59% | -10.25% | -9.82% |
| run-answer | 29,905,570 | -1.14% | -8.61% | -9.06% | 4.391 | -6.12% | -9.90% | -10.99% |
| run-empty | 28,389,226 | -1.16% | -8.51% | -9.13% | 4.060 | -6.12% | -10.09% | -10.65% |
| run-numeric | 1,759,086,772 | +3.22% | -2.71% | -2.03% | 113.100 | -9.02% | -15.81% | -15.84% |
| run-fib | 833,591,432 | +11.02% | +9.33% | +10.04% | 52.006 | +13.68% | +15.52% | +14.62% |
| run-strings | 227,835,016 | -0.95% | -8.37% | -9.55% | 18.532 | -5.83% | -15.17% | -16.61% |
| run-while | 1,143,104,486 | +10.91% | +8.35% | +9.73% | 69.415 | +4.75% | +3.13% | +5.95% |
| run-compare | 1,722,541,672 | +11.44% | +8.83% | +10.29% | 106.484 | +4.04% | +4.63% | +4.69% |
| run-calls | 1,780,162,642 | +8.80% | +6.99% | +7.94% | 94.753 | +13.98% | +14.41% | +16.16% |
| run-vector | 1,314,402,856 | +8.47% | +5.73% | +6.87% | 84.213 | +1.80% | +0.88% | +0.07% |
| run-overwrite_inline | 1,142,212,978 | +3.65% | +1.00% | +2.47% | 68.387 | +3.15% | +1.75% | +2.92% |
| run-overwrite_mixed | 748,091,678 | +3.36% | -2.19% | -1.57% | 48.126 | -8.46% | -10.62% | -9.89% |
| run-overwrite_deep | 254,881,164 | +2.81% | -2.59% | -2.18% | 21.622 | -4.63% | -7.91% | -7.73% |
| run-overwrite_alias | 30,332,530 | -1.15% | -8.57% | -9.03% | 4.403 | -5.50% | -9.74% | -10.42% |
| **rnx eval 42** (installed rnx, Rune 0.14.2) | 28,752,878 | -2.27% | -9.78% | -10.25% | 4.178 | -2.48% | -6.65% | -7.61% |

¹ The P0 columns show P0's medians from the P0-vs-P1 comparison. P0 was measured again in each comparison, and every percentage is relative to the P0 measured in that same comparison.

### (ii) instructions:u change, base → subject, within each profile

| Workload | S71 P0 | S71 P1 | S71 P2 | S71 P3 | S72 P0 | S72 P1 | S72 P2 | S72 P3 |
|---|---|---|---|---|---|---|---|---|
| floor | +0.07% | -0.11% | -0.03% | +0.00% | -0.00% | +0.59% | -0.01% | -0.09% |
| empty-context | +0.05% | -0.01% | +0.04% | -0.01% | -0.05% | +0.51% | -0.12% | +0.03% |
| context | +2.09% | -0.00% | +0.00% | +0.00% | -3.62% | -3.45% | -2.94% | -2.95% |
| runtime | +1.80% | +0.00% | -0.00% | -0.00% | -3.45% | -3.28% | -2.80% | -2.81% |
| compile-answer | +1.54% | +0.00% | -0.00% | -0.00% | -3.25% | -3.09% | -2.64% | -2.64% |
| run-answer | +1.54% | -0.00% | +0.00% | -0.00% | -3.25% | -3.09% | -2.64% | -2.64% |
| run-empty | +1.80% | -0.00% | +0.00% | -0.00% | -3.42% | -3.26% | -2.78% | -2.79% |
| run-numeric | -2.76% | -4.13% | -4.62% | -4.24% | -0.17% | -0.05% | -0.04% | -0.04% |
| run-fib | -3.76% | -9.68% | -10.32% | -9.84% | -0.42% | -0.10% | -0.08% | -0.08% |
| run-strings | -1.56% | -1.05% | -1.17% | -1.09% | -0.48% | -0.40% | -0.35% | -0.35% |
| run-while | -5.12% | -12.46% | -13.48% | -12.52% | -0.70% | -0.07% | -0.06% | -0.06% |
| run-compare | -4.60% | -12.48% | -13.47% | -12.51% | -0.81% | -0.05% | -0.04% | -0.04% |
| run-calls | -3.40% | -7.69% | -8.19% | -7.75% | -0.45% | -0.05% | -0.04% | -0.04% |
| run-vector | -2.37% | -7.48% | -8.08% | -7.51% | -0.51% | -0.06% | -0.05% | -0.05% |
| run-overwrite_inline | -4.86% | -6.00% | -6.41% | -5.98% | -0.52% | -0.08% | -0.06% | -0.06% |
| run-overwrite_mixed | -1.82% | -5.47% | -6.07% | -5.66% | -0.37% | -0.12% | -0.10% | -0.10% |
| run-overwrite_deep | -0.02% | -2.63% | -2.86% | -2.69% | -0.54% | -0.35% | -0.29% | -0.29% |
| run-overwrite_alias | +1.51% | -0.00% | -0.00% | -0.00% | -3.20% | -3.05% | -2.60% | -2.60% |

## 4. Reading (descriptive)

**Profile effect on the engine (i).** No profile is better everywhere.

- **Startup gains:** all three single-unit profiles cut startup instructions. Thin or fat LTO cuts them by 8.4–9.2% (context, runtime, compile, run-answer and run-empty) and wall time by 9–11%.
- **Product level:** the installed `rnx eval 42` falls by 9.8% (P2) and 10.3% (P3) in instructions, and by 6.7% and 7.6% in wall time (4.18 ms at P0).
- **Interpreter-loop regressions:** the tight interpreter-loop workloads (fib, while, compare, calls, vector) gain **+5.7% to +11.4% instructions under every single-unit profile**, and fib and calls take 13.7–16.2% longer in wall time.
- **Mixed workloads:** numeric, strings, overwrite_mixed and overwrite_deep get faster in wall time.

Collapsing to one codegen unit evidently changes how the dispatch loop is compiled. The mechanism is not shown here; there's no assembly evidence.

**0171's change (S71, `store_with` fast path).**
- At P0 it reproduces 0171's startup regression: +1.5% to +2.1% on context, runtime, compile and run-answer, where 0171 found +1.5% to +1.8%.
- Under P1, P2 and P3 the same startup workloads move by at most ±0.11%.
- Its VM gains grow from −2% to −5% at P0 to **−7.5% to −13.5%** on while, compare, fib, calls and vector.

So the startup side effect that stopped 0171 is **associated with the default 16-unit partitioning**. The cause is unproven.

**0172's change (S72, borrowed ContextType).**
- Its intended effect is stable across profiles: context −3.6% (P0) and −2.9% to −3.5% (P1–P3).
- On VM workloads at P0 it measures −0.17% to −0.81% here, where 0172 measured +0.53% (while) and +0.70% (compare). **The sign is not reproduced.** 0172 built each binary in its own worktree, so embedded paths differed; this record used 0173's single-worktree method.
- Under P1–P3 every VM workload moves by at most 0.40%. The largest S72 shift outside context-dominated workloads is P1 floor and empty-context at +0.59% and +0.51%, on ~450k-instruction windows.

This record doesn't re-label 0171 or 0172 and proposes no gate. Shipping a profile, or re-evaluating either change, needs its own pre-registered record, and a profile change also needs the user's input because it changes `cargo install rnx` time.

## 5. Departures and disclosures

- **Counter builds dropped:** plan §2's four `counter` builds were not made, because no §4 or §8 measurement uses them.
- **Two failed attempts before the official run,** both retained as `measure-attempt1` and `measure-attempt2`; no measurement was taken in either:
	1. The scanner's positive control expected one occurrence in an `.xz` stream. Random hex is incompressible, so it also appears verbatim inside the compressed bytes. The control now requires the decompressed copy to add exactly one occurrence.
	2. A `BIN[...]` indexing bug in the rehearsal and correctness code.
- **Smoke run:** after the fixes, a 2-workload smoke run of the whole pipeline ran in scratch space under the lock. Its output is not retained and is not evidence.
- **Tooling amended:** the tooling commit was amended twice, `358b0ec7` → `59240eb3` → `e0223549`, before any push. The official run used `e0223549`.
- **No per-sample child affinity:** affinity per sample is the controller's, as accepted in plan §8.4; child affinity was not observed per sample.
- **Single observations:** each rnx install is one observation; the environment is one machine and one toolchain (rustc 1.98.1).
