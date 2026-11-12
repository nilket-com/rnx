# ADR 0261: a final keeps no bulk of its own and can run exactly what is required

Status: Proposed

Base: published 66e72772697f85fe4ee36fe5f3d25e892c351ad9 (ADR 0260).

## Problem

ADR 0260 released the named evidence roots. The complete final of that
record still left 4.5 GiB in its own directory: 3.7 GiB of baseline
checkouts and their build output made by the Clippy comparison stages,
and 0.75 GiB of fixed-route payload. ADR 0260 left both out, the first
on an estimate that was wrong by a factor of forty for a complete final,
the second because the shared anchor cache refers into the final that
produced an anchor.

The driver of ADR 0260 has two modes: every stage in the catalogue, 86
minutes, or a list typed by the person launching it. The gate already
computes which stages a record requires from what it changed. The mode
every ordinary record needs, exactly the required stages, 36 minutes on
ADR 0259, exists only as a list someone types. A rule typed by hand is
not a rule.

## Goal

After a final, its directory holds receipts and logs, not checkouts,
build output or measurement payloads. A record is finalized by one
command that runs the stages the gate requires for it, no fewer, and
says so in its result.

## Required behavior

Selection

- The driver has a required mode: the stages the gate requires for the
  candidate, closed under prerequisites, with no list given by the
  caller. Its result names the scope as required and carries the
  selection identity. The complete mode and the explicit list remain.
- A result states its scope truthfully. An explicit list is never
  reported as required or complete, even when it happens to cover them.
- Which scope a record needs to be published is decided by the gate
  policy in the tree, not by the launcher: at least the required scope.
  If the policy can express when a complete final is needed, such as a
  change to the gate tooling itself, say how in the note; do not invent
  a new policy rule without it being ruled.

Baseline checkouts

- The baseline source checkouts and build output that the Clippy
  comparison and suite stages create are owned by the final and released
  with the other roots, after every reader has finished. Their archives
  and diagnostics that a receipt binds are kept if small or summarized
  by digest in the manifest if not; the note gives the sizes.

Fixed-route payload

- The shared anchor cache owns its payloads: an anchor is published into
  the cache as verified copies bound by recipe and hash, atomically, and
  a cache hit reads only the cache. No cache entry refers into a final.
- Existing cache entries that refer into earlier finals are handled
  explicitly: migrated if their files exist and verify, otherwise
  dropped and rebuilt on next use. The note says which and what a
  rebuild costs.
- Then the final releases its fixed-route payload like the other roots.
  The fixed-route receipts that state the measurement and its verdict
  stay.
- Nothing about what fixed-route measures, its bands, its policy file or
  the inventoried items changes. A cache hit and a miss give the same
  verdict on the same candidate; a control shows it.
- The cache has a stated bound: what an entry is keyed by, when an entry
  can no longer be hit, and that such entries are released by the tool
  that owns the cache. No growth without limit.

All of ADR 0260's rules on ownership, order, refusal and failure apply
unchanged.

## Controls

- Required mode on this record: stage set equals the gate's required
  set; result scope is required; revalidation passes.
- An explicit list equal to the required set is still reported as an
  explicit list.
- A complete final on this record: its directory afterwards is reported
  by size and holds no checkout, build output or fixed-route payload.
- Anchor published, producing final removed, later final hits the cache
  and passes with the same verdict.
- A cache entry with a missing or altered payload is refused and
  rebuilt, not trusted.
- An unreachable cache entry is released by the cache's owner.
- Baseline reader ordering: release before the comparison has read the
  baseline fails the stage as today.

## Out of scope

Shared Cargo caches and their size; the Miri preparation; resumption.
