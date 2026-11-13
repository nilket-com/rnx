# ADR 0269: admitting a session entry does not scan its arguments' data

Status: Proposed

Base: published b251e1b117164fa032cc28575327af35531bc207 (ADR 0268).

## Problem

ADR 0268 removed the walk of live data after each session input and
measured what was left. With a vector of 5,000,000 integers bound, the
inputs `v.len()` and `v[0]+1` still take about 141 ms in an unoptimized
build, against about 0.6 ms for an input that does not mention `v`.
The length itself costs about 0.0002 ms.

The time is the formatting admission check. Before an entry runs, the
engine walks every value reachable from the entry's arguments and
confirms that each struct type found is permitted by the program's
formatting certificate. A session passes the bindings an input mentions
as arguments, so the walk covers the whole vector. It runs twice per
input: the domain prepares the entry when it starts and again when it
starts the cross-program call.

An input that reads one element of a large value must not cost time in
proportion to the value.

## Goal

Admitting an entry costs time and memory that depend on the number of
arguments and on the number of distinct struct types involved, not on
the number of values reachable from the arguments.

## Required behavior

- The check keeps its meaning exactly. An entry is refused, before any
  effect, when a value reachable from its arguments has a struct type
  the certificate does not permit. Every existing admission control
  keeps passing unchanged, including shared subgraphs, cycles and the
  injected allocation failure.
- The note states what the check protects: which operation would go
  wrong, and how, if an unpermitted struct reached a running program.
  The design is judged against that, not against the current code.
- However the answer is obtained without a walk, it must be right
  after every way a struct can come to be reachable from a value:
  construction, mutation in place, a push or insert into a container
  already bound, a callback result, a host result, a value moved in
  from another program in the same domain. The note lists these and
  shows the design is right for each. Anything not proved is walked.
- The two preparations of one start are either shown to be necessary
  or reduced to one. If the second sees the same arguments and the
  same certificate as the first with nothing able to run in between,
  it is redundant.
- A store that holds no struct pays nothing in its ordinary operations
  for this. The fixed-route bands show it, with no exception and no
  policy edit.
- Measured, in the harness of ADR 0268, with 5,000,000 integers bound:
  `v.len()`, `v[0]+1`, an input that pushes one element, an input that
  passes `v` to a function defined in an earlier input. Each must be
  within noise of the same input on a 16-element vector. Then the same
  with a vector of 1,000,000 struct instances of one permitted type,
  stated honestly whatever it is, and a control that an unpermitted
  struct deep in a large bound value is still refused.
- No value's size or layout grows in the core entry types.
- The preparation lives in a file the fixed-route policy inventories.
  The note names every inventoried item the design would touch. If any
  is touched, send the note and stop: that is the project owner's
  decision and it is still open.

## Not in this record

The remaining collection walks of ADR 0268. The cost of restoring
bindings into an entry and of writing them back. Formatting itself.
