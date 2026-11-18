# Implementation plan 0305: A DataFrame reaches the new engine

Status: Proposed

Paired ADR: ../adr/0305_a_dataframe_reaches_the_new_engine.md.

1. Boundary note, read-only: the registration-time metadata API in
   Rune's module system, how the adapter attaches it to the existing
   type, `empty` and `width` bindings, the single display owner, the
   charge, the session admission path, and confirmation that no
   runtime, host bridge, store or interpreter item changes.
2. Record the old engine's outputs for every session control first.
3. The metadata API, then the Polars enrollment of the existing
   bindings; the early fixed-route screen.
4. Session parity, charging and release, refusal and routing controls;
   inventory refresh; mutations.
5. fmt, Clippy, the suites, preflight and the commit checker; sign;
   source review; one complete final; Results.
