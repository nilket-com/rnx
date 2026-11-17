# ADR 0288: Admission waits are recorded

Status: Proposed

Base: published 8092c4b575f9fb5e60f93b9ce475f4e814aa8d13 (ADR 0287).

## Problem

ADR 0287's complete final took 54.1 minutes against ADR 0286's 45.7,
with each stage about as long as before. For six minutes only one stage
ran while later stages waited, and the receipts cannot say why: the
controller records when a stage starts and ends, not why a ready stage
did not start. A slowdown under the 60-minute limit should be explained
from the final's own evidence, not guessed.

## Decision

1. The controller records each stage's admission as a sequence of state
   changes, in a new terminal record, `SCHEDULE.json`, bound into the
   verdict's records as `ENVIRONMENTS.json` is. For every stage: when it
   became ready (its producers passed and no earlier stage sharing a
   resource lane is unfinished), each reason it then waited, when that
   reason began, and when it started and ended. Times are seconds since
   the final began, as its other durations are.
2. The reasons are the admission's own gates, named as the code checks
   them:
   - `producers`: a declared producer has not passed;
   - `lanes`: an earlier stage sharing a resource lane is unfinished,
     with that stage named;
   - `alone`: the stage runs alone and another is running, or another
     runs alone;
   - `disk`: free disk below the floor and the remaining stages'
     declared growth, with both numbers;
   - `tokens`: its weight in tokens was not available, with the weight;
   - `queued`: an earlier stage in the same pass stopped admission, with
     that stage named.
3. Only a change is recorded: a stage's reason is written when it first
   applies or differs from the last one written, and its start and end
   once. The controller reads nothing new: no timer, no polling, no
   progress log. Admission itself, its order and its gates, does not
   change.
4. Revalidation checks `SCHEDULE.json` against its binding, its stages
   against the selection, and each stage's start and end order. The
   comparison of two finals treats its times as run-specific and its
   reasons as evidence, not as a parity field.

## Constraints

- Admission, scheduling, weights, lanes, disk and the job budget are
  unchanged; the complete final stays under 60 minutes.
- No new third-party crate, no new Python file.

## Controls

- Planted schedules: a stage waiting on a producer, then a lane, then
  tokens; a stage queued behind one refused its tokens; a disk wait; an
  alone stage. Each records exactly those changes, in order, and nothing
  while the reason holds.
- A final whose `SCHEDULE.json` is altered after binding is refused by
  revalidation; a missing or extra stage in it is refused.
- One complete Rust final; its record attributes each wait of its
  critical path to a gate.

## Out of scope

Changing admission to act on what the record shows (for example waking
on returned tokens), and the `rune-suite` port, which follows.
