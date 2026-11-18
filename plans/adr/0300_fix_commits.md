# ADR 0300: Fix commits

Status: Proposed

Base: published 85b40eb01f2f4895c38ccce3b810af495d09d333 (ADR 0299).

## Problem

Every commit is half of a record: `plan: adr NNNN …` then `impl: adr
NNNN …` (`ci/commit_subjects.py`, ADR 0217's one open record). A small
correction with an obvious decision, such as an intermittent test, a typo or a
formatting pass, still costs a numbered ADR, a plan commit and a review
of a decision nobody disputes. The user asked for a lighter form where
an ADR is too heavy.

The first case is the 0290 control `capacity_returned_after_the_recheck_is_not_lost`
(`tools/final/tests/scheduling.rs`). It fails about once in a hundred
isolated runs (2 of 200 at 79056cc1), and failed the first 0299 final.
The test arms its after-recheck handoff on the first token event taken
after the chunk write and assumes that event covers the write. A late
notice from the gate's earlier token return can be taken first. Its
recheck then arms the handoff while the chunk write's own notice still
raises the flag, so the handoff write is coalesced. The controller loses
nothing; the control's expectation is timing-dependent.

## Decision

1. A third subject form, `fix: <text>`, with the same character set,
   label ban, empty body, signature and identity as record commits. An
   ADR-style number is forbidden in it (`fix: adr 0300 …`); ordinary
   numbers in the text, such as an HTTP status or a timeout, stay valid.
2. A fix commit is valid only between records: no plan is open (after
   an implementation, before the next plan). It consumes no record
   number and leaves the ADR sequence unchanged; it cannot satisfy a
   pending plan or take the next number. Identity, signature, the
   single-parent chain and every other history check still apply.
3. A fix commit adds, removes or renames (on either side) no file under
   `plans/` or `rune/plans/`, and does not replace either root.
4. What a fix may be is a review judgment, written in `ci/README.md`: a
   correction whose decision is obvious and needs no design, such as an
   intermittent or wrong test, a typo, a formatting or style pass, or a
   broken control. A change to product behavior, a public contract,
   measurement identity or the protected execution path is a record.
   Review and publication are unchanged: Codex accepts, then one final
   on the candidate (complete or required, as its selection says).
5. The final carries a fix as a distinct typed phase. `types::Record`
   today requires a plan hash and a record number, and
   `selection::check_selection` lets only an implementation publish. A
   fix instead carries its commit, subject, parent and selected paths;
   its baseline is its parent and its selection follows its changed
   paths. It passes through selection, clean-candidate admission,
   finalization and revalidation, and may publish as an implementation
   does. The numbered-record JSON, revalidation of existing receipts and
   `rnx-jobs` estimate matching (by scope and stage set) are unchanged;
   plans and explicit development runs keep their eligibility. Receipts
   and `rnx-jobs` name a fix as a fix.
6. `ci/data_hygiene.py` compares new or growing files against the
   fix's own parent, not the most recent plan's parent, so consecutive
   fixes are not checked against a stale tree. The fixed lint and anchor
   policies keep their declared comparisons.
7. The 0290 scheduler control is the first fix commit, after this
   record publishes, as a separate fix.

## Constraints

- No new crate and no new Python file. `ci/commit_subjects.py` and its
  tests change in place.
- Every existing history check (identity, signature, first-parent chain,
  pinned cutoffs, sequence, the import and follower rules) is unchanged
  for record commits.

## Controls

- The checker over planted histories: a fix between records passes; a
  fix while a plan is open, a fix with a body, an unsigned fix, a fix
  with an ADR-style number and a malformed `fix:` subject each fail with
  a named rule, while a fix whose text holds an ordinary number passes;
  additions, removals and both sides of a rename under `plans/` and
  `rune/plans/`, and a replacement of either root, each fail; a fix
  cannot satisfy a pending plan, and the next plan after one or more
  fixes still needs the next record number.
- The final over a fix candidate: selection, clean-candidate admission,
  finalization and revalidation, with its parent as baseline and its
  stages from its paths, and its receipt naming the fix. A record
  candidate, an existing receipt's revalidation and the `rnx-jobs`
  estimate are unchanged.
- Data hygiene over two consecutive fixes: a planted case in which the
  fix's parent and the preceding ADR's baseline differ, compared
  against the parent.
- A mutation per rule fails its control.
- One complete final.
