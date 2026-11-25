# ADR 0338: The final builds once and cleans up

Status: Proposed

Base: published b8d5ad340653623cc5e1855bc4d2c85ce00ccb8f (ADR 0337).

## Problem

Since ADR 0333 a complete final first prepares a checkpoint of the
published baseline, outside its clock, and then builds the candidate
on top of it. Most of the tree depends on Rune, so a change there
rebuilds most of it again. That is two builds for each final.

| Final | Preparation | Timed run | Result |
|---|---:|---:|---|
| 0334 | 2,030 s | 3,591 s | pass |
| 0335, second | 3,718 s | 2,743 s | pass |
| 0336, second | 3,932 s | 2,042 s | fail |
| 0336, third | 3,851 s | 2,802 s | pass |
| 0337, first | 2,289 s | 347 s | fail |
| 0337, second | 3,942 s | 3,047 s | pass |

In these runs preparation took at least 2,030 seconds, and over an
hour each time it followed a failed final. The four passing finals
took 94, 108, 111 and 116 minutes from launch to verdict. A final from
empty build state, the first of ADR 0323, passed all 107 stages in
68 minutes and failed only its one-hour limit.

The checkpoint also brings a store of about 113 GB, a disk floor
for it, three preparation modes, a rule against warming caches, and
snapshots of the developer's caches. All of it exists so that a
one-hour clock starts from a known state. A build that starts empty
and removes what it built has no state to know.

No stage's verdict needs compiled baseline output to survive from
one final to the next. Two comparisons need the baseline itself:
the lint comparisons, which run on the baseline's source, and the
fixed-route gate, which measures the baseline's images.

## Decision

1. A complete final builds from empty state. It creates one output
   directory for the run and owns it alone. Every build output,
   archive, scratch file and temporary file of the run is inside it,
   and so are the run's private home and tool directories. The
   memory-backed paths that the contract and census stages use today
   stay memory-backed, on mount points private to the run. Receipts,
   failure evidence and verified gate inputs are kept and live
   outside it.
2. Nothing is built before the clock, and nothing built is carried
   to the next final. The checkpoint store, its preparation, and
   the promoted and restored modes are removed.
3. The lint comparisons run their baseline side from the baseline's
   source, inside the run's directory.
4. The fixed-route gate may reuse the published baseline's gate
   inputs: the runner, the installed helper, the engine image and
   the anchor. They are kept by commit and never change. Each use
   verifies recipe, toolchain, digests and measurement policy. A
   missing or mismatched input is rebuilt inside the clock. A
   publication that changes only documents carries forward the
   tested candidate's inputs under the existing diff proof.
5. Downloaded dependency sources may stay between runs. Registry
   archives are checked against the lock file's checksums and git
   sources against their resolved commit and tree. Whatever is
   missing is fetched inside the clock. Each run has its own
   writable Cargo home. It shares no unpacked source tree and takes
   no Cargo configuration, wrapper or build output from the
   developer's account.
6. Reuse inside one run is Cargo's own. The stage groups that share
   a target directory today keep sharing one, inside the run's
   directory. Each needed combination of source, profile and
   features is built once, and its binary serves discovery and
   execution. There is no cache layer and no generation named by a
   hash.
7. The clock starts at entry, before any fetch or admission. The
   run ends by copying out its failure evidence, draining its
   children, cleaning each target it owns, and removing its
   directory. It does so on a pass, a failure and an interrupt, and
   it removes only directories it claimed. A directory left by a
   killed run is recovered at the next entry. The clock stops after
   the cleanup, and the verdict includes it. Gate inputs built in a
   run become reusable for their commit only when that commit is
   published; on a failure they are discarded.
8. The time limit is measured. This record's reviewed candidate, and
   no other, runs its complete final without a wall limit. Every
   other deadline still applies: stages, interrupt and cleanup. The
   limit in seconds becomes 60 times the ceiling of 1.25 times the
   elapsed seconds over 60. That run is this record's final if its
   checks and its cleanup pass. The commit that publishes the
   Results also publishes the receipt's digest and the derived
   limits in a record the tool reads. A later final with that record
   missing or changed is refused. There is no flag to run without a
   limit.
9. The disk check covers each filesystem the run writes, sized from
   the measured peak plus one quarter. The peak is the largest
   additional space observed on a filesystem during the run,
   sampled from bootstrap through cleanup, with paths on one device
   counted together and memory-backed space counted apart. The
   census's per-case memory-backed mounts keep their fixed caps and
   the controls that fill them; their peak is reported and no free
   space is demanded of them. The measuring run keeps a live guard
   against low space. There is no floor for a permanent store.
10. Removed with the store: lane marks and sweeps, the snapshots of
    the developer's caches, and the rule against warming caches.
    Kept: sole ownership of the output directory, the private home
    and temporary paths, the guardian and its drain on interrupt,
    the checked identity of each artifact between its producer and
    its consumers, the receipts, the meter's pinned loader inputs,
    and ADR 0335's early start of the longest chain.
11. Receipts of earlier finals still validate, read only. A new
    receipt must carry the cleanup evidence.
12. The old store is deleted once, by agreed path, after the
    candidate's source review and before its final. It holds only
    build output that the old tool rebuilds, as it did after every
    failed final, so nothing is lost if this record is abandoned.
    The measuring run then also shows that nothing reads it. Check
    the capacity before launch.
13. Scope: the final's tool and its controls. What each stage
    checks doesn't change. Any other mode of the tool follows this
    design or is removed. A publication that changes only documents
    still succeeds its tested candidate.

## Controls

1. A passing run and a run with a planted failure each leave no
   output directory, and a second run starts empty.
2. An interrupt drains the run's children before its directory is
   removed.
3. A directory beside the run's own, owned by someone else,
   survives the cleanup.
4. A gate input from another commit, recipe or toolchain is refused
   and rebuilt. A missing one is rebuilt inside the clock.
5. The receipts of ADRs 0333 to 0337 still validate.
6. A receipt without cleanup evidence is refused.
7. The selected stages and recipes equal the accepted catalogue of
   107, and targeted controls show a stage's pass and failure reach
   the verdict as before.
8. Nothing that this run created remains in the shared temporary
   directories. A file someone else creates there during the run
   doesn't fail it.
9. A directory left by a killed run is recovered at the next entry.
10. A final with the limits record missing or changed is refused.
    A commit other than this record's candidate can't run without
    a wall limit.
11. One complete final, which is the measured run of Decision 8.

## Constraints

- No new third-party crate. Tabs.
- No second run to measure and then accept. One run does both.
- No waiver of a failed check because the run was a measurement.
