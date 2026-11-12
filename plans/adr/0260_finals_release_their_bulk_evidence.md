# ADR 0260: a final releases the bulk evidence it owns

Status: Proposed

Base: published 73a8e332fff55da47d470505e8de99c3aba537f2 (ADR 0259).

## Problem

ADR 0253 made a final release its build output. It left the evidence a
final writes: the routing tests keep every case's tree and raw streams
under target/routing-cli, and the cross stage keeps enrolled trees, raw
streams, inventories and child receipts under target/rnx-verify. Those two
stores hold 37 GiB and 34 GiB. Free space fell from 64 GiB to 48 GiB
across ADR 0259 alone, and the fixed-route stage refuses to start below
20 GiB. Two more records and a final stops for a reason unrelated to the
candidate.

Some of that output is not owned by anything: a routing test run without
an assigned root creates a directory named by process id and nonce that no
receipt names and no tool will ever remove.

The project rule is that results are reproduced by committed code and not
kept as durable evidence. The gate does the opposite by default.

## Goal

A final ends holding its small receipts and none of the bulk evidence
its stages wrote to the roots named below. The receipts are enough to say
what was run, on what, and what it concluded, and to run it again.

## Required behavior

- Ownership. Every bulk output root a final's stages write is assigned by
  the final, created new, and recorded before the producer starts, with
  the ownership checks of ADR 0253. The final, not the stage, releases
  them. A stage run inside a final does not create an output root the
  final did not assign. This covers the routing tests and their alias
  root, the adapter routing tests, the generated-project tests and the
  cross controller.
- One committed driver runs a final. A final is not launched by a script
  that is not in the tree.
- Validation is not weakened. Every check that reads a bound file runs
  with the file present. Nothing is changed to accept a missing file
  during validation.
- Order. All selected stages have finished and been validated; the
  terminal result and a compact manifest are written and durable; then the
  owned bulk output is released; then a release record is written. The
  final is complete only when acceptance and release both succeeded. A
  release that fails is visible and makes the final fail; it never hides
  or replaces an earlier failure.
- The compact manifest carries what a later reader needs in place of the
  bulk: candidate commit, toolchain and tool identities, commands, the
  declared environment by name only, selection and policy identities,
  stage statuses and durations, and the digests of the released payloads.
  No environment values and no argument vectors from the real process.
- Revalidating a completed final reads the terminal result, the manifest
  and the release record and does not need released files. It states that
  bulk payloads were released; it does not report them as verified again.
  A final whose release record is missing or incomplete is not complete.
- Failure. A final that failed keeps the diagnostics needed to decide
  the repair, bounded in size, and releases its bulk. The point at which
  the diagnostics are released is stated in the result.
- Interruption. There is no resumption. An interrupted final stops its
  children, releases its bulk and is not complete; the final is run
  again. If the driver itself is killed, its ownership record remains,
  and the next final releases the roots of any record whose owner no
  longer holds its lock before it starts. That is release by recorded
  ownership, not a sweep.
- Refusals as in ADR 0253: a root that is a symlink or is reached through
  one, a root whose identity differs from the one recorded at creation, a
  path resolving outside the recorded root, a root the final did not
  create, or a shared or reused cache is not removed, and the refusal is
  a failure.
- No sweep by name or age inside the tool. The tool removes what it
  recorded and nothing else.

## Existing stores

The 71 GiB already in the two stores predates ownership records. It
belongs to finals of published records and to orphan test runs, all
reproducible from published commits. It is removed once, by hand, as part
of this record, limited to those two directories, with the entries and
bytes listed in the results. Receipts of the unpublished candidate of this
record are not touched. This is a one-time cleanup, not a mechanism.

## Out of scope

Shared build caches and their size policy; the Miri preparation; any
change to what a stage measures or to a band; the inventoried executor
items.

The fixed-route outputs are bulk of the same kind, about 0.7 GiB per
final, but the shared anchor cache refers into the final that produced an
anchor. Releasing them needs the cache to own its payloads first. That is
its own record.

The final's own directory of stage logs, baseline archives and child
receipts, under 0.1 GiB, and temporary directories of unrelated tests are
not changed here.

## Controls

- A complete final on this record ends with its two stores holding only
  the records named above; sizes before and after are reported.
- Revalidation of that final after release passes and names the release.
- A failing stage: bounded diagnostics kept, bulk released or kept as the
  rule states, original failure reported first.
- A release failure after a pass makes the final fail.
- Symlink, outside-root and not-created-here roots are refused.
- A routing test run inside a final without an assigned root fails.
- Interrupt during a stage: children gone, bulk released, final not
  complete. Kill the driver: the next final releases the recorded roots
  and refuses a record whose owner is alive.
- An attempt to validate with a bound file missing before the terminal
  result still fails as it does today.

## Consequences

A published final can no longer be inspected stream by stream; it can be
run again. That is the stated rule of this project. Disk use per record
becomes roughly constant.
