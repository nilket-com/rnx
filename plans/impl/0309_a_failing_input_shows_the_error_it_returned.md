# Implementation plan 0309: A failing input shows the error it returned

Status: Proposed

Paired ADR: ../adr/0309_a_failing_input_shows_the_error_it_returned.md.

1. Boundary note, read-only. Cover:
   - where each consumer reads the envelope;
   - how an outer `Err` is recognized;
   - the `Failure` used for a session report and its display;
   - one-shot's exit path;
   - binding staging, and whether anything protected moves.
2. Probe whether an explicit top-level `return Err(...)` is admitted.
   Measure the old engine's outputs for each control once, as a record
   only.
3. The session and one-shot handling, then the early fixed-route screen.
4. Session, one-shot, inner-`Err`, binding, bounded native-display and
   explicit-return controls. Replace 0308's pin. Add the mutation.
5. fmt, Clippy, the suites, preflight and the commit checker. Sign.
   Source review, one complete final, Results.
