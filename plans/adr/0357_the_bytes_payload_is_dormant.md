# ADR 0357: The Bytes payload is dormant

Status: Proposed

Base: published 2543c244 (ADR 0356).

## Problem

1. Bytes must run on the new engine before the old engine can be
   deleted. Thirteen byte-string cases and the `into_bytes`,
   `as_bytes` and `with_capacity` cases still run on the old
   core.
2. The first piece is the Bytes type and its heap storage, with
   no way to construct a Bytes value. On f6ce4a3f this piece
   (scratch 56db49ec) passed 30 of 32 per-record rows and 32 of
   32 cumulative rows. Native-off Sieve rose 475,411
   instructions at size 5 in W and T, because `Store::push` made
   its existing String query as an out-of-line call.
3. ADR 0355 accepts that kind of drift only for a
   representation-only delta carried by a reviewed proof pinned
   in the record's signed plan. 56db49ec was unsigned and had no
   proof, so it is not eligible.
4. Two exhaustive inventories outside the engine also name every
   type: the root routing inventory (`src/routing_inventory.rs`,
   test-only) and the cross host-path sweep. ADR 0356 added the
   sweep's Bytes witness.

## Decision

1. Add the Bytes type identity and its heap payload, a dedicated
   byte vector behind the existing branded handle, with its
   accounting and the exhaustive matches the compiler requires,
   including the routing inventory's Bytes classification.
   Nothing produces a Bytes value: no constructor, literal,
   conversion, host return or other ingress.
2. No new source-level check on an old path. An arm added to an
   existing match is selected only by the new discriminant, and
   no existing arm changes.
3. The record commits `tools/column/dormant/0357.json` under ADR
   0355. Its baseline is ADR 0356's measured source 1551d2c4 and
   publication 2543c244. After review, this plan is amended to
   pin the proof's path and SHA-256, and is signed again; the
   impl is signed on that plan.
4. The ordinary scores are kept. Only per-record execution rows
   covered by the verified proof may take
   `accepted_layout_drift`. Every cumulative row and every other
   check must pass. Any other failure stops the record for
   review; there is no tuning loop.
5. Scope: the new-engine source the payload needs, its tests and
   the proof file. The gate tools, bands and policy do not
   change.

## Controls

1. The structural checker passes and the proof matches the full
   delta, modes and blobs.
2. No byte-string case moves to the new engine; the gap-hunt
   dispositions and old-core routing are unchanged. Cross passes
   with Bytes in the kind matrix and every Bytes substitution
   refused.
3. The candidate's raw per-record and cumulative rows are
   recorded in the Results with the effective verdict.
4. One complete final.
