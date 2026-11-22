# Implementation plan 0323: The final owns its build caches

Status: Proposed

Paired ADR: ../adr/0323_the_final_owns_its_build_caches.md.

1. Boundary: inventory every Cargo writer in the complete catalogue,
   nested writers included, against published 555cdb1a. Record each
   writer's lane key, or its transient owner, and any deviation before
   editing.
2. Add owned, locked lanes, one per existing sharing group, and route
   every writer to one. Keep the fixed-route environment pinned.
3. Add per-recipe marks from Cargo's JSON stream, the drained sweep,
   whole-lane retirement and reclamation before admission. Keep the
   existing output parsers.
4. Move record scratch for `polars-inventories` and `polars-suite` to
   stage lifetime. Add a lane only where timing justifies it.
5. Run the incremental A/B and set the per-lane choice from it.
6. Add the churn and admission controls. fmt, Clippy, the full
   final-controls command and the targeted checks. Sign.
7. Claude and Codex agree on the one-time migration list and delete it.
   Source review, one complete final, Results with the footprint report.
