# Implementation plan 0240: Repository data cleanup and history purge

Status: Accepted

Paired ADR: ../adr/0240_repository_data_cleanup.md. Phase 1 implemented; validation pending.

1. Pin cd7e36e9. Inventory every tracked non-source file and every file over
   16 KiB in rnx and rnx-bench: path, size, duplicate group, and reader
   evidence (direct paths, globs and directory discovery, generated paths,
   manifests and config, include_bytes/include_str, build and packaging
   tooling, hand-run tools and examples). Classify each file per the ADR;
   mark anything unsettled as unresolved.
2. Inventory record-specific tooling by dependency closure from every live
   entry point, including the 0239 gate: live, archive-only reproducer, or
   unused.
3. Bring the inventory, classes and dispositions for review before deleting
   anything, with the proposed consumer and generator manifest.
4. After review: build the archive (repository bundles, removed files,
   checksummed index) outside git and verify that it restores and reads.
5. Apply deletions, dedupe, generator replacements with equivalence checks,
   compact output for the generators that stay, tooling retirement, report
   locators, and the hygiene rules with controls.
6. Run the required stages for the touched paths, preflight and the commit
   checker; bring the phase 1 packet.
7. After publication: phase 2 manifest and dry run on fresh mirrors of rnx
   and rnx-bench; review; the user force-pushes.

## Required controls

- Hygiene: unreferenced data fails; a Markdown-only citation fails; an
  indirect reader through a glob passes; a dead reference (named but never
  opened) is caught by the manifest check; configuration files are source;
  a duplicate over 4 KiB fails; a 1 MiB file added against the baseline
  fails unless allowlisted.
- Every test that read a removed or regenerated file passes, offline, from a
  fresh clone.
- Archive: restore from bundles and index; checksums match.
- Phase 2 dry run: commit count and topology unchanged; empty commits kept;
  HEAD tree identical; no removed blob is reachable from a retained path;
  final commit map verified after re-signing.

## Evidence discipline

Commit the tools and tests. Scratch receipts under target/0240; the archive
lives outside both git and target/. Message Claude only for design questions,
gate failures and the packet, as requests.

Results: pending. Publication: not authorized at draft stage.

Reviewed implementation boundary: enforce duplicates on data only. Retain
exact reasoned duplicate groups for immutable imported records, separately
packaged assets and the unresolved 0072 default/versioned sample outputs.
Use a new archive catalog; historical plans remain untouched. Keep uncertain
fixtures and scorer/test dependencies. The large-data rule also checks an
existing path crossing from below 1 MiB in the plan baseline to at least
1 MiB now, unless explicitly allowed.

Archive restoration verified both repository bundles and every tracked
regular file against the inventory. rnx removes the reviewed historical
measurement files, compacts the two active Polars JSON outputs without
changing their values, and updates their generators. No runtime source
changes. Benchmark cleanup follows in its own signed follower pair. Final
gate receipts and exact removal/retention lists belong in the packet.

The first adapter-suite run passed its tests but exposed producer receipt
binding before declared oracle-output cleanup. Move the existing validated
cleanup inside the suite producer before publication; preserve all failures
and test the ordering. The failed wrapper receipt remains in the packet.

The second validation stopped on a pre-existing Polars preview Clippy deny.
Share the existing Polars lint arguments between both Clippy runners, retaining
exact diagnostic multiplicity checks. Other stages retain their lint policy.
Final validation uses a fresh offline clone of the amended implementation;
retain both earlier failed receipts without relabeling them.
