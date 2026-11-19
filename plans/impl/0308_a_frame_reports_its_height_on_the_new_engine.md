# Implementation plan 0308: A frame reports its height on the new engine

Status: Proposed

Paired ADR: ../adr/0308_a_frame_reports_its_height_on_the_new_engine.md.

1. Boundary note, read-only: enrolling `polars::Error` (charge, display,
   `kind()`), the checked `Result<S, E>` read through registration, the
   cold output and the frontend proof, how `height()` can fail, and any
   inventoried item touched.
2. Record the old engine's REPL outputs for every control first.
3. The error enrollment, the certified typed-result read metadata and
   cold output, and the admission rule (input-level `?` and direct
   `match` only); then `height()`; the early fixed-route screen.
4. Session, error, capacity, registration, helper-refusal, retained
   helper, escape, routing and inventory controls; mutations.
5. fmt, Clippy, the suites, preflight and the commit checker; sign;
   source review; one complete final; Results.
