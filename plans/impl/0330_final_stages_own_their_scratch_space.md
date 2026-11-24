# Implementation plan 0330: Final stages own their scratch space

Status: Proposed

Paired ADR: ../adr/0330_final_stages_own_their_scratch_space.md.

1. Boundary: published 0f3df71d and the 0328 FINAL-v1 and v2 evidence.
   Trace temp-writing stages and test families through the actual child
   environment and controller-assigned roots. Identify which exceptions
   use a private mount and which use a host path. Record deviations
   before editing.
2. Assign private stage scratch through the existing ownership and
   recovery paths. Keep producer lifetimes, bounded failure diagnostics
   and the census and fixed-route contracts.
3. Bind disk-only fixtures to the guarded bootstrap handoff. Preserve
   real build-path admission and account for the output filesystem.
4. Repair the leaking test families and verify cleanup independently of
   the controller's sweep. Keep caller-requested evidence.
5. Run the ownership, exception, disk and failure controls from the ADR,
   including the full final-controls command under the real recipe.
6. fmt, Clippy, preflight, sign and source review. One complete final;
   report its duration and any attributable leftovers.
