# Implementation plan 0315: A tensor method family runs on the new engine

Status: Proposed

Paired ADR: ../adr/0315_a_tensor_method_family_runs_on_the_new_engine.md.

1. Boundary note, read-only. Cover:
   - the file-only unary producer descriptor;
   - receiver-to-output admission and cold dispatch;
   - the file-entry binding;
   - turning `unary!` into the single declaration and removing the names
     from `assoc!`;
   - the adoption charge over the stated domain.
2. Restore the published source guard on all 40 items. Build the core
   change with a synthetic unary control, then run the complete early
   fixed-route screen. On a band failure, stop.
3. Integrate the Candle family. Add the per-member route and value,
   chain, error, accounting, extension, session, inventory and mutation
   controls.
4. fmt, Clippy and preflight. Sign. Then run `final.sh --required` on the
   signed commit. Source review, one complete final, Results.
