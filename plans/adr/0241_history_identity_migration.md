# ADR 0241: Restore executable identities after the history cleanup

Status: Accepted

Base after user cutover and re-clone: rnx
b2fa284ebd57f7b92ce260ba0646c22ced57653b; rnx-bench
6b608fbad00fdc63fa50fed83de928e995b4939a.
Cutover and fresh clones verified. The plan is local pending implementation
review and the complete pair's ordinary restored checks.

## Goal

Restore executable history checks after ADR 0240 without weakening their
rules or relabeling original measurements. No runtime, dependency, fixture
value or performance-policy changes. One local plan and one implementation;
publish neither until the pair passes the ordinary restored checks.

There are no commit hooks requiring a new bootstrap tool. The signed plan
stays local while the implementation restores preflight. No interim PASS is
claimed, no mapped-policy bootstrap verifier is introduced, and ordinary
publication stays frozen until the complete 0241 pair passes its gates.

## Authoritative, compact commit map

Use only the accepted final signed two-repository map:
- FINAL-MAP.json SHA256
  d39743e15f91dd07bc2693ca3a2a739e24575b33a5a29673aba589a53193ac50.
- PACKAGE.json SHA256
  9ba38d951dff43ae1967e491f13614d12600a1b477160890aaabde9f0ee748bd.

Commit only the rewritten pairs: 342 rnx rows and 281 rnx-bench rows, each
old/new, plus schema, source hashes, old/new tips and counts. No unchanged
rows or signature-status dump: those proofs stay in the archive. Verify the
projection exactly against the accepted full map. Exclude 0241's own commits.
Treat the compact map as reviewed source configuration in the data manifest.

A historical commit absent from the map maps to itself only if the loader
proves membership in the unchanged ancestor closure of the first rewritten
commit's parents or the preserved imported Rune graph. Fail closed otherwise.
Use repository namespaces and require unique resolution for short prefixes;
do not guess unknown identities or blindly pass them through.

Translate reviewed executable Git commit identities only. Git blob IDs,
source/binary/receipt hashes, external revisions and historical provenance
are different types and stay unchanged. No global textual replacement.

## Executable consumers

- ci/commit_subjects.py: map our changed cutoffs, import/follower identities;
  preserve unchanged imported identities, metadata, sequence, signature,
  merge-proof and MAX_IMPLEMENTATIONS=1 rules.
- ci/hygiene_history.json and ci/hygiene.py: map the baseline. Retire active
  exemptions only for exact purged path/blob pairs proved by 0240's manifest.
  Keep surviving document blob hashes. Missing-baseline checks stay strict;
  reintroduced forbidden content at a retired path must fail.
- tools/column/clippy.py and live Git/build lookup consumers: map lookup
  commits; preserve diagnostic and recipe rules. Audit values loaded from
  manifests/CLI, not only literal hashes.
- tools/refresh_census.py and tools/verify fixtures: map enforced lookup
  revision pairs, retaining source fingerprints, witness values and registry
  rules exactly. Provenance-only fields remain original.
- Fixed-route anchor: apply the narrow same-anchor rule below.

For every historical replayer/pair-pinned scorer listed in CONSUMERS.md,
trace required-stage and test execution. Exactly two dispositions:
(a) executed on current history: it is live; translate its lookup commit;
(b) not executed: leave bytes unchanged and list as a deletion candidate for
    a later cleanup record. The archive already retains it.
No annotation, retargeting or evidence-envelope project for historical-only
tools. Merely finding a hash or importing a module does not prove that its
old-history lookup is executed; record the actual reader/test path.

Archive catalogs in both repositories retain original bundle revisions,
INDEX checksums and file/blob identities. They locate original archived
objects and are not stale current-checkout pins. Imported/external revisions,
comments, historical plans and measured report labels remain unchanged.

## Existing anchor transition and ordinary recapture

User-approved anchor transition
Old anchor: 3fe44bf9245c780cf784554293e0a1b1e7c00c26
New anchor: 48fe3b15f48c92be91f04aa97f1f2f3fc31cf860

The user approved this transition before cutover. No alternative approval
format or new validation mechanism is introduced.

Use the existing anchor_transition/anchor_approval mechanism unchanged:
- Old anchor: 3fe44bf9245c780cf784554293e0a1b1e7c00c26.
- Mapped anchor: 48fe3b15f48c92be91f04aa97f1f2f3fc31cf860.
- fixed_route_policy.json anchor becomes the mapped commit; anchor_transition
  has old/new above, plan = the signed 0241 plan commit, and path = this ADR.
  The baseline policy still contains the old anchor, satisfying existing
  lineage checks. The plan hash is installed in the implementation, not
  self-referenced inside its own commit.

No identity bridge, comparison whitelist or new envelope. No changes to
reuse_anchor or capture_key, and no --import-anchor. The key changes with the
anchor identity; run the gate's ordinary cache-miss path for one mapped-anchor
build and capture after the required approval and cutover. Keep thresholds,
kernels, sizes, modes and source inventory untouched.

The rename proof is a 0241 validation control, not production bridge code.
Preserve the old anchor capture, binary and referenced artifacts outside the
clones before re-cloning. Compare new binary_sha256 to the old capture's and
every measured record by kernel/mode/size/Ir, requiring exact record equality,
not merely a band PASS. If the binary differs, report the differing bytes
and whether all records remain exact. If ANY record differs, STOP and return to the user:
this would no longer be a pure rename. Retain both original and new receipts
with their true revision identities; overwrite neither. Cumulative drift
continues against the same measured numbers.

INITIAL_ANCHOR is read only when a baseline lacks a policy file. Translate
it only if a current required check/test can still reach that branch;
otherwise leave it unchanged and record that disposition.

## Thin citation resolver

One small read-only command over the committed map takes repository plus an
old or new commit (full, or unique prefix) and prints both identities. For an
optional path, report whether the mapped commit contains the same blob; if
that cannot be proved or the path was purged, point to tools/archive_data.py.
Do not implement new archive reading, fetch objects implicitly or restore
files into a checkout. Do not infer blob equality from the map alone.

Keep historical plans and receipts byte-identical. Add one pointer from
ARCHIVED-DATA.md. No mass citation edits and no new benchmark figures. If
rnx-bench needs a live-consumer change, use a reviewed follower pair citing
the signed rnx plan; otherwise leave that repository untouched.

## Acceptance

- Reviewed per-consumer execution/disposition inventory, including dynamic
  lookups; lexical hits are candidates, not an automatic edit list.
- Exact 623-pair map projection; repository/prefix and unchanged-closure
  controls, missing/duplicate/corrupt map rejection. Signature proofs remain
  in the accepted archive rather than duplicated in git.
- Normal commit checker, hygiene, controller controls and actual path-derived
  required gates PASS for the complete pair on a fresh rewritten clone.
  Cross is required for census/verify changes. No unrelated runtime/Miri
  reruns absent source changes or required-stage triggers.
- Existing signed-plan anchor-transition approval passes without new bridge
  code. The mapped-anchor capture passes the exact rename control above;
  changed inventory/limits or any record difference must not be hidden by a
  drift-band PASS. User approval precedes plan commitment and capture.
- Thin resolver controls: old/new identity, wrong repository, ambiguous
  prefix, retained identical blob, changed blob and purged path.
- Retired exemptions cannot grandfather forbidden content; surviving blob
  exemptions, historical measurements, archive catalogs and report tables
  remain exact. Baseline-to-implementation runtime source/lock diff is empty.
