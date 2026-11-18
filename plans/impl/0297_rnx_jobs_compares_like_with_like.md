# Implementation plan 0297: rnx-jobs compares like with like

Status: Implemented

Paired ADR: ../adr/0297_rnx_jobs_compares_like_with_like.md.

1. `tools/final/src/jobs.rs`: the reference by scope and stage set, its
   label, and the fallback.
2. `tools/final/tests/jobs.rs`: the snapshots of both scopes and the
   mutation.
3. The Rust suites, preflight and the commit checker; sign; review; the
   required final; Results.

## Results

A running final's estimate is now taken from the newest other final
that passed, began strictly before it, has a finite positive duration,
and has the same known scope. For a complete final, that PASS must also
be complete, and it stays comparable across catalogue changes. For
required, explicit and docs finals, it must have selected exactly the
same stage names, in any order. The label names what was matched ("the
last required PASS of the same 6 stages"). Without a match, the view
shows no bar and no estimate, only the elapsed time and what it found
none of. Terminal views, discovery, liveness, regular-file reads, the
idle view and `--final` are unchanged.

The control plants both scopes:
- a required final against an earlier required PASS of the same
  stages, while a newer complete PASS is present;
- the fallback for a same-count PASS with one different stage, a later
  PASS, a zero-duration PASS, and an unknown scope;
- a complete final against the last complete PASS, while a newer
  required PASS is present.
Mutations back to the complete-only rule, to matching by count, and
without precedence, the duration check or the scope check each fail it.
The existing snapshots keep their text.

The required final passed in 2 min 17 s (`clippy-final`,
`commit-discipline`, `controller-controls`, `final-controls`,
`format-final`, `python-list`).
