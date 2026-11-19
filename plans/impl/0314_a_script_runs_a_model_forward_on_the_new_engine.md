# Implementation plan 0314: A script runs a model forward on the new engine

Status: Proposed

Paired ADR: ../adr/0314_a_script_runs_a_model_forward_on_the_new_engine.md.

1. Boundary note, read-only. Cover:
   - the sealed registration with receiver, argument and success
     descriptors;
   - the two-native case in the cold shared-producer branch;
   - the frontend and lowering distinction from 0310's column arguments;
   - the file enrollment ahead of the `producer_outputs` skip;
   - the left-to-right borrows and the error precedence;
   - the retained charge.
2. Restore the published source guard on all 40 items. Build the cold
   branch and registration with a synthetic two-native control, then run
   the complete early fixed-route screen. On a band failure, stop.
3. Integrate Candle. Add the file, value, error-order, moved-operand,
   lifetime, accounting, session, `model.rn`, inventory and mutation
   controls.
4. fmt, Clippy and preflight. Sign. Then run `final.sh --required` on the
   signed commit. Source review, one complete final, Results.
