# ADR 0210: Durable supplemental Miri pool

Status: Accepted

Seven supplemental Miri subsets have accumulated outside the canonical verifier:
recursive native glue, method closures, canonical payload scanning, column
execution, portable column primitives, ambient budgets and terminal budget
errors. Their target-directory launch scripts execute sequentially and do not
provide a committed, identity-bound resume protocol. The canonical verifier
already runs its 98 Miri rows in parallel; this decision does not replace it.

Make the supplemental subsets committed manifest-driven jobs. Prepare each of
the two feature configurations once, then execute independent subsets in
isolated target directories or prepared harnesses without shared Cargo locks.
Select concurrency from available CPUs and memory, including ancestor cgroup
limits and a declared per-interpreter reservation. Record admission observations
and enforce the derived process-group memory ceiling at launch. Keep one
libtest thread per subset and the existing pinned nightly and feature choices.

Formatting and Clippy prerequisites precede expensive preparation. A cold run
starts with empty project targets. Successful prerequisites, preparation and
subsets receive atomic durable receipts, including immutable log hashes and
complete source, toolchain, manifest, environment and command identities. Reuse
only verified PASS receipts with matching identity. A changed source or missing
artifact cannot reuse success. Completed failures persist and are never retried
implicitly. An interrupted invocation may explicitly resume uncompleted work;
archive incomplete attempts and distinguish interruption from a completed
failure. Record attempts so resume cannot hide work or erase evidence.

The runner exits only after its child processes exit and emits a terminal
receipt. No polling or progress watchers are introduced. Tests must establish
actual interpreter overlap, exact subset coverage, stop/resume behavior,
identity mismatch refusal, failed-receipt persistence, and atomic publication.
A passing canonical suite cannot substitute for missing supplemental jobs.

This is tooling only: no Rune execution semantics, test expectations, Miri
selection, acceptance thresholds or canonical row scheduling changes.
