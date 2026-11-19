# Implementation plan 0313: A script loads a model on the new engine

Status: Proposed

Paired ADR: ../adr/0313_a_script_loads_a_model_on_the_new_engine.md.

1. Boundary note, read-only. Cover:
   - the typed one-string constructor, its file-only association and its
     pre-erasure check on the ordinary host bridge;
   - the file-admission extension and the epoch re-binding;
   - the session refusal by entry kind;
   - the retained charge;
   - the `dims()` triple output;
   - keeping `Mlp` without a display.
2. Restore the published source guard on all 40 items. Build the
   registration, admission and no-display witness changes with a synthetic
   no-display native shaped like the model, then run the complete early
   fixed-route screen. On a band failure, stop.
3. Integrate Candle. Add the file, boundary, error-string, `model.rn`,
   session-decline, accounting, `dims()`, inventory and mutation
   controls.
4. fmt, Clippy and preflight. Sign. Then run `final.sh --required` on the
   signed commit. Source review, one complete final, Results.
