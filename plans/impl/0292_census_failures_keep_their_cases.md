# Implementation plan 0292: Census failures keep their cases

Status: Proposed

Paired ADR: ../adr/0292_census_failures_keep_their_cases.md.

1. Pin 4714ae19. In the census comparison, a bounded error naming the
   implicated cases and reasons.
2. In `tools/verify.py`'s receipt: the error, and `diagnostic_files` for
   the summary and the implicated cases' outputs within the envelope's
   limits.
3. The controls of the ADR, at the verifier and through the final's
   failure retention.
4. The affected checks, preflight and the commit checker; sign; source
   review; one complete Rust final; Results from its receipts.
