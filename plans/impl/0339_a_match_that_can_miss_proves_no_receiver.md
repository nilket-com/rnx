# Implementation plan 0339: A match that can miss proves no receiver

Status: Proposed

Paired ADR: ../adr/0339_a_match_that_can_miss_proves_no_receiver.md.

1. Boundary: the published base. Reproduce the two observations of
   the ADR's Problem on both engines in both native modes, with a
   counted side effect in the method body. Probe the other forms
   whose value can be the implicit unit, as Decision 8 names them.
   Record each outcome and the route taken.
2. Add the predicate of Decision 2 where both proofs and the
   `Result` rule can use it. Repair the lowering as Decision 3
   states. Add its hit and miss witnesses.
3. Repair the proof graph as Decision 4 states. Add its witnesses
   through a local, a parameter, a script return and a nested
   match. Report before going on if the possible unit can't be
   carried through a join without a change beyond Decision 9.
4. Add the remaining controls: the positives, the refusal with its
   position and no earlier effect, and the two planted proofs.
5. Replay the retained proofs and the public admission corpus
   against the published base. List every changed source as Control
   7 states. Send the list for review before signing if any working
   public behavior is lost.
6. Score the file gate and both project screens.
7. fmt, Clippy, preflight, the whole Rune library test binary and
   the targeted checks in separate state. Sign. Source review, one
   complete final, Results.
