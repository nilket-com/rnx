# rnx 0178: profile-paired range study

**Status:** plan for Claude's review before any build or new subject execution. The user authorized 0178 as a diagnostic build-profile study. Codex owns the plan, independent review and evidence; Claude owns build/diagnostic/measurement tooling. No engine edit or shipping decision is part of this record.

## 1. Question and frozen matrix

The unrelated regressions in 0175/0176 were located, not explained. Changed code can affect compilation of paths it never executes. 0177's retained-edge analysis did not identify a safe local fast-path transformation; partition/inlining/layout effects remain competing hypotheses.

Question: do range gains survive, and do unrelated regressions disappear, when the same sources are built with one codegen unit? This can establish a profile-dependent result, not causation or stable behavior under arbitrary future source edits.

Exactly two profiles, no later addition or selection:
- **P0:** Cargo's default release profile, opt-level 3, default 16 CGUs, lto=false, panic=unwind, no debug info. lto=false can still perform crate-local ThinLTO; do not call it "no LTO."
- **P1:** P0 with only codegen-units=1. No explicit LTO, opt-level, target-cpu, linker, panic, stripping or RUSTFLAGS change.

Exactly three pinned Rune subjects under each:
- **B:** semantic baseline eaa59fc208c136ead86f8c4fa565431ea18de88b, production equivalent to 3e7d4da9ce908eeb7e0e1ac119dec24f68d5449a.
- **S75:** stopped 0175 candidate 863370da279031b369ebb0ac1c0d80cd9ca159f6.
- **S76:** stopped 0176 candidate 6f54bd32ee1038e3927ef384d1a914d8d5b36c4f.

Deciding comparisons: (P0,B:S75), (P0,B:S76), (P1,B:S75), (P1,B:S76). Each uses fresh baseline samples from that profile, never samples from a different profile. No S75:S76 comparison selects a candidate. Report all four, regardless of outcome. Cross-profile ratios are descriptive only.

No P2: ThinLTO+cgu1 would add a second mechanism and a larger build matrix without resolving the stated single-CGU question. No pinned partition, nightly CGU introspection or further source variant.

## 2. Build and semantic gates

Tooling starts from reviewed 0176 driver/history at bench b66d596ce2301447418c445a00de328db2549fcf, measurement algorithm from 640a792d. Add the profile axis and 0174's profile-setting/build mechanics only. Codex reviews committed tooling and untimed controls before official diagnostic/measurement. No build before this plan is accepted.

One fixed fork worktree path, separately empty equal-length target paths per build, fixed staged executable paths, immutable subjects.json binding every profile/source/build-kind identity. Set only the harness [profile.release] section, so the entire dependency graph uses that profile. Restore/verify the manifest and source after each staged source/profile. No package overrides or hidden config/environment profile settings.

Build 3 sources × 2 profiles × 3 kinds (primary, counter, allocation) = 18 fresh builds. Timing/counting artifacts are never interchanged. Measured Rune features stay alloc,anyhow,fmt,serde,std (no tracing). Keep existing stdio and inventory contracts. Verbose rustc commands for every crate/build script prove the actual CGU/LTO/optimization settings; Cargo default omission is distinguished from explicit flags.

Pin rustc 1.98.1, compiler -vV and Cargo.lock hash to 0176. Fetch locked dependencies once before builds, retaining its separate duration/status; then --locked --offline, CARGO_INCREMENTAL=0, -j8, fixed safe EB, no RUSTFLAGS. Each build deadline is 60 minutes, with explicit group ownership/cleanup and retained failure. Profile/source build order is fixed: P0-B, P1-B, P1-S75, P0-S75, P0-S76, P1-S76; within each primary, counter, allocation. Build cost includes compile/link/copy and is one observation per cell, not a distribution. Report all durations, executable/.text hashes and sizes. No new rnx cargo-install study: 0174's product costs are historical context, not validation of this matrix. A shipping/profile decision needs a separate product record.

Rerun full base/S75/S76 test-profile suites once per source, named non-tracing specialization/fallback and no-std checks before diagnostics. Tests/goldens unchanged. Every primary binary passes the 40 frozen correctness fixtures against P0 B (36 origin-qualified cases + four range/manual-next), using only the two historical normalizations. Tracing builds never specialize.

Counting/budget controls run for every profile/source. Preserve exact outputs/statuses, explicit memory-limit scopes including usize::MAX, zero/default/unlimited/tight budget controls, raw-boundary golden behavior, custom handlers, unit shadows, positive hits, terminal and manual-next Options. Context's +1 temporary tag registration (floor/empty zero) remains an exact qualification. Allocation counts may not be pooled across profiles. Inventory golden coverage is test-profile evidence; release-mode registration invariance remains an explicitly untested assumption, not a stronger certificate.

## 3. Reviewed pre-check diagnostic

After semantic/build/control gates, run exactly once for each (profile,subject) on context, numeric, fib, calls: 24 Callgrind runs. Use the reviewed 0176 lifecycle/retention tooling, Valgrind 3.26.0, E0 plus required VALGRIND_LIB, shared lock, bounded process groups. Bind profiles/sources/binaries explicitly in every result. Retain raw Ir, caller edges, inclusive/exclusive tables, full nm and run/op_call/helper disassembly, command versions/argv/statuses and partial failures before parsing/checks.

Replay 0177's parser fixtures and caller analysis per profile, retaining all callers without a display threshold. Inspect pop_call_frame -> Repr/Value drop and BTreeMap::entry_with -> infallible_cmp<Component>; compare B to both S75 and S76. Preserve profile-local identities and ambiguity among same-named nm symbols; caller names alone do not identify unique addresses. Ir is instrumented events, not instructions:u; inclusive edge costs cannot be added to exclusive costs.

A changed/unchanged/absent edge is a diagnostic result, not a gate requiring that P1 look better. All valid profiles proceed to the frozen matrix. No source, profile, target or gate changes follow viewing the diagnostic. Tool execution/identity/retention failures STOP before deciding measurements and require reviewed repair; they are never interpreted as an absent edge. No additional perf attribution or CGU dump.

## 4. Deciding comparisons and reproduction boundary

Use the unchanged 0176 workload list: 18 historical workloads + signed-range, negative-range and range-while. For EACH of the four pairs:
- Native PMU: five true ABBA repetitions, ten samples per side/workload, instructions:u/cycles, core4, at least99% running; every raw record retained before checks.
- Wall: pinned 0169 resident observer, fresh pipe-captured process, three ABBA/BAAB/ABBA rounds of five-sample blocks, 30 per side/workload. No added workload warmups, time offsets or process-floor subtraction.
- Hyperfine -N --output=pipe calibration first, within0.15ms. Representative inherited-affinity and process lifecycle controls before and after; per-sample controller affinity is not relabelled as observed child affinity.
- Correctness, allocation, budget statuses, temporary registration qualification, sentinel, retention and safety gates unchanged.

Fixed pair order: P0:S75, P1:S75, P1:S76, P0:S76. Run complete pair data even when its scientific regression decision is STOP; STOP does not justify omitting later matrix cells. Infrastructure/semantic failure stops the whole run and retains partial cells. Total deciding PMU samples =1680; wall blocks =1008 (5040 timed child processes).

**Historical reproduction scope, explicit review point:** apply the frozen applicable 2% historical reproduction gates to the fresh P0 baseline only, plus the original descriptive tiny FIFO windows. P1 deliberately changes the profile, so applying default-profile historical instruction totals to P1 would test the wrong baseline. P1's references are its own fresh verified B artifacts and samples; report its cross-profile baseline shifts descriptively without calling them reproduced default-profile results. No post-result reference, adjusted tolerance or cross-profile subtraction. Within-pair regression thresholds and decision precedence remain identical. P0 artifact reproducibility against 7a66b042 (B), ec6962fb (S75), 3461f953 (S76) is reported separately; source/profile/build receipts, not a guessed binary match, establish P1 identity.

Lua references remain descriptive whole-process workload windows under the inherited protocol. Do not use them to rank profiles or attribute startup. Cross-profile medians can be compared descriptively from fresh measurements, but pair schedules differ and are not a separately randomized cross-profile causal experiment.

## 5. Frozen outcome matrix

Apply verbatim and separately to all four pairs:
1. Correctness, safety, calibration, applicable historical reproduction or retention failure: **STOP**.
2. Any workload's instruction median regression >0.5%, wall median worsening beyond its own base p10–p90 width, or opposing instruction/wall changes both >3% outside those bands: **STOP**.
3. Otherwise **WIN** requires at least10% fewer instructions on original numeric AND both admitted range controls, plus at least90% fewer numeric allocation calls, with every unrelated workload passing its gate.
4. Otherwise **NO-WIN**.

Each wall band comes from that pair's same-profile fresh baseline. No average across profiles/candidates/workloads overrides a failed workload. Report full samples, medians and every triggering gate, allocation qualification, disagreements and cell decision.

A P1 passing cell is **diagnostic WIN**, not adoption, retrospective reversal of 0175/0176's default-profile STOPs, proof of partitioning causation or a shipping recommendation. If no cell passes, report that negative result without another variant. If P1 passes but its baseline loops/build costs worsen, report both facts together. The range approach remains parked pending a separately reviewed source/profile adoption decision; this study changes neither current fork main nor shipped Rune0.14.2. No kill criterion is bypassed by changing a label.

## 6. Security, lock schedule and integration

Claude owns isolated driver worktree/branch and reports the build/measurement lock schedule before starting. All heavy compilation/tests/diagnostics/measurements hold /tmp/rnx-runtime-bench.lock. Stage boundaries (suite/build preparation, diagnostic, official matrix) may release the lock only at explicit completed boundaries with verified artifacts; no measurement includes another heavy job. Codex does read-only review while Claude holds it.

Replay fake-secret controls on success/failure/ledger/archive paths before official work. Subjects use explicit E0; builds fixed EB. Never serialize inherited environment. Known exposed credential checks run in memory and print paths/counts only. Rotation remains separately unconfirmed until independently confirmed; a zero artifact scan is not proof of rotation.

Review tooling/rehearsal receipts before the official run, which is once only. Failures retain raw/partial data and stop; any plumbing repair/replay requires prior review and disclosure with source/profile/gates unchanged. Report every attempt, outer launch/status, effective environment and lifecycle gate.

Codex independently reconstructs all four decisions from retained raw records and reviews diagnostic identity/attribution limits. Generic rnx plan/evidence plus bench probes/results only, preserving author commits with serial integration; no engine edit or new fork candidate branch. Bench subjects use "probes: rnx 0178 ...". On acceptance publish the diagnostic, not a shipping profile. Public upstream action and deployment are out of scope.
