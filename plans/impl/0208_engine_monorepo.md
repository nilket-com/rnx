# Implementation: ADR 0208 pure history import

Status: Accepted

## Exact import

The pre-plan rnx baseline is `002584a48d1f714e3856f5a3d6a99ac86c2204eb`.
The imported published Rune commit is
`6d7aa59dee00dd9c5c598b3a1e0d54fca6ae6bdf`, with tree
`e0539fbf87c9d7f9b67db89a65fdae67715285f8`. Import it at `rune/`.
The signed implementation merge has this amended plan as first parent and that
unchanged Rune commit as second parent. Locate the integration SHA from the merge
itself or its receipt, avoiding a self-referential SHA inside its own tree.

Preserve all 4,986 source entries and original history. Compare full recursive
tree listings, including modes and blob IDs; this also excludes ignored build
outputs. Require no changes outside the prefix relative to the first parent.
Retain all six license/notice entries found in the anchored source inventory.

## Evidence and restoration

The isolated dry-run verified both source bundles, restoration into empty bare
repositories, object integrity, anchor signatures, both parent ancestries and
the signed pure merge. Adding a synthetic imported file changes the subtree
and is rejected by the equality check. The original non-bare restore attempt
failed before integration and remains recorded; it was corrected explicitly.

Recreate the signed merge after amending this unpublished plan to import-only.
Repeat signatures, ancestry, prefix equality and outside-prefix checks. Preserve
a bundle of the amended rnx anchor as well as the already verified Rune bundle,
and restore it independently. The canonical engine source mapping is the merge
second parent, its tree, and `rune/`; original Rune objects stay unchanged.
Do not add provenance files to the pure merge outside its imported prefix.

## Boundaries

No Cargo manifest, dependency pin, lockfile, profile, source path or verifier is
rewired in this record. Those changes and their compatibility gates move to
ADR 0211, with a separate signed plan and implementation. The final migration
retains independent adapters where needed and preserves compiler recipes; this
import alone is not evidence of compatibility after that future rewiring.

Integrate locally after proof review. Do not push or archive as part of the
local import authorization. ADR 0209 subsequently enforces one plan and one
implementation per record, with grandfathered pre-0206 historical cutoffs.
