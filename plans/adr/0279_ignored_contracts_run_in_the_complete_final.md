# ADR 0279: ignored contracts run in the complete final

Status: Proposed

Base: published aa12d6db59c9c87a9eccb9ba17729021006523f5 (ADR 0278).

## Problem

The native manifest lists 35 contract selectors marked `#[ignore]`.
The complete final runs the Rune suite without them, so no gate runs
them. Run on aa12d6db, 24 pass and 11 fail, and the 11 fail for causes
introduced by records that were published while no gate ran them:

| Contracts | First failure |
|---|---|
| callback_sort, dynamic_methods | the refusal is now AwaitRequired where the contract expects AsyncFunction, with a different span |
| function_values, iter_adapters, numeric_methods, object_order, option_methods, type_values | the diagnostic is now HostNotEnrolled naming the item, where the contract expects the text Custom Native |
| iter_terminals, object_keys, wrong_arity | a row the contract expects to be refused is now admitted |

Each contract stops at its first failing assertion, so the rows after
it are not known to pass. A contract that stops at its first difference
also hides how many rows drifted.

## Decision

1. Every selector the native manifest lists as a contract runs in the
   complete final, ignored or not. The list comes from the manifest,
   not from a second hand-kept list; a manifest contract that the
   complete final does not run fails the final's own catalogue check.
   The selectors may run in parallel with the final's other stages,
   sized from the detected cores and memory.
2. A contract compares every row and then fails once, reporting every
   differing row with its expected and observed values. No contract
   stops at its first difference.
3. The 11 contracts are brought to the current behavior. Each changed
   row is in exactly one class:
   - A refusal whose kind or span changed. The record that changed it
     is named, and the new refusal is at least as precise as the old.
   - A diagnostic that now names what it refuses (HostNotEnrolled
     instead of Custom Native). The named form is kept.
   - A row now admitted. The admitted result is checked against the
     old engine's observable result, or against a published semantics
     ruling where the two differ on purpose. Only then does the row's
     expectation become the admitted result.
   - A defect: the new engine is wrong. A small fix is in this record;
     otherwise the row stays failing, the note names it and stops.
   No row is deleted, skipped or weakened to make a contract pass.

## Constraints

- Diagnostics never regress: an expectation is never moved to a less
  precise refusal or message.
- The engine source changes only for a defect found in the third
  decision. If it changes, the fixed-route screen runs and its bands
  pass unchanged; no inventoried item changes.
- The complete final grows by the time these selectors take (about
  17 minutes run one after another on this host); the per-stage
  durations are in the packet.
- No real environment or argv in a committed fixture.

## Controls

- All 35 selectors pass in the complete final on the candidate.
- A planted difference in two rows of one contract reports both rows
  and fails the contract once (committed, under test configuration).
- A manifest contract left out of the final's run fails the catalogue
  check (committed negative control).
- The count of rows in each class, per contract, in the packet; the
  rows total before and after for each of the 35 contracts is
  unchanged.

## Out of scope

Importing JSON directly into the new store; new admissions beyond the
rows these contracts already declare.
