# Implementation plan 0317: The gate measures a runner and its engine image

Status: Proposed

Paired ADR: ../adr/0317_the_gate_measures_a_runner_and_its_engine_image.md.

1. Boundary: Codex's isolation proposal at 30ba8857, first record. Record
   any deviation before editing.
2. Add the versioned artifact description, split-image capture with
   loader-readiness and mapped-identity checks, exact-once attribution,
   and the cache, retention and revalidation changes.
3. Add the split-image fixture and the cache, refusal and accounting
   controls. Capture both historical triggers in split-image mode.
4. fmt, Clippy and preflight. Sign. Then run `final.sh --required` on the
   signed commit. Source review, one complete final, Results.
