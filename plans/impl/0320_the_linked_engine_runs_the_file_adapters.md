# Implementation plan 0320: The linked engine runs the file adapters

Status: Proposed

Paired ADR: ../adr/0320_the_linked_engine_runs_the_file_adapters.md.

1. Boundary: Codex's 4/6 proposal, reconciled against published d7c04262.
   Record any deviation before editing.
2. Freeze the checked-file installer inventory, and map each shape to C
   fields and ownership. Do this before any production edit.
3. Add the versioned registration surface, the single adapter-side
   owner, and the descriptors with pre-effect rejection.
4. Add the isolation, parity and cap/cleanup controls. Freeze the launch
   configurations, then capture and attribute both images.
5. fmt, Clippy and preflight. Sign. Then run `final.sh --required` on the
   signed commit. Source review, one complete final, Results.
