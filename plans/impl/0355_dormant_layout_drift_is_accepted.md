# Implementation plan 0355: Dormant layout drift is accepted

Status: Proposed

Paired ADR: ../adr/0355_dormant_layout_drift_is_accepted.md.

1. Boundary. Report, before any change: where the per-record
   verdict is computed and written, what the receipt holds, how
   revalidation and the docs successor replay it, and which
   witness form of Decision 4 the representation-only case uses,
   with the reason.
2. The verdict and the witness of Decisions 2 to 5, with
   Controls 1 to 4.
3. fmt, Clippy, preflight and the stages of Control 5 by their
   stage commands. Commit unsigned. Source review, one complete
   final, Results.
