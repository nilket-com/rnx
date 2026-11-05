# Implementation: ADR 0211 engine workspace rewiring

Status: Accepted

## Starting point

ADR 0208 import merge `8bbb90ed64c30e994db9ac2b1ab2bbb83d1b5d52` preserves
published Rune `6d7aa59dee00dd9c5c598b3a1e0d54fca6ae6bdf` at `rune/`.
The engine tree is `e0539fbf87c9d7f9b67db89a65fdae67715285f8`; root rnx runtime
still matches baseline `002584a48d1f714e3856f5a3d6a99ac86c2204eb`. Import proof,
restored bundles, signatures and tamper control passed. Nothing is published
from the migration yet. This plan moves the workspace/profile/guard work out
of the amended import-only 0208 plan.

## Dependency and profile inventory

Inventory all tracked manifests: 29 from rnx and 22 from Rune at these anchors.
There are no package-name collisions, but independent adapter/probe roots and
engine tool assumptions need explicit handling. Classify active recipes versus
historical fixtures rather than automatically rewriting every old manifest.

Capture Cargo metadata, lock versions, dependency/feature edges and compiler
recipes for root rnx, Polars and other active hosts, engine native/execution/Miri,
benchmarks and selected tools. Preserve this baseline before editing manifests.
Set root rnx as default member. Include engine crates explicitly; select tools
and examples deliberately. Preserve the imported workspace manifest as migration
provenance before replacing its active role. Independent adapters may retain
workspace roots but must consume the same canonical engine paths.

Rune uses resolver 2 and bench no-LTO with debug information; rnx release uses
thin LTO and has its own development profile. Choose resolver and named profiles
from comparisons, not convenience. Preserve relevant old build recipes. Avoid
incidental dependency upgrades or feature widening while regenerating lockfiles.
Bring the exact metadata/profile/version deltas to review before long gates.

## Guards, tooling and provenance

Replace active Rune git pins and pin guards, including generated host templates,
with one canonical-source guard over all active lock graphs. Negative controls
must reject an external git source, a duplicate engine source and path escape.
Keep historical manifests and archived receipts unchanged and labeled historical.

Audit root aliases, engine verifier commands and cwd-dependent manifests. Adapt
0210 runner paths and identities without losing scope, failure persistence,
artifact rehashing or memory admission. Preserve original engine history lookup:
the import merge's second parent is the unmodified source anchor. Record the
integration boundary and source-tree map in this record's provenance artifacts.
Update licensing source maps while preserving original license and notice bytes.

Expose build identity with monorepo commit, engine subtree and dirty state; do
not hardcode a revision into the tree that defines that same revision. Existing
public metadata and compatibility behavior remain covered by focused tests.

## Ordered checks and publication

Run structural/source guards, negative controls, Python controls/syntax,
formatting and Clippy before builds or measurements. Review the final gate
inventory and exact rewiring diff before expensive execution. Fresh engine and
host checks, actual CLI behavior and existing-path performance comparisons use
preserved recipes. Unchanged Rust bodies do not prove binary/performance identity.
Any evidence reuse needs a scoped proof, not an inferred blanket waiver.

Keep stage outputs durable and identity-bound; preserve raw failures. Explicit
resume reuses only matching validated PASS stages. Completed FAIL requires
attribution and a concrete correction, never a blind retry. Wait on terminal
notifications and do not poll long jobs. Tooling-only followups use the user's
proportional rule (small real subset plus kill/resume/identity controls); full
coverage rides with the next product change. This migration still has dependency
and compiler-context compatibility requirements.

After all required gates pass, present exact canonical commits, redirect notice
and archive setting for review. Publish normal fast-forward, verify the remote,
then archive the source fork as reviewed. Do not delete it or rewrite old pins.
ADR 0209 follows once the canonical layout is established.
