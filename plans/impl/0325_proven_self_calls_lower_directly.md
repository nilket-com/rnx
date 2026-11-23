# Implementation plan 0325: Proven self calls lower directly

Status: Proposed

Paired ADR: ../adr/0325_proven_self_calls_lower_directly.md.

1. Boundary: Codex's lowering proposal (seq 35303), reconciled against
   published 5530079d. Record any deviation before editing.
2. Split the self binding and target proof from the dynamic gate in
   `self_receiver.rs`. Order the specialization in `lower.rs` before the
   general closed-name dynamic route.
3. Add the guarded direct call with receiver and operands saved once,
   and the unconditional form only where ingress proves the receiver.
4. Fold the proven self nominal test in the native recursive-method
   analysis under a validated concrete receiver at region entry, as in
   Decision 4.
5. Extend the contract with the 0325 overlay and add the file witnesses
   on both engines.
6. Score Permute, the file gate and both project routes per Decision 8
   before signing, and report the guard cost and any residual.
7. fmt, Clippy, preflight and the targeted checks under the final's real
   recipe. Sign. Source review, one complete final, Results.
