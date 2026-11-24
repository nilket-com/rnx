# ADR 0330: Final stages own their scratch space

Status: Accepted

Base: published 0f3df71d4126b6893460a692cfd70bfeefb0a604 (ADR 0329).

## Problem

Two complete finals on the 0328 candidate stopped in final-controls
with the candidate unchanged. The bootstrap tests
(`tools/final/tests/bootstrap.rs`) build a fixture checkout under
`std::env::temp_dir()` and run `final.sh`. `final.sh:50` measures free
space at that checkout and needs 24 GiB.

On this host `/tmp` is a 30.7 GiB tmpfs. Engine and adapter tests leave a
per-process directory there on every run, for example
`adapters/polars/tests/dense.rs:44` and
`adapters/polars/tests/collect_dtypes.rs:34`. 934 such directories had
built up since 2026-11-21, leaving 23.1 GB free. The finals failed on
space their own earlier runs had leaked. The first diagnosis looked at
the root filesystem, because the refusal did not say which filesystem
it measured.

## Decision

1. Each stage gets an atomically claimed private scratch directory outside
   Git, Cargo workspace, Cargo config and toolchain ancestry. When the
   output has such an ancestor, use a private ledger root in /var/tmp
   after validating that location. Pass its absolute path as `TMPDIR` through the real
   launch path to child tools and their descendants. Route controller-
   assigned temp roots too, including `Bulk::Project`, which currently
   reads the controller's own temporary directory before the child starts.
   Keep existing producer outputs available until their consumers drain.
   Declare exceptions by consumer and reason in the catalogue. The 0318
   census keeps its size-capped private tmpfs. Fixed-route observers keep
   their canonical paths and measured environment. The rest of each
   enclosing stage still uses its private scratch.
2. Bootstrap admission is testable. Accept fixture disk bytes only under
   the existing bootstrap fixture guard. Using the disk fixture alone
   marks the handoff as a fixture; the real controller refuses it.
   Production still measures the filesystem holding the bootstrap build
   target. A refusal reports the measured path, filesystem, available
   bytes and required bytes.
3. Tests clean up their own scratch, including on an assertion failure
   or an early error after creation. Use existing helpers or a drop guard
   that owns only a directory the test successfully claimed. Inventory
   the known leaking test families and other writers on the selected
   routes. Keep outputs explicitly requested by the caller. No new crate.
4. Preserve the existing checkout, output and Cargo-target admission
   checks. Account for private scratch on the filesystem that backs it,
   including when scratch, output and build targets use separate mounts. Count
   concurrent demand on a shared filesystem once per allocation. A
   namespace-private tmpfs is memory-backed; its presence alone does
   not require checking the host's /tmp. Keep its existing size and
   memory limits.
5. Use the final's ownership ledger, child draining and release/recovery
   paths. On failure, collect named, bounded, sanitized diagnostics
   before releasing scratch. Successful and failed stages leave no
   unbounded scratch in the published evidence. An interrupted final
   can recover its own scratch without removing a foreign directory.

## Controls

1. Observe an actual stage and grandchild using the owned TMPDIR. Two
   concurrent stages get distinct directories. Exercise the project
   root and verify the census and fixed-route exceptions at their
   consumers. Compare the measured launch environment and paths with
   their existing contracts.
2. Plant a scratch file on success and on failure. Verify cleanup after
   users drain, bounded diagnostics on failure, and recovery after an
   interrupted owner. A foreign directory at the proposed path survives.
3. Exercise above- and below-threshold disk fixtures without filling
   the host. A disk-only override without the fixture flag is refused,
   and a disk-only fixture handoff is rejected by the real validator.
   Pin the path, filesystem and byte counts in the disk refusal. Keep
   each existing bootstrap test's own outcome reachable. Exercise
   shared versus distinct filesystems with controlled free-space inputs.
4. Run the repaired test families under an isolated temporary root and
   check it before the controller removes anything. Include an early
   failure or unwind and a pre-existing sentinel. For the complete
   final compare run-attributable leftovers in shared /tmp; unrelated
   host activity cannot fail this comparison or become a deletion list.
5. Run fmt, Clippy, preflight and the full final-controls command under
   the final's real recipe before signing. Run one complete final on
   the signed candidate and report its duration and scratch outcome.

## Constraints

- No engine, adapter behavior or gate-band change.
- No new third-party crate. Tabs.
