# ADR 0240: Repository data cleanup and history purge

Status: Accepted

Base: published cd7e36e9 (ADR 0239).

## Problem

The user does not want data stored in git; code that regenerates data is
preferred (2026-11-10). The repository holds a lot of it:

- At HEAD, 608 tracked JSON files (45.7 MB) and 45.3 MB of gz, tsv, npy, npz
  and csv files. probes/ alone has 22.8 MB of JSON, of which about 1.3 MB is
  named by any code. Twelve JSON files are byte-identical duplicates.
- Generators write pretty-printed JSON (polars-gen surface.json and
  oracle-results.json, the gap_hunt case files), inflating line counts.
- A fresh single-branch clone of main is 66 MB; probes/ history is 28 MB of
  it. rnx-bench clones at 83 MB, almost all results/ data.
- Nothing stops new data from being committed.

Deleting files at HEAD does not shrink a clone. The user accepted a history
rewrite and force push: there are no outside users yet.

## Classes of committed files

Every tracked non-source file, and every file over 16 KiB, gets one class:

- Source configuration: manifests, lockfiles, policy and pin files, gate
  policy, small tables code is written against. Not data; kept.
- Live fixture: read by committed code, directly or indirectly (path
  strings, globs and directory discovery, generated paths, manifests and
  config, include_bytes/include_str, build and packaging tooling), including
  supported tools and examples run by hand, not only required stages.
- Reproducible fixture: a live fixture a committed generator rebuilds,
  semantically equal and available offline from a fresh clone, within the
  budget of the stage that needs it.
- Measured evidence: benchmark results, receipts, captured outputs and
  probes. A script existing does not make them reproducible: rerunning gives
  new measurements, not the same ones.
- Unused: no live reader after the evidence above.
- Unresolved: anything the evidence does not settle. It stays, pending review.

A path appearing in a string is evidence, not proof of a live reader, and
its absence is not proof of none. The checker reads a small explicit
consumer and generator manifest rather than guessing.

## Decision, phase 1 (a normal plan and impl at HEAD)

1. Archive first. Before deleting any measured evidence or unused data, copy
   the current repositories (rnx and rnx-bench, as bundles) and the files
   being removed to an archive outside git, with a checksummed index, and
   verify that it restores and reads. No measurement is rerun to replace
   archived data.
2. Delete unused files and measured evidence from HEAD. Published reports
   and plans that cite removed evidence keep an honest locator (the archive
   index entry) or say the evidence was retired.
3. Replace reproducible fixtures with their generators where equivalence and
   offline availability are shown; keep the rest as live fixtures.
   Third-party fixtures keep their license and attribution files, also when
   archived.
4. Remove duplicate files; readers use one copy.
5. Switch remaining generators to compact output (one JSON value per line
   where a file holds records) and regenerate. Do not compact or regenerate
   files being deleted.
6. Retire record-specific tooling by dependency closure, not by absence from
   a current stage: a live transitive dependency (including of the 0239 gate)
   stays; an archive-only historical reproducer is archived, then removed;
   truly unused code is removed.
7. Enforce in ci/hygiene.py, run by preflight, using the consumer manifest:
   - a data file over 16 KiB must be a live fixture with a manifest entry, or
     on an allowlist with a reason;
   - duplicate blobs of 4 KiB or more fail;
   - a data file of 1 MiB or more added relative to the record's baseline
     (the parent of its plan commit) fails unless allowlisted.
   Unit tests cover each rule, indirect readers and dead references.

No runtime behavior changes.

## Decision, phase 2 (an operation after phase 1 publishes)

1. Manifest. Write an explicit manifest of refs and of the path and version
   removals: blobs of paths absent at the new HEAD that are classed as data or
   evidence, and blobs over 1 MiB absent at HEAD. Check every removed blob
   against retained paths; a blob shared with a kept path stays.
2. Preserve structure. Commit count, parent topology (including the Rune
   history import), empty plan and impl commits, and the HEAD tree stay
   exactly the same. Scope is main; tags, other branches and imported
   upstream commits are listed and each one's treatment is stated.
3. Bundle first. Take an immutable pre-rewrite bundle of each repository,
   which also preserves the original signed objects.
4. Re-sign our own commits from the first rewritten one onward. The new
   signatures attest the rewrite, not the original history. Verify the full
   old-to-new commit map after re-signing, not the intermediate filter map.
5. Dry run on a fresh mirror, review, then the user force-pushes. Every
   clone (this host, Codex, the chatd pair, micro) re-clones afterward.
   rnx-bench follows the same steps in the same window.

## Decision, hash references (a separate record after phase 2)

Executable consumers of commit hashes, including ci/commit_subjects.py
cutoffs and follower pins, ci/hygiene_history.json baselines and blob
exemptions, and the fixed_route_policy.json anchor, are migrated by a normal
plan and impl record after the rewrite. That record commits the final
old-to-new map (excluding its own commits). Commit identities translate
through the map. Source, binary and receipt hashes are never rewritten, and
historical measurements are not relabeled as if taken on new commits: capture
evidence either has an explicit source-equivalence bridge or stays anchored
to the archived original. Every executable hash consumer must pass after the
migration.

## Decisions for the user before phase 2

- Re-signing our commits in the rewritten range: recommended.
- The migration record above for hash references: recommended.
- rnx-bench results: confirm the archive is enough and none must stay in git.

## Acceptance

Phase 1: required stages for the touched paths, the hygiene rules with
controls (indirect readers, dead references, configuration classed as source,
duplicates, the 1 MiB rule against the baseline), generator equivalence for
anything regenerated, an archive restore check, preflight and the commit
checker. The packet lists every file with its class, reader or reason, the
archive index, and HEAD size before and after.

Phase 2: the dry-run report shows the manifest, shared-blob checks, identical
topology and HEAD tree, empty commits kept, the verified final commit map,
signatures on re-signed commits, and clone sizes before and after.
