# Implementation plan 0300: Fix commits

Status: Implemented

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

## Results

`ci/commit_subjects.py` accepts a signed, bodyless `fix: summary` with the
repository identity and one parent, only between closed records. It
refuses an ADR-style number (`fix: adr NNNN`) while allowing ordinary
numbers, and refuses any addition, removal or either side of a rename
under `plans/` or `rune/plans/`, or a replacement of either root. Each
refusal names its `fix.*` rule. A fix cannot close an open plan, and
consecutive fixes leave the next record number unchanged. `ci/README.md`
says what a fix may be and what stays a record. `ci/data_hygiene.py`
measures a fix from its own parent.

The final carries a fix as `Candidate::Fix`, with its commit, subject,
parent, paths and required stages. The baseline is the parent, and the
parent's subject must be an implementation or another fix. A fix is
publishable as an implementation is, and its subject reaches the final
result and manifest. `rnx-jobs` shows it by subject and short hash and
still matches estimates by scope and stage set. The numbered-record JSON
is unchanged, and the rebuilt controller revalidated the published 0299
complete receipt.

Controls: the history checker's planted fixes, in 50 Python tests with
the hygiene cases, including two consecutive fixes whose parent and
preceding ADR baseline differ. The controller ran a planted clean fix
through required selection, a publishable final and revalidation, and
refused a changed subject or a changed typed selection. 13 enforcement
mutations failed their controls. The controller package passed 275
tests.

The complete final passed all 101 stages in 43.7 minutes, with no
memory.high, memory.max or OOM events, and both fixed-route bands held.

Measured for the first fixes, from `catalogue::required` over their
paths:
- the 0290 scheduler control selects six stages, the longest
  `final-controls` at 45 s in the 0299 final;
- a root `journey.rn` reformat selects the three base stages.
