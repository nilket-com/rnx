# Implementation plan 0295: Running jobs and final progress at a glance

Status: Proposed

Paired ADR: ../adr/0295_running_jobs_and_final_progress.md.

1. `tools/final/src/jobs.rs`: the user-manager and process seam, final
   discovery, the final view and the snapshot renderer; `rnx-final jobs
   [-w] [--final DIR]`.
2. `tools/final/rnx-jobs`, the alias, and a README line.
3. Fixture tests with recorded snapshots.
4. The Rust suites, preflight and the commit checker; sign; review; the
   final; Results.
