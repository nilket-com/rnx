# ADR 0351: The gate and contracts repeat no work

Status: Proposed

Base: published 359c1efa (ADR 0350).

## Problem

A complete final took 4,164 to 4,454 seconds on the last four
records against a limit of 5,100, and the stages run before
signing cost about as much again. Three parts of that time redo
work the run already has. Measured on the finals of ADR 0347,
0348 and 0349:

- The gate reuses its published base and its anchor capture only
  when the whole policy file is unchanged. A record that
  declares a source transition under ADR 0346 changes that file,
  so the gate rebuilt the base and missed the anchor on all
  three: 552 to 591 seconds in the gate against 237 on ADR 0345,
  which declared none. The declaration doesn't change what the
  base image is or how it is measured.
- The 49 shared contract cases start in manifest order on five
  workers. The longest, `shadow`, takes 361 to 371 seconds and
  starts 41st, so four workers stand idle while it finishes.
- The contracts build their test binary cold in an empty target
  directory. The suite's own check built the same binary, with
  the same features and profile, minutes before in the same run.

## Decision

1. The gate reuses a published base and an anchor capture when
   everything that affects the built images and the capture is
   unchanged. Fields that only declare or score don't enter that
   identity: the source transitions, the inventory, the anchor
   transition and the two bands. A change to any field that
   affects an image or a capture still refuses reuse. The whole
   policy still governs the source condition and the scoring of
   every run.
2. The shared contract cases start longest first, by a weight
   committed in the manifest. A stale weight costs time and
   nothing else. The set of cases, their isolation and the order
   of the results are unchanged.
3. The contracts take the test binary the suite's check built in
   the same run. The selection of the artifact, the check of its
   debug information and the digest every case verifies are
   unchanged. Nothing is reused from an earlier run.
4. No check loses power. Every stage and every case that runs on
   the base runs on the candidate, on the same inputs, with the
   same verdict.
5. The design of ADR 0338 stands: its limits, the build from
   clean inside the run, the cleanup, and no state kept between
   runs apart from the published gate inputs.
6. Scope: `tools/final`, the contract manifest and their
   controls. No engine source changes, so the gate's source
   inventory is unchanged.

## Controls

1. Reuse accepts a policy that differs only in a field Decision
   1 names, one witness for each. It refuses a change in each
   kind of field that affects an image or a capture, one witness
   for each. The old identity, planted back, fails the accepting
   witnesses.
2. The cases dispatch in weight order with ties in manifest
   order. Results stay in manifest order. Results give the list
   of cases and their order on the base and the candidate, and
   the contract seconds on each.
3. The contracts' binary record has the same fields. A binary
   built with other features fails the check. The log shows no
   second cold build.
4. The stage names and the contract case names of the
   candidate's final equal those of the base's.
5. Before signing, the whole rune-suite stage, the root-tests
   stage, the cross stage and the final-controls stage, each by
   its stage command. Then one complete final on the signed
   candidate, under the limits of ADR 0338. Results give the
   seconds of the suite, the contracts, the gate and the whole
   run on the base's final and the candidate's.

## Constraints

- No new third-party crate. Tabs.
- Worker counts come from the detected host.
- A record that declares a transition after this one is the
  first to show Decision 1's saving in a final. Its Results
  state the gate's seconds and the lookup states.
