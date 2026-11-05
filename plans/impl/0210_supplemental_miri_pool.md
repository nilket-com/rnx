# Implementation: ADR 0210 supplemental Miri pool

Status: Accepted

## Sequence

Execute 0210 after the published 0206 follower and benchmark report, before
0208 monorepo migration and 0209 ket checker port. Record numbers identify
decisions, not execution order. Keep 0208 and 0209 reserved for their distinct
approved scopes.

## Surface and identity

Add a committed supplemental manifest and one runner under tools/xtask. Expose
new-run and explicit-resume modes; use the existing canonical format/Clippy
prerequisites and leave the canonical 98-row scheduler unchanged. Use a
stdlib-only Python orchestration layer if appropriate; do not add runtime
crate dependencies. Preserve the nightly-2026-09-01 toolchain, locked dependency
resolution, native-code and execution feature configurations, and the seven
current selectors. Validate the exact selected test inventory from prepared
harness listings and retain ignored-test accounting. Zero selected tests fail.

Bind receipts to the source tree and revision, Cargo.lock, tool identities,
manifest, runner, flags and controlled environment. Publish each receipt by
fsync, atomic rename and directory fsync. Hold an exclusive invocation lock.
Interrupted work resumes in a new attempt directory with earlier logs retained;
completed FAIL cannot be silently retried. Previously committed PASS results
are rehashed before reuse. No cross-source reuse exception is added.

## Preparation and resources

Prepare one build per feature configuration before releasing interpreters.
Use public cargo miri with a shared Cargo home and isolated copies of prepared
targets. Prove there are no build-directory lock waits or rebuilds. Measure and
report package-cache startup lock waits in each receipt; prove that interpreter
runs overlap rather than serialize. Do not depend on the private cargo-miri
runner interface. Keep mutable outputs isolated. Record detected affinity, available
memory and ancestor cgroup headroom; derive a bounded worker count and launch
MemoryMax from those observations with an explicit reserve. A caller override
can only reduce concurrency. Admission must fail before launching workers if
one interpreter cannot fit. Calibrate the declared interpreter memory
reservation from retained peak evidence or a bounded preparation/control run;
never infer available memory from CPU count alone.

## Validation and publication

Run formatting and runner controls before expensive work. Controls cover
preflight failure releasing no jobs, duplicate/missing selection, changed
identity, corrupted logs, atomic receipts, completed failure refusal, and
interruption with successful subsets reused exactly once. Confirm real Miri
process overlap and correct final coverage in a fresh checkout, with the
per-subset terminal receipts retained. Preserve individual bounds; no flake
retries. Review signed implementation and exact-source evidence before normal
fast-forward publication. No rnx pin is required for a tooling-only record.

## Implemented evidence and limits

All seven subset probes passed. Kernel traces proved four-way and three-way
interpreter overlap, without rebuilds or build-directory lock waits. Maximum
observed individual-descendant RSS was 3,288,178,688 bytes. Admission reserves
5 GiB per worker (rounded to 4 GiB plus 25%), with a 2 GiB host reserve and
CPU/ancestor-memory bounds. RSS is not aggregate process-tree memory.

N5 exact partitions passed in 3391.32 and 402.10 seconds. One individual test
dominates; sharding does not promise to eliminate that floor. N5, method
closures, and budget errors use singleton exact-name shards, including ignored
tests. Other subsets retain verified exact-name batches. Cold acceptance reports
per-shard timing and ignored status; future balancing can use those observations.

Artifact files and nested directories are fsynced before receipt publication.
Unique same-directory temporary files prevent stale temporary-file collisions.
Controls include real process interruption/resume and durability fault injection.

## Acceptance recovery

The first cold tooling acceptance found a copied-harness incremental-cache race.
Captured absolute incremental paths mutated shared preparation while later copies
read it. Retain the raw FAIL. Disable incremental compilation, reject captured
incremental arguments, and rehash preparation after execution. Approved recovery
is explicit tooling-short acceptance: fresh preparation, two real concurrent
subsets, isolation/kill-resume/identity controls, and measured overlap. Scope is
bound into receipts and identity; the full default is unchanged. This acceptance
does not claim that the initial incomplete full run passed.
