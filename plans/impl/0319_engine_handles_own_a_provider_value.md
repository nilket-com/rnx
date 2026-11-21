# Implementation plan 0319: Engine handles own a provider value

Status: Proposed

Paired ADR: ../adr/0319_engine_handles_own_a_provider_value.md.

1. Boundary: Codex's 0319 proposal at 230ab56e. Record any deviation
   before editing. State the program's consumption point, including when
   preparation fails.
2. Add the versioned handle surface: the context, program and invocation
   lifecycle; typed, generation-checked tokens; re-entry rejection across
   both ABI versions.
3. Add the engine-owned opaque slots, the synthetic provider (create,
   shared read, drop) and the credit transfer under a drop guard.
4. Add the checked-file witness, and the lifecycle, credit and
   planted-fault controls. Check v1 compatibility. Capture the frozen
   split-pair matrix, and report stock-to-sibling differences as
   diagnostics only.
5. fmt, Clippy and preflight. Sign. Then run `final.sh --required` on the
   signed commit. Source review, one complete final, Results.
