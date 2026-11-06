# ADR 0209: Enforce commit discipline at the monorepo root

Status: Accepted

Port ket's read-only commit preflight discipline to the rnx monorepo. Check
committed objects, signatures, author and committer identity, subject-only
messages, printable subject syntax, plan lineage and document scope before
expensive validation. One shared policy constant sets the implementation cap
to one, as required by the user's RULE #1. Each record has one signed plan and
one signed implementation, paired `plans/adr/` and `plans/impl/` documents and
an opening `Status:` line in its ADR. Plans may be pending implementation.

Keep the existing shared four-digit sequence beginning at 0206. Unlike ket's
current five-implementation limit and ADR-only plan commits, this repository
requires one implementation and both plan documents. Preserve the historical
0206 follower record, whose implementation plan cites the canonical imported
Rune ADR at an exact revision. No duplicate new cross-repository ADR is needed.

Use fixed grandfather cutoffs: rnx `bc0fada750c6fa50f2b0e765a2985155bd933e00`
and Rune `1b4b66bd3657868ecb1d49f8b3f355585b3ec98d`, each the parent of its first
0206 plan. Check the adopted portions of both histories in their original path
contexts. Do not reword older commits. Verify the history-import merge's exact
second parent and subtree equality and retain first-parent host continuity.
Future merges cannot introduce unchecked second-parent history. Reject shallow
repositories and history replacement; do not let a command-line cutoff bypass
normal prerequisite policy.

The checker is the single root policy used by both host and engine cheap
prerequisites. Keep signatures and historical objects intact. Synthetic tests
exercise malformed messages, wrong identities, missing plans, duplicates,
unsigned commits, document scope, invalid cutoffs and merge bypasses. Validate
the real adopted history as well. This tooling-only record requires these
controls and integration checks, not another runtime benchmark or Miri run.

Port the test-concurrency rule: a parallel-only failure is a defect candidate,
not a flake. Retain failures, reproduce under representative contention, and
never retry until green or serialize tests merely to hide the trigger. Any
exclusive-resource lane needs an explicit reason. Use durable receipts and
bounded jobs with terminal notification.
