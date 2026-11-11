# ADR 0253: a final releases the build output it owns

Status: Proposed

Base: published 4e4c980af46e02d9fcc9b6b2d4239b6c726127af (ADR 0252).

## Problem

Every final leaves reproducible build output behind and nothing ever
removes it. The cross stage alone leaves about 2.5 GiB per final in a
private target directory under target/rnx-verify. Twenty-five such
directories held 61.5 GiB. During ADR 0250 and ADR 0251 the disk filled
three times and each time a final stopped, once inside the fixed-route
admission check before any measurement.

The output was removed by hand. A rule that lives in a person's memory is
not a rule; the tool that creates the output is the one that knows when it
is no longer read.

The gate also does not record how long each stage took. The figures used
to compare record costs were reconstructed from file times.

## Goal

A final ends with no build output that it alone created and that nothing
reads after its receipts are written. The terminal receipt states how long
each stage took.

## Required behavior

- Ownership is explicit. A producer records the build directory it creates
  for one invocation before it starts a child process. Only a directory
  recorded this way is ever removed. No sweep by directory name, no
  removal of the root target directory, no removal of shared or reused
  caches.
- In scope: the private target directory of the cross stage; the private
  caches of a paired build when the build fails (success already removes
  them); the per-run target copies of an exact Miri run.
- Kept, and named as kept: the Miri preparation that later runs reuse and
  that receipts bind file by file; the shared Clippy caches; the native
  target directories of the suites; generated-project trees; test caches;
  external registries and toolchains. Baseline Clippy exports are released
  only if their exact target directory is resolved, not guessed; otherwise
  they stay and the results say so.
- Order. All children have exited, every check that reads the build
  output has finished, and every artifact a receipt binds has been copied
  and is durable before anything is removed. For the cross stage the host
  check reads the original compiler artifact, so removal comes after it.
  The terminal result is written after the removal and records its
  outcome.
- Removal happens on failure as well as on success, with diagnostics kept.
- A removal that fails never turns a failure into a pass and never
  replaces the original failure; it is reported beside it. A pass with a
  failed removal is a failure of the stage.
- A path that is a symlink, resolves outside the recorded directory, or is
  shared or bound as retained evidence is refused, and the refusal is a
  failure, not a skip.
- Receipts written before the removal stay valid afterward: revalidating a
  finished final does not need the removed output. What can no longer be
  replayed without a rebuild is stated, not implied.
- A process killed without warning can leave output behind. The record
  says so and adds no later sweep.
- Each stage receipt carries its elapsed time from a monotonic clock,
  covering discovery, command, validation and removal, with the removal
  time shown separately. One definition, used by every stage. Time is
  recorded, never gated.

## Out of scope

- Engine, session and host sources; manifests and lock files.
- The policy for which stages a manifest edit selects.
- Cleaning directories left by earlier finals.
- Remote hosts beyond a producer that already runs there.

## Acceptance

- Controls with short synthetic commands: removal after a pass and after a
  failure; the original failure preserved when removal also fails; refusal
  of a symlink, an outside path and a shared path; retained artifact
  hashes unchanged; receipt revalidation after removal; a Miri preparation
  still hit after the run copy is removed; the measured binary of a paired
  build survives; the cross host check precedes removal.
- One real cross run and one real exact Miri run show the owned directory
  absent afterward and the receipts valid.
- The final of this record leaves no private cross target directory, and
  its terminal receipt lists a duration for every stage.
- Required stages follow the changed paths.
