# Implementation plan 0300: Fix commits

Status: Proposed

Paired ADR: ../adr/0300_fix_commits.md.

1. The `fix:` form in `ci/commit_subjects.py`: subject and its
   ADR-number ban, between-records placement, no `plans/` changes on
   either rename side or root; its planted-history tests.
2. `ci/data_hygiene.py`: the fix's parent as baseline, with its planted
   consecutive-fix case.
3. `ci/README.md`: what a fix may be, and what must be a record.
4. The final: a typed fix phase through selection, admission,
   finalization and revalidation; receipts and `rnx-jobs` naming it;
   existing receipts and estimates unchanged.
5. Mutations; the suites, the Python discovery, preflight and the
   commit checker; sign; review; one complete final; Results.
