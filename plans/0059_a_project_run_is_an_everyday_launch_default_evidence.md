# 0059 stage two: an ordinary launch checks inputs, then trusts matching output

Ready for Linux review after accepted stage one at rnx 347acd8 and rnx-bench
9c058e7, both pushed. Gates 3–6 are exercised here. The source/dependency content
checks remain, the artifact default changes as accepted, and --verify retains a
full content check. No root runtime, manifest/lock, notice, adapter, server or
kernel change accompanies it.

## The end-user result

Same generated Polars artifact, entry, source inputs and public locks; ordinary
release tools without counters, pause hooks or internal timers. Two fixed-seed
interleaved repeats of twenty samples per workload/product, one pinned CPU and
one Polars thread. All 400 observations retained with exact output/status/stderr
checks. Median milliseconds:

| Launch | Init | Whole tiny pipeline |
| --- | --- | --- |
| Old tool | 149.90 / 149.99 | 154.58 / 154.73 |
| Accepted stage one, full hash | 90.52 / 90.57 | 94.92 / 94.86 |
| New default | 24.45 / 24.50 | 28.84 / 28.86 |
| New --verify | 90.60 / 90.67 | 95.07 / 94.95 |
| Generated-direct | 6.51 / 6.52 | 10.80 / 10.78 |

The default is about 81% below the old product launch, and about 18 ms above
direct in each repeat. Both declared gates pass: at least 70% improvement and
no more than 25 ms over direct. The pipeline itself is unchanged. This is a
warm Linux CLI result, not a notebook-cell or machine-independent latency claim.
Explicit full verification is about 95 ms and is shown with the default.

Old tools require a valid v1 receipt; new tools receive a valid v2 receipt for
the same artifact. The driver prepares those private bytes outside timing. It
never includes repeated migration in the ordinary default numbers. Fresh output
directories and lock/build are also outside timing. Spawn/capture/exec/wait are
inside. Every sample is also in the journal, checked equal to samples.json.

Five separately reported observations per transition give medians of 98.24 ms
for v1 migration, 98.25 ms for a metadata-mismatch full verification/refresh,
and 77.34 ms for first override establishment. The override has no declared
native-source inventory, so its establishment cost is not a generated-project
migration measurement. These are full-check first-use costs, not free operations.

## What changed in the product

The private artifact module issues a Checked value only after metadata equality
or full digest verification. Command construction consumes that capability;
the older assembly probe's entry still obtains it by full verification. Overrides
no longer hash incidentally in verify_inputs and again in command construction.
Source/native inputs and wrapper identity still use the original checks.

The stamp contains size, full available mtime seconds/nanoseconds, executable-bit
state, and Unix device/inode. Inspection uses a non-following, non-blocking regular
file open and handle metadata, with the existing artifact size limit. A metadata
match reads no payload. A mismatch or --verify hashes against the already-known
digest, then checks that the opened file's before/after metadata and current path
identity agree. A changed stamp is atomically published before launch; mismatch
of contents refuses rather than changing the expected digest.

The shared fingerprint reader supplies its opened-file metadata alongside the
content record, so full verification does not certify metadata from a different
open after hashing. A final metadata recheck precedes receipt refresh after its
failure/pause boundary. This detects ordinary changes across those boundaries;
it is not an atomic filesystem snapshot or race-free path-based exec guarantee.

Build publishes receipt v2 only after fully verifying the installed artifact.
Valid v1 generated receipts migrate after one full check; missing generated
receipts still require build. Overrides can establish a receipt from their
locked digest, including after relocking or deleting an old private receipt.
Malformed receipts, unsupported versions, invalid fields and generated lock
mismatches refuse. --verify is accepted once in either order with --manifest,
only for run and before --; after -- it is passed to the script.

The help and README state the intended user default: checks your sources,
trusts your build output unless you ask it to verify. The same-size in-place
artifact edit with restored/indistinguishable mtime is a documented miss, along
with manipulated metadata/receipts and identity reuse. A source edit with the
same characteristics is still caught by its content fingerprint. No source
cache, ignored-input expansion or input-set narrowing was introduced.

## Coverage gates

The two tool suites each pass 37 tests, zero failures; two earlier assembly
integrations remain ignored and are not counted as executed. The new inert-file
unit cases demonstrate both default acceptance and forced-hash refusal for a
same-size restored-time change without executing corrupted code. They also cover
replacement with copied mtime/size and different inode, executable-bit changes,
pre-epoch nanosecond times, malformed/versioned receipts, non-regular files and
oversize artifacts. Old fingerprint/tree/allowance tests still pass.

The separate product contract fixture passes seven groups. A path-specific byte
counter in test-support proves zero artifact bytes for an unchanged default and
exactly the file size for --verify, legacy migration, mismatch and override
establishment. Positive full-read controls prevent a missing counter from passing
vacuously. Flags/forwarding, missing/malformed/stale receipts and identical versus
different replacements run through the real tool. A restored-time source edit
refuses before artifact reads. Production builds contain no counter/pause code.

Publication failure and SIGINT before receipt refresh preserve the prior receipt,
with no script output. Modifying the artifact at a pause after hashing causes a
refusal before any fresh receipt is published. Every paused child has timeout,
release/kill and reap cleanup in the fixture. These hooks are observation points,
not a performance path or a claim to interrupt synchronous hashing mid-read.

The unchanged sixteen-group 0057 workflow passes, including publication failure,
failed build, edited inputs, relocation, interrupts, overrides and no Cargo/rustc
on run. The real PostgreSQL project lock/build/run passes with the mapped query
and private-cluster cleanup. Both old result directories were restored. The
ordinary Polars application runs throughout the performance gate.

## The remaining inventory cost, now subdivided

An isolated copy adds disjoint child timers plus an inclusive native-total timer.
Twenty paired profile/control init launches agree at 23.92 versus 23.86 ms:

| Native inventory group | Median ms |
| --- | ---: |
| Git enumeration and subprocess work | 5.86 |
| Native tree metadata/open/read/hashing | 10.09 |
| Ancestor/config audit | 0.21 |
| Inclusive native total | 16.30 |

The parent is not added to its children; their small remainder is package/setup
work. Git is not the majority. The tree bucket is not pure hashing throughput.
This fixture and its input size do not establish costs for every project. No
optimisation or reduced coverage of those inputs is smuggled into this record.

## Checks, artifacts and qualifications

Formatting, strict all-targets Clippy in both configurations and notices pass.
The tool's x86_64-pc-windows-msvc all-targets check passes. Windows execution is
not claimed; its existing command refusal remains. Manifests, lockfiles, notices,
root source/tests, adapters, server and kernel are unchanged, so no stock-rnx
startup change is claimed or retimed. The only README edit is within the tool.

rnx-bench/probes/project-run-default contains reproduction scripts. Raw checks,
contracts, old workflow/Postgres logs, all samples/journal, transition observations,
inventory instrumentation/source, hashes and public lock snapshots live in
results/project-run-default-0059. The measured source patch is pinned separately
from subsequent documentation; local locks must be refreshed for a later tree.
This stage changes the accepted default coverage exactly as the plan states;
it does not purport to detect hostile mutation of user-owned build output.
