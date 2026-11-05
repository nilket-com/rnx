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

## Reviewed dependency and profile choices

Resolver 3 preserves the shipped root CLI choice; standalone engine resolver 2
drops out. Engine standalone tests move from TOML 1.1.8 to TOML 1.1.6, which the
shipped CLI already uses. Keep CLI serde_spanned/parser/writer versions unchanged.
Restore other incidental tool resolutions to the anchored engine lock versions.

Named engine-dev/release/bench profiles were compared using Cargo 1.98.1 unit
graphs for the default Rune library recipes: 65/73/73 units, matching target,
mode and feature identities and all codegen/profile fields except profile name.
This proves those recipes, not binaries or every feature/test configuration.
The normalized CLI normal/build dependency versions and features are unchanged.

## Rewiring review surface

The cross-verifier now binds the canonical engine subtree and root workspace
inputs. Historical `--rune-rev` / external `--rune-path` overrides are rejected
explicitly: use the historical standalone host checkout for those comparisons.
The redundant Git-source patch table is removed; resolved metadata is checked
before compilation as well as by the Rust controller.

The imported record checker validates the original Rune boundary separately
from the new monorepo first-parent boundary. ADR 0209 remains the full checker
port. Existing license acquisition records remain historical;
`provenance/0211/engine-licenses.json` maps unchanged imported license bytes.

The Hegel integration workspace is optional and independent; its offline
metadata attempt failed because `hegeltest` is not cached. It is not a shipped
host graph or a member of the root workspace. Its manifest still participates
in the canonical-source guard. Existing independent engine measurement harness
locks and profiles remain unchanged. All active host locks are refreshed.

Reviewed gates (Claude seq 27265): structural guards, controls, formatting and
Clippy first. Build baseline 002584a and the candidate release CLI and Polars
worker on the same host with identical recipes; compare .text and .rodata.
Byte-identical .text permits scoped reuse of published 0206 performance evidence;
report and attribute embedded-path/identity string changes separately. If .text
differs, attribute it and run the column preflight and old-path checks.

Run fresh engine native/execution tests for the changed dependency context.
Run canonical Miri rows plus supplemental tooling-short scope; full supplemental
coverage rides with the next engine product change. Run the existing fresh host
acceptance and cross-verifier to exercise the workspace wiring. Receipts bind
the final signed implementation; publication still requires passing gates.

Host development recipes are checked as well as release recipes. Former Git
Rune dependencies receive explicit opt-level 2 / incremental false overrides
in the root dev profile; engine-dev keeps opt-level 0 / incremental true.
Independent Rune hosts retain incremental false for those path dependencies.
The committed tools/profile_recipes.py compares dev/test/release for all eight
Rune-consuming host workspaces and engine-dev/release/bench against anchored
checkouts. It compares every unit's package/version, target, features, platform,
mode and profile fields (except the intentional profile name). Empty graphs
and changed optimization settings fail controls. Jupyter and the generator do
not link Rune and are outside this engine-dependency recipe comparison.

The host release-metadata fixture now checks canonical source identity and
copies tracked Rune inputs into its scratch workspace. The worker budget and
30-second watchdog stay unchanged. Before host recovery, rerun every completed
stage that compiled or executed dev/test artifacts; only source/metadata/format
and diagnostic-only stages are eligible for scoped reuse. Final binary identity
must bind the measured product inputs; later tooling-only amendments require
an exact source-diff proof before reusing those binaries.

Migration measurements compare the already optimized published engine with the
same engine in the monorepo. The pair-pinned migration scorer replaces CK2's
strict improvement requirement with no growth in the program plus unknown
partitions, retains all raw absolute ceilings, and applies the existing system
library slope and constant limits to every unchanged path. It introduces no
new tolerance. The reviewed memcpy adjustment is limited to one exact branch
in one library for the measured pair: evidence, binaries and profiles are
hashed; caller identity and branch counts are rechecked; unexplained routine
cost fails. Raw measurements and failed receipts remain intact. Old-path checks
use three predetermined repeats and carry no earlier pair's exceptions.

The cross-verification launcher binds both canonical Rune request fields and
validates them before compilation. Terminal notices use an absolute executable;
a control proves that a failed job sends its notice even with an empty PATH.
