# Implementation plan 0318: A separately linked engine runs a file

Status: Proposed

Paired ADR: ../adr/0318_a_separately_linked_engine_runs_a_file.md.

1. Boundary: Codex's 0318 proposal at 1a8d5187. Record any deviation
   before editing.
2. Add the engine package, header, gated file-execution shim, ABI v1 with
   pre-effect validation and bounded diagnostics, and the
   thread-scoped host callback.
3. Add the C launchers with digest binding and owning-mapping
   verification, and the TLS budget slot.
4. Add the callback, refusal and TLS controls. Capture the split pair
   through the 0317 gate and report the stock-to-sibling boundary cost
   separately.
5. fmt, Clippy and preflight. Sign. Then run `final.sh --required` on the
   signed commit. Source review, one complete final, Results.
