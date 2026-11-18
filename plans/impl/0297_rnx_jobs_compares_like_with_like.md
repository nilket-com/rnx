# Implementation plan 0297: rnx-jobs compares like with like

Status: Proposed

Paired ADR: ../adr/0297_rnx_jobs_compares_like_with_like.md.

1. `tools/final/src/jobs.rs`: the reference by scope and stage set, its
   label, and the fallback.
2. `tools/final/tests/jobs.rs`: the snapshots of both scopes and the
   mutation.
3. The Rust suites, preflight and the commit checker; sign; review; the
   required final; Results.
