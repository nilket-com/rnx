# Implementation plan 0346: Gate rules for declared and loader changes

Status: Proposed

Paired ADR: ../adr/0346_gate_rules_for_declared_and_loader_changes.md.

1. Boundary: the published base. Find what the dispatch
   observation records today and what the loader rule needs from
   the profile. Report anything in Decision 2 that the kept
   profiles can't support before building.
2. Declared transitions as Decision 1 states, with Control 1.
3. The loader rule as Decisions 2 and 3 state, with Control 2.
4. Score the two kept pairs again as Control 3 states.
5. fmt, Clippy, preflight and the targeted tests. Then the whole
   final-controls stage by its stage command. Sign. Source review,
   one complete final, Results.
