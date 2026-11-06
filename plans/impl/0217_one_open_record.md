# Implementation plan: ADR 0217

Extend `ci/commit_subjects.py` with the ordering rule as a policy check that
applies only after the pinned cutoff. Keep the cutoff as one policy field next
to the existing pins. Reuse the existing segment walk and metadata validation.
Do not add a second traversal of the same history. Compute the next permitted
number from the committed tree at each plan's parent, including `rune/plans/adr`.
Report the violating commit and rule in the machine-readable FAIL receipt.

Define withdrawal narrowly: the implementation commit of a withdrawn record
changes only its paired documents, and its ADR opens with a withdrawn status.
Reject any other use of that status.

Add negative and positive controls to `ci/test_commit_subjects.py` with
temporary signed repositories, as the existing tests do. Run the checker on the
real history at the signed plan and at the final implementation, plus
format/lint checks for the touched files. Document the rule beside the
existing policy in `ci/README.md`. Then send the change for source review and
publish with receipt hashes.
