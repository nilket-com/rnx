# ADR 0350: Host paths that run on the old core run

Status: Proposed

Base: published 66c8f732 (ADR 0349).

## Problem

ADR 0347 let a host function called by path run on the new
engine. Some calls that succeed on the old core now fail. They
ran on the legacy route before ADR 0347:

    let v = [10, 20];
    let r = Vec::remove(v, 1);

The old core returns 20 and leaves `[10]`. The new engine ends
with `execute: InvalidCode`. `Vec::insert(v, 0, 9)` fails the
same way. With method syntax both fall back and run.

A sweep of the installed registry on the image tested for ADR
0349 took 461 admitted paths, their 648 argument positions and
41 kinds of input. Of 25,278 combinations that reach the new
engine, 425 on 70 paths succeed on the old core and fail on the
new engine, the same in both native modes. Three causes:

- 45 are `Vec::insert` and `Vec::remove` by path. Lowering finds
  a method of that name on another owner and doesn't check that
  it accepts the receiver.
- 42 are `fs::write`, `fs::write_new`, `fs::append`,
  `web::html`, `web::text` and `web::parse_form` given a range,
  a capturing closure or a host function. The host bridge has no
  conversion for those inputs and ends with `InvalidCode`.
- 338 on 62 paths are a receiver test stricter than the old
  core's. `std::i64::clone(1u64)` returns 1 on the old core. The
  new engine reports "Expected type `std::i64` but found
  `std::u64`". The summing and multiplying forms of an iterator
  given another iterator fail the same way.

The public admission corpus has none of these. The checks of
ADR 0347 compared the failures and a list of values. They didn't
give each position the inputs the old core accepts.

The third cause is the next record's. One call in one source
runs on both engines with one receiver and fails on the new
engine with another, so a refusal would move a program that
works. Giving it the old outcome means converting the receiver
and reducing by protocol, with an order of effects and failures
that needs its own design.

## Decision

1. `Vec::insert` and `Vec::remove` by path have the old core's
   result and effects. The engine has neither operation, so the
   path is refused at compile time, called or taken as a value,
   by a check that the provider lowering selects accepts the
   owner the path names. The check is general and is not
   weakened for any path. A program that took such a path as a
   value and ran on the base moves to the legacy route with the
   same outcome.
2. A host parameter that takes a string or rejects its input
   keeps that contract for every kind of input: the call returns
   the function's own `Err`, as on the old core, and has no
   effect. This is a rule of the parameter, not of the six
   paths measured. The public host ABI doesn't change.
3. The sweep becomes a control in the repository that a stage of
   the final runs, in process. It takes the callables from the
   installed registry and the kinds of input from the heap's own
   list of types, with stated boundary values such as an empty
   iterator. It fails if an admitted combination fails where the
   old core succeeds, unless the combination is on a pinned list
   of the third cause. The list only shrinks: an entry that
   passes must be removed, and a new failure is never added
   without a ruling.
4. A combination that fails on both engines keeps what it has on
   the base. Its message is a later record's subject.
5. The paths a program took on the base keep their cost. A new
   error detail doesn't change how the no-detail case is
   encoded.
6. Scope: the new engine. No change to the legacy engine. If the
   fix changes a function of the gate's source inventory, the
   record declares the transition under ADR 0346. Record the
   count of legacy-only cases on the base and on the candidate.
   It is expected to stay 67. Any other count is a finding to
   explain.

## Controls

1. The programs of the Problem and one of each of the first
   two causes, with the result and what is left behind compared
   with the old core in both native modes: the vector's
   contents, and for the string parameter the `Err` and the
   absence of the file.
2. The control of Decision 3. Results give the number of
   callables, positions, kinds and combinations, the 425 of the
   base by cause, the 87 of the first two causes repaired, the
   pinned list of the third, and the time the control takes in
   its stage.
3. Every path whose route changes is listed with its old and new
   route.
4. Each fix's old behavior, planted back, breaks its own
   witnesses and the control of Decision 3.
5. The public admission corpus is replayed against the published
   base in both native modes. Every source that changes is
   listed with its old and new outcome and route.
6. Pinned verdicts follow the engine. Results list the rows.
7. The file gate and both project screens, image to image
   against the published base. Results give the encoding of the
   no-detail case on the base and the candidate.
8. Before signing, the whole rune-suite stage, the root-tests
   stage, the cross stage and the final-controls stage, each by
   its stage command. Then one complete final on the signed
   candidate, under the limits of ADR 0338.

## Constraints

- No new third-party crate. Tabs.
- No refusal at run time.
- A program that succeeds on the base keeps its results, effects
  and route.
