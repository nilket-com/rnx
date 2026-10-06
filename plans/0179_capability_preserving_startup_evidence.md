# 0179: capability-preserving startup — STOP

The single frozen P0 experiment is **STOP**. Constructing a complete default
context used 7.24% fewer whole-process instructions, and compile/run/full cold
first-use paths also improved in instruction count. The context and run-answer
WIN legs fell short of 10%. Six regression triggers took precedence: Fibonacci
instructions and wall time, plus calls, numeric, signed-range and negative-range
wall time. Nothing is adopted: the fork candidate remains unmerged, fork main
stays `bb8e69372353c50e271c9f115bc771c77aa6b83e`, and rnx still ships Rune 0.14.2.

## Scope and frozen identities

Accepted plan `a2bea93`; revised read-only feasibility `5c9847d`; tests-first
checkpoint `4b3ff0d`. Rune base `4e84cfac` follows the reused 0172 inventory
commit `58ba84f1` on fork main; candidate `b28f8cb3` adds only a private direct
name builder/fallback pair and one associated Function alias registration
call site in `compile/context.rs`, plus its test. Root/numeric-component paths
keep the whole-item formatter. String/crate paths preserve Item Display bytes,
including empty, later-crate, Unicode, long and deep components. No VM, public
API, hashing, Item Display, Cargo profile or shipped dependency change.

The initial handler-sharing feasibility selection was withdrawn before an
engine edit: the retained source/cost audit did not justify it. The accepted
replacement is one string-construction site, not a bundle of registration
changes. This is an isolated main-engine experiment, not a product migration.

Claude's benchmark driver history is preserved through `f49d36b1` in bench
merge `d22069fb`; independent read-only reconstruction is `22565065`.
Official artifacts live under `results/startup-0179/official1/`; frozen subject
manifest is `probes/startup-0179/subjects.json`. Raw-file SHA-256:
`d0643c0fc1ca42f6c4e9e5d9748c351da546b0baed3d1226349d6e5fab848b03`.

Six deciding binaries use rustc 1.98.1, P0 default release opt-level 3, default
CGUs, lto=false and panic=unwind. Each fresh build uses the same paths, explicit
safe EB environment and empty target directory. All 32 target-code invocations
have no explicit codegen-units flag. Rune features are exactly
alloc/anyhow/fmt/serde/std; tracing and diagnostic cfg are not enabled.

| Role | Base SHA-256 prefix | Candidate SHA-256 prefix |
| --- | --- | --- |
| Primary | `af06e8b3b162` | `e4a5f207c38a` |
| Counter | `0500d0ebec87` | `e0f39201842b` |
| Allocation | `9b2b35640dde` | `2f1694c655da` |

Primary sizes are 9,030,216 and 9,053,984 bytes: candidate +23,768 bytes.
Builds took 36.6–37.2 s each. The binary-size rise is descriptive; it prompted no
source change, post-result variant or unplanned diagnosis.

## Correctness, feature checks and first-use coverage

Base all-target/all-feature suite: 601 passed, 0 failed; candidate 602/0.
Focused context contracts under exact production features: 6/0 and 7/0.
Alloc-only no-std checks pass for both. Exhaustive registration inventories
and deterministic event order match the frozen base for both stdio settings
and both doc/no-doc configurations. User-module constants/isolation/refusal
controls pass; candidate direct/fallback bytes match 19 frozen display cases.
The copied first-use scripts/mapping are pinned before candidate editing.

The original production-feature unit-test command failed with 32 errors in
Rune's existing root test module because it assumes emit/workspace APIs.
Claude approved a tests-only amendment before applying it: that module alone
is excluded under `rune_startup_inventory`, declared in the existing check-cfg
list. All-feature suites retain it; no deciding build enables the cfg. The
focused first-use tests use public compile/VM APIs rather than root helpers.
No other test module or production feature was omitted/added.

Both cold first-use workloads exercise the mapped capabilities of all 34
constructor factories, including private collections, macros, traits,
generators, streams and futures. Namespace-only/reexport/native-type probes
are identified as such; this is not coverage of every standard-library method.
The harness completes async main with argument `((),)`, existing 1e9 budget,
requires returned i64 equal to 42, then prints `FIRST-USE 42\n`. Both stdio
variants have status 0, that exact stdout and empty stderr. Ordinary `run`
would not complete this async fixture; its use was rejected before measurement.
The in-crate first-use test has no imposed budget, a disclosed difference.

Official correctness is 42/42: the 40 standing fixtures plus both first-use
variants. Only the existing pointer/thread-id normalizations are used where
applicable; both first-use rows are compared exactly. Budget controls preserve
expected completion/halt statuses and output. Allocation call deltas are 0
without default context, and exactly −5,801 everywhere it is constructed,
including both first-use workloads and manual-next. Context calls fall from
32,304 to 26,503 (−18.0%); cumulative allocated bytes fall 137,036. Context
tracked peak remains 1,767,837 bytes. Run-answer and both first-use tracked
peaks likewise remain unchanged. These are tracking-pass observations, not RSS.

## Preparation stops and reviewed amendments

All failed preparation attempts and approvals are retained; there was one
official measurement and no post-result repair/replay.

1. **prep1 STOP:** the strict unchanged-harness binary hash differed:
   `619d30c3…` against 0178's `7a66b042…`. An untimed investigation rebuild
   reproduced it. It was run by hand, not through the ledger, and is labelled
   with its safe command/status/identity receipt. The original control failed;
   whole-binary identity is not claimed.
2. **Reviewed neutrality amendment:** byte comparison accepts only validated
   build-id descriptor bytes, equal-length local ThinLTO symbol suffix digits,
   and source-diff-explained panic line fields. Every other file byte—including
   ELF/program/section headers and padding—must be identical. Panic Location
   pointers are resolved via unchanged RELATIVE relocations and PT_LOAD mapping
   to the exact context.rs path in both files. One location moves 535→537 at
   column 47 because two test-only source lines were inserted above it.
   Of 9,007,272 file bytes, 477 differ: 20 build-id, 456 suffix digits in 32 symbols,
   one line-number byte. Machine code is identical; panic diagnostics differ.
   Earlier comparator gaps accepted an ELF-header mutation and checked path
   length without path identity; repaired before replay. All 33 controls pass,
   independently reproduced, including both concrete refusals. Control binaries
   are retained before target cleanup.
3. **prep2 STOP:** a driver guard confused `--check-cfg` declaring a known name
   with `--cfg` enabling it. The build had not enabled the diagnostic cfg.
   Reviewed repair exempts only its quoted declaration and still refuses active
   cfg/RUSTFLAGS/feature-like mentions. No prep2 binary is reused.
4. **prep3 succeeds:** suites, neutrality, six fresh builds, freeze, rehearsal
   and controls. The first new-control-script run failed on relative output
   paths and a floating-point synthetic 10% boundary; only control code/inputs
   were corrected. The deciding formula and thresholds are unchanged.
   Retained final controls: 16 lifecycle/retention, 15 startup/identity/decision,
   and 33 neutrality, all pass.
   Codex's independent new-control replay first used an output directory in
   `/tmp`; two controls rejected receipt paths outside the bench repository.
   Repeating at the required in-repository path passed all 15 without a tool
   change. This invocation error preceded official approval and is disclosed
   in that approval message; it collected no deciding samples.

The unchanged-harness control isolates the tests-only base. Deciding binaries
use an explicit adapted harness with the completing first-use mode, so they
are not the 0178 historical binaries. Applicable 2% historical instruction
reproduction is still required against those references; all gated rows pass.
Tiny FIFO floor/empty-context windows remain descriptive. New first-use
workloads have contemporary paired baselines, not invented historical ones.

## Single official run and reconstructed decision

Official 1 ran once from the committed clean driver tree; exit 0 and status
complete denote a completed scientific STOP. Representative child-affinity
controls pass; per-sample affinity fields observe the controller, not every
child. Safe E0/EB environments, process-group/reap/survivor/deadline receipts
are retained. Raw records precede checking. Fake-secret success/failure,
ledger/archive rehearsal and final sentinel scan find 0 occurrences. Claude's
known-credential receipt (`002b3276`) reports 0 matches in 249 tracked driver/
result files, including decompressed archive content; its positive control
finds the synthetic needle. This is a reported in-memory scan, not proof of
credential rotation, which remains separately unconfirmed.
Calibration differences are −0.010079 ms for true and −0.001562 ms for floor,
inside the frozen 0.15 ms gate.

The independent auditor imports no measurement/decision code. It reconstructs
1,014 raw rows, 23 PMU/wall workloads, 10 instruction/cycle samples per side
(five ABBA repetitions) and 30 wall samples per side (ABBA/BAAB/ABBA). It checks
raw counters and running percentages, sample order, medians, deciles, receipt
outputs, allocation counts, calibration and pinned historical references.
All decision rows and six trigger entries reproduce exactly. Its control
suite accepts the original and refuses 11 corrupted copies. No subject executes
in this audit and no new deciding sample is collected.

| Workload | Instructions change | Base wall ms | Candidate wall ms | Wall change |
| --- | ---: | ---: | ---: | ---: |
| context | −7.242% | 3.760 | 3.553 | −5.51% |
| runtime | −6.890% | 3.962 | 3.817 | −3.65% |
| compile-answer | −6.508% | 4.335 | 4.173 | −3.74% |
| run-answer | −6.504% | 4.380 | 4.194 | −4.26% |
| first-use-true | −3.479% | 7.292 | 7.144 | −2.03% |
| first-use-false | −3.492% | 7.288 | 7.130 | −2.16% |

These are whole-process engine-harness medians. The startup wall decreases
are inside their own baseline p10–p90 widths; they are not resolved wall-time
wins under that noise criterion. Context instructions fall 1,945,888; no work
is deferred or charged to a warm singleton. The historical shipped
`rnx eval 42` reference 4.173 ms is a separate subject, unchanged by this record;
no below 1 ms product claim follows.

| Regression trigger | Observed | Allowed comparison |
| --- | ---: | ---: |
| fib instructions | +2.283% | maximum +0.5% |
| fib wall | +1.407 ms (+2.73%) | base width 0.908 ms |
| calls wall | +2.925 ms (+3.07%) | base width 1.290 ms |
| numeric wall | +9.385 ms (+8.32%) | base width 1.788 ms |
| signed range wall | +13.234 ms (+15.58%) | base width 5.064 ms |
| negative range wall | +13.716 ms (+16.21%) | base width 4.873 ms |

No opposing-change trigger fires: the for-range instruction decreases are
under 1%, not above the 3% required by that rule. Descriptively, numeric cycles
rise 8.32%, signed-range 16.11% and negative-range 16.38%, while their instruction
counts decrease. Each includes the changed startup registration; the scripts'
hot execution does not invoke the new helper. This is a workload-level result,
not proof of an instruction-layout, cache, branch, frequency or compiler cause.
No post-result disassembly/Callgrind/layout experiment is performed here.

Frozen precedence stops at regression step 2. Without regressions the two WIN
legs would still fail the 10% requirement, giving NO-WIN. The cold first-use
instruction decreases preserve full capability but cannot override a failed
workload or substitute for either WIN leg. Candidate branch retained unmerged;
no fork-main integration, rnx dependency change, profile adoption or deployment.
