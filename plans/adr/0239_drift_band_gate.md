# ADR 0239: Drift band gate and path-derived required stages

Status: Accepted

Base: published 3fe44bf9 (ADR 0238).

## Problem

Three engine records in a row stopped at the fixed-route identity gate even
though none of them changed the fixed operation loop. ADR 0236 extracted a
macro and was withdrawn. ADRs 0237 and 0238 added code beside the loop: new
callers of shared helpers and new struct fields changed LLVM inlining across the
crate. Each stop needed a measurement round, an attribution report and a
user-approved exception bound to one build pair. The measured effects were
small and mostly favorable (0237 at most +0.351%; 0238 seven of eight
interpreted kernels faster, worst +0.084%), but each cost hours to a day.

Separately, ADR 0237 published with a failing root test (tests/budget.rs).
gate_policy.required() forces only the cross stage; every other stage is
chosen by the implementer, and 0237's final sequence omitted root-tests.

The user approved both changes on 2026-11-10: replace per-record exceptions
with a band and a cumulative cap enforced in code, and make required stages
follow the changed paths.

## Decision: source condition

The band applies only to drift the compiler causes, never to an edit of the
fixed route. Today's drift guard (ci/execute_guard) compares execute.rs with
its sliced copy inside one checkout; an identical edit to both copies passes
it, and it does not fingerprint layouts. Keep that mirror guard and add an
inter-revision source condition:

- A committed, reviewed, finite inventory names the fixed operation loop, the
  Frame and Executor definitions and the guarded helpers.
- The gate fingerprints each item at the baseline and the candidate. Any
  difference, a missing or duplicated item, or a change to the inventory
  itself is STOP for review.
- The source condition runs first, before the identity shortcut. A fixed-loop
  edit that happens to compile to identical bytes still stops.
- Mutation controls include an equal edit to both copies.

## Decision: drift band

After the source condition passes, the resolved identity and instance
comparison runs as today. On identity PASS the per-record band is satisfied,
but the cumulative cap still needs anchor evidence (below). On identity FAIL,
capture vectors.

Metric. W = rnx_Ir + other_Ir per case, where other_Ir includes anonymous and
JIT code totals. Raw total and dynamic-library totals are retained in the
receipt; dynamic libraries follow the protocol v2 ELF-object rule (binary
functions exact; libc banded at slope <= 0.001 and constant <= 4096 Ir) and do
not count toward W. Anonymous JIT addresses are compared only in totals.

Validity, checked mechanically before any verdict: B5 > 0 and B5 > B1, every
fixture present, program outputs identical across builds, and the same build
recipe, toolchain, CPU model and declared environment for baseline, candidate
and anchor. Any violation fails closed.

Per-record band, candidate against the record's baseline, on the eight
established CLI kernels at n=1 and n=5, native off and on, in exact integer
arithmetic:

- 1000 x (C5 - B5) <= 5 x B5;
- 1000 x ((C5 - C1) - (B5 - B1)) <= 5 x (B5 - B1).

Decreases never fail.

Cumulative cap, candidate against the anchor, with the same two inequalities
at 10 per mille. The anchor starts at 3fe44bf9. Anchor evidence is a receipt
of the anchor build's vectors under the current recipe; missing or stale
evidence (recipe, toolchain, CPU or policy changed) fails closed until it is
recaptured. An identity PASS does not bypass the cap unless such a receipt
exists. Percentages are never added across records; the cap is always a direct
comparison with the anchor.

Moving the anchor: the policy file binds the old and new anchor to a committed
record whose plan states the user's approval. The checker verifies
mechanically that the approval record exists in the candidate's ancestry, is a
plan commit, and names both anchors; it cannot verify that the user approved,
which review attests. A bare approval string is refused.

Outside either limit: STOP for review. A user-approved exception bound to the
exact pair, as in 0237 and 0238, remains possible but is no longer the normal
path. Limits, kernels, inventory and anchor live in committed policy files
read by the gate. Every per-case figure, both comparisons and the validity
checks go to the receipt. No timing campaign and no wall-time claim.

## Decision: path-derived required stages

gate_policy.required(paths) maps changed paths to stages that cannot be
deselected. The union never depends on which stages the caller selects. Paths
include added, removed and renamed files; where manifests or lockfiles change,
both the before and after dependency graphs decide what is affected.

- src/ or tests/ (rnx crate): root-tests, inventory, cross.
- rune/crates/: the Rune crate's own suite at the engine profile, Rune Clippy,
  cross, and the fixed-route gate when the crate's runtime sources change.
- adapters/<name>/: that adapter's suite and Clippy.
- tools/project/ and every root or ancestor Cargo manifest and lockfile: keep
  today's cross coverage, plus everything their dependency graphs reach.
- Plans, docs and tools only: commit discipline and the tools' own tests.

The boundary note lists the exact command and feature recipe of every stage.
A stage whose test discovery finds no tests, or whose suite is empty, fails.
The fixed-route gate gets a registered receipt contract in accept.py and
child_result, enforced at finalization like cross. Final acceptance refuses a
packet missing any required stage.

## Decision: Miri per-test receipts

Committed tooling runs Miri one exact test per process, in parallel sized from
detected cores and memory, longest first by recorded time. Start from the
miri_cache work drafted after 0238 published (target/0239/miri-cache.patch):
independent content keys for each test and for the prepared target,
artifact-hash validation, durable attempts and interprocess locks.

- The runner takes an explicit package and test inventory and verifies it
  against the live test list; no hardcoded per-record driver.
- The key covers test name, toolchain, MIRIFLAGS, features and a digest of
  the tracked crate, dependency and config inputs as resolved for the
  requested features. External path inputs are covered explicitly or refused.
- Any byte change inside those inputs, comments and lint attributes included,
  invalidates the key unless a separately reviewed source-equivalence proof
  authorizes reuse; source positions are observable through line and file
  macros and diagnostic tests. Plans and unrelated host source do not affect it.
- Recorded timings only order work and never authorize reuse. An interrupted
  attempt or timeout resumes; a completed failure is not retried automatically.

## Out of scope

Changing the kernel set, the Callgrind protocol or the identity tool itself.
Retiring the 0237 and 0238 exception files (they stay valid for their pairs;
the data cleanup record may remove them). Wall-time benchmarks.

## Acceptance

Band controls with synthetic vectors: each inequality exactly at, one Ir
above and well below; decreases; validity failures; the cap failing while the
band passes; missing or stale anchor evidence; an anchor change without, or
with a malformed, approval record. Source-condition controls: a fixed-loop
edit with identical bytes, an equal edit to both copies, an inventory change.
Replay of the retained 0237 and 0238 vectors reproduces their approved
verdicts as per-record results only. Stage-table controls: a 0237-shaped path
set requires root-tests; renamed and removed paths; a manifest change; an empty
suite fails; deselecting a required stage fails acceptance. Miri controls: a
real prepare, test and restart smoke; a killed run resumes without repeating
passed tests; corrupted artifacts and changed keys rerun; the completed 0238
semantic suite is not repeated to test the cache.

This record changes tooling only, so the fixed-route gate does not apply to
it. Required stages under the new table, preflight and the commit checker
must pass.
