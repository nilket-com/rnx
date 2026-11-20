# ADR 0316: Native setup and execution are measured separately

Status: Proposed

Base: published ff5bf81efba559573d0b8f0de84fd9fba919ef89 (ADR 0315).

## Problem

The fixed-route gate scores each workload on whole-invocation
instruction counts: W (executable and unknown objects) and T (also libc
and the loader). These are taken between the entry and normal return of
`PreparedInvocation::invoke`. With native code requested, that window
includes one-time native preparation (region analysis, the cache
decision, emission) as well as execution.

The per-record 0.5% level band failed twice on `shared_rows` with native
code on. Both times a constant, roughly 2.2k-instruction shift came
from native compilation, when unrelated code moved a ThinLTO placement
decision: in 0310 a `host_call` body edit, and in the first 0316 attempt
a registry field type. Two root fixes failed:
- Forcing `BVec::push` inline stabilized that call edge but left
  residues of 274 to 390 instructions in both historical pairs.
- Isolating region analysis was audited and declined, not built: an
  audit showed it could not make whole-run totals exact, because the
  executor and the Store shift too.

Both phases are real costs. A short script pays setup in full. A
long-running script pays for every extra instruction in its loop. The
user ruled on 2026-11-19 that they get separate budgets, with these
requirements:
- The phases are measured directly. A fixed cost inside execution counts
  against execution.
- The 0.5% execution limit and the growth check stay. Setup gets its own
  limit and a cumulative budget against a fixed baseline. The whole total
  is still reported and reconciled with the phases.
- The limits are frozen from independent calibration before the
  historical failures are replayed.

An exploratory, read-only phase diagnostic at ff5bf81e, over 214
captures that reconcile to the process total, found the following. These
captures predate the freeze rule. They are not calibration data.
- `shared_rows` setup accounts for each historical shift: +1,597 W in
  0310 and +2,183 W in 0316. Execution changes by -1 to +10 instructions.
- The first 0316 candidate also has a real execution regression on
  native-on `fib`: +389,780 W (+0.5264%) at size 5, of which 389,779
  instructions are in the recursive `Executor::invoke_args` loop body. The whole-invocation gate
  passed it because about 48 million setup instructions sat in its
  denominator.

## Decision

1. Native-on captures get one extra boundary, at the entry of the outer
   `Executor::invoke_args` specializations' `closure#0`. The addresses
   are resolved from the measured ELF by full owner and signature, with
   the set of specializations, unique addresses and mangled names, and
   the source function all checked.
   - Everything before that entry is setup: region analysis, the cache
     decision, emission and activation preparation.
   - From that entry through the original outer return is execution:
     frame entry, the dispatch loop, result export and teardown. Any
     later compilation also counts as execution.
   - Callgrind is zeroed at outer entry. It dumps setup at the split and
     execution at the return.
   - A capture must show entry, then exactly one in-frame split, then
     return, all on the same thread and invocation, with the expected
     fixture, route and output and a zero exit.
   - A missing, ambiguous, duplicate or nested boundary rejects the
     capture. The phase is never inferred from a similarly named callee.
   - Native-off keeps its one-map capture, and its setup is recorded as
     inapplicable.
   - Every production capture runs in a private mount namespace. For a
     given fixture, size and mode, baseline, candidate, execution anchor
     and setup reference see byte-identical argv, working directory and
     ordered environment across runs. Mount each role's actual ELF,
     fixture and Valgrind tools at fixed read-only paths. Mount only that
     capture's output directory at the fixed output path used by vgdb and
     Callgrind. Pass the ordered measurement environment explicitly,
     including removal of wrapper-added variables such as PWD.
   - The capture records the launch strings passed to the measured process
     and its real input/output paths. Bind the measured configuration and
     input identities into the capture key. A launch differing from the
     declared configuration fails as incomparable before scoring.
2. Reconciliation: each dump's object self counts match its own summary.
   Setup plus execution reconstruct the whole per object, and T matches
   Callgrind's process-wide collected total. Otherwise the receipt
   fails. The whole-invocation total is still reported.
3. Execution keeps the existing bands, checked separately on W and T
   with signed integer bounds:
   - the large-size level and the size-1-to-5 growth at 5 per mille
     against the previous record and 10 per mille against the anchor;
   - the existing per-record growth check on the recombined whole, at 5
     per mille;
   - the 10 per mille level and growth checks on the recombined whole
     against the same anchor.
   An improvement in setup cannot pay for a regression in execution.
   There is no averaging across workloads, modes or objects. A growth
   denominator must be positive, or the receipt is invalid.
4. Setup gets absolute-instruction limits per native fixture, per metric
   and per size, against the previous record, plus an absolute growth
   limit for the change in (size 5 − size 1). A zero calibrated envelope
   is an exact growth control.
   - A cumulative setup envelope is measured independently against the
     same published anchor. A per-record allowance never adds to it.
   - Setup checks act on regressions. Improvements are recorded.
5. Calibration is independent and frozen before any acceptance run.
   - Before any calibration count is collected, the plan records:
     - the training and reserve manifests;
     - the base and anchor revisions;
     - the generation recipe and its seeds;
     - the expected semantic invariants;
     - the cell matrix, the repeat count and the aggregation formula.
   - The sample, on each revision, is eight training layouts, four
     reserved layouts, and two independent clean builds of the unmodified
     source. The variants are generated in code the eight fixtures never
     run, for example by reordering independent private helpers or
     renaming private symbols in unrelated host modules. They use the same
     toolchain, flags, paths and environment. The revisions are the
     published anchor and a published baseline from before the binary
     family.
   - Each variant is checked by source diff to leave the executed
     algorithms, fixtures and the 40 protected items untouched. Image
     identity, outputs and routes are checked too. Whether a variant
     actually changes emitted layout is recorded, and inert variants,
     duplicates and failed controls stay in the report.
   - Training must cover the affected compiler routines, as shown by
     symbols and disassembly, before scoring. If it does not, calibration
     is recorded as insufficient and the record stops. Variants are never
     selected by the threshold they produce.
   - The per-change setup allowance, per kernel and per metric, is the
     largest absolute pairwise difference between eligible training
     layouts on the published baseline. One maximum is taken across both
     sizes and applied to each size separately.
   - The cumulative setup allowance is the largest absolute departure of
     an anchor training layout from the clean fixed anchor, again one
     maximum across both sizes applied to each size.
   - The setup growth allowances are the same two spreads, per-change and
     cumulative, computed on the signed (size 5 − size 1) counts. A
     measured zero gives a zero allowance.
   - Missing cells are rejected. Every replicate is kept: no outlier is
     deleted and nothing is rounded up.
   - Calibration uses the production measurement code. If a scratch
     observer is used instead, an exact, matched-path equivalence control
     between it and the production observer must pass before the frozen
     table is used. The selector, the counting rules and the capture
     inputs are bound to the calibration evidence. A discrepancy stops the
     record before any held-out run.
   - The numbers, the rule, the manifest and the capture-recipe digest are
     frozen in this signed record before the reserve is opened or any
     acceptance replay runs.
   - The 0310 and 0316 versions, their split and root-replay builds, the
     214 exploratory captures and 0277's figures are excluded from
     calibration. Replaying the historical pairs after the freeze tests a
     precommitted policy whose outcome is already known. The four
     reserved layouts are the unseen test.
   - A failure stays a failure. Changing calibration needs a new decision
     and fresh unseen validation data.
6. Migration: the phase schema and capture key are versioned. The plan
   reports where the current published baseline sits relative to the
   fixed anchor. If published history already consumes or exceeds a phase
   budget, that conflict is recorded as a migration failure and brought
   back for review, not rebased away. The anchor
   revision 48fe3b15f48c92be91f04aa97f1f2f3fc31cf860 is recaptured with
   the new observer. Phase artifacts are bound to the measured binary,
   fixture, recipe and the ordered boundary receipt. Published one-map
   receipts are revalidated under their original version; only fresh,
   binary-backed captures get phase numbers.
   After signature of this layout amendment, remeasure the published
   setup reference ff5bf81e under the canonical layout. Bind its counts
   and measured configuration in the signed reference. Cumulative setup
   uses those stored counts. The independently frozen allowances stand.
   Recapture execution anchor 48fe3b15 under the same layout and report
   any movement in execution W and T by fixture, mode and size. Preserve
   the earlier PUBLISHED.json, the production migration stop, and both
   crossed diagnostics as historical evidence.
7. Files: `tools/column/fixed_route_vectors.py` and its GDB measurement
   child; `tools/final/src/fixed_route.rs` (schema, reconciliation,
   scoring); `fixed_route_stage.rs` (recipe, cache, retention);
   `tools/column/fixed_route_policy.json`; and their controls and
   witnesses. `compare_route.rs` and the cache export and revalidation
   consumers are reviewed. None of the 40 inventoried items, the engine,
   its codegen flags or the Candle registry changes.

## Constraints

- Measurement and scoring only. No product source changes.
- `final.sh --required` passes on the signed candidate before the
  complete final.
- No new crate. Tabs.

## Controls

The limits are frozen first. Then the four reserved layouts are checked,
and the held-out evidence follows, each item with its own verdict:
- The two historical failures are explained: `shared_rows`' cost lands in
  setup, within the frozen limit, and execution stays inside 0.5%.
- The first 0316 candidate fails on native-on `fib` execution, a real
  regression the old gate missed.
- Real negatives stay refusals:
  - 0302's A (setup -70; execution +268 at size 1 and +1,296 at size 5);
  - 0301's integrated binary (setup +13,221 W; execution +19,002 at size
    5).
- Planted regressions, each made in a scratch build, each landing in the
  intended phase as shown by the two profiles, and each rejected by the
  right check:
  - extra compile work, in setup;
  - a fixed execution surcharge, in execution, though it is
    size-independent;
  - extra work per row, in execution and growth.
- Scorer fixtures cover:
  - exactly at, and one instruction over, each limit;
  - W and T independently;
  - no cancellation between phases;
  - small successive increases that exceed the anchor.
- Capture failures each reject: a missing, ambiguous, duplicate or nested
  boundary; extra or missing dumps; bad totals; missing objects; and
  incompatible cache entries.
- A compiled execution specialization and the native-requested
  interpreter fallback are both exercised.
- The cache is checked on a cold miss, a genuine new-format hit, and
  refusal of mixed or corrupted artifacts. Selectors and tool hashes are
  recorded.
- One complete final.
- Put the same ELF at two distinct real paths and run the two placements
  with separate real fixture and output paths under the canonical layout.
  Require identical setup and execution W and T at both shared_rows
  sizes that exhibited the path effect.
- Change the actual measured launch configuration with the ELF fixed.
  Require an incomparable-capture error and zero rows scored.
- In one real run compare the baseline and candidate launch records for
  every matching cell, including native-off. Require byte-identical argv,
  cwd and ordered environment, with each real source recorded separately.
