# Implementation plan 0312: A checked file runs on the new engine

Status: Proposed

Paired ADR: ../adr/0312_a_checked_file_runs_on_the_new_engine.md.

1. Boundary note, read-only. Cover:
   - the sealed entry: its owned domain, the argv construction and
     cleanup;
   - the exact-local file policy and the escape checks;
   - where the file runner selects the entry, and the pre-effect
     decision;
   - failure and diagnostic mapping, without borrowing `Program`;
   - the work-budget, signal and lifecycle handling;
   - the unbounded file allowance and the injected test limit;
   - the purpose-aware extension registration and the private file
     context, and whether isolating it stays narrow (otherwise opt-in);
   - the Polars diagnostic settings, re-measured on the path reader.
2. Measure the old engine's outputs for the witness and controls once,
   as a record only.
3. Restore the published source guard on all 40 items. Build the entry
   with a synthetic file control, then run the complete early
   fixed-route screen. On a band failure, stop.
4. Wire in the file reader and the witness. Add the admission, escape,
   failure, signal, binding-isolation, routing, inventory and mutation
   controls. Run the large-file CLI witness; automatic routing changes
   only on its pass.
5. fmt, Clippy, the suites, preflight and the commit checker. Sign. Then
   run rnx root-tests on the signed commit. Source review, one complete
   final, Results.
