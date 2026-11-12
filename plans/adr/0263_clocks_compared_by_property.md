# ADR 0263: clock functions are executed and compared by property

Status: Proposed

Base: published 085850ee7b1eff3d0d11b7665b68e3057008f785 (ADR 0262).

## Problem

Three rows of the pinned session parity table say "Admitted, not
executed": time::now_ms, time::monotonic_ms and time::monotonic_ns. The
execution engine admits each call, and the table then stops, because two
readings of a clock cannot be expected to be the same value and the table
knows only how to compare values. When two readings do coincide, as two
first millisecond readings can, that says nothing either. So the table says nothing about whether the execution
session calls these functions correctly. A session that returned zero, a
text, or a decreasing monotonic reading would leave the three rows as
they are.

The verification code already has predicates for live clock readings,
used by the census. The session parity table does not use them.

## Goal

The three clock rows are executed in both sessions and compared by a
stated property of each reading, under a class that says so.

## Required behavior

- A new class, "Equal by property", distinct from Equal. A row has it
  only when both sessions ran the input, both readings satisfy the
  row's committed property, and everything about the row that is not the
  reading (the kind of outcome, the type of the value, the presentation
  form, what is bound afterwards) is compared exactly as for an Equal
  row. The summary counts the class separately. Equal never includes a
  row compared by property.
- The property of a row is committed data in the row, named, and
  evaluated by one function. For the three rows:
  - now_ms: an integer; between a reading the test takes before the
    input and one it takes after, with a stated tolerance for flooring.
  - monotonic_ms and monotonic_ns: an integer; not negative; two
    readings in one session, the second in a later input, do not
    decrease; a reading taken after a measured pause differs from the
    one before by at least the pause less a stated tolerance and by no
    more than a stated bound, in the unit of the function.
  The implementer proposes the tolerances from what the functions
  document and measures them under load; a property that fails one run
  in many is not acceptable, and neither is one so loose that a constant
  passes.
- The predicates are the ones the verification code already has, or one
  shared set that both use. No second copy of a clock rule.
- A row whose property fails in either session is not "Equal by
  property". It is classified as measured and the test fails, as for
  any other row that moves.
- Teeth: controls show that a constant reading, a reading of the wrong
  type, a reading in the wrong unit (milliseconds where nanoseconds are
  claimed) and a decreasing pair are each rejected by the property.
- "Admitted, not executed" stays as a class for whatever still has it.
  If nothing has it after this record, the class is removed from the
  table rather than kept at zero, and the results say which.
- No change to the engine, to the host functions, or to any inventoried
  item. This record changes what is measured, not what runs.

## Not in this record

The Dense receiver rows, the async host functions, bytes values, and the
host functions that need an edit to an inventoried executor item.

## Acceptance

The three rows carry the new class with a measured property in both
sessions; the teeth controls pass; the parity tests pass fifty times in
a row under a parallel build load without a property failure; a final in
required mode passes; the totals before and after are in the results,
with every row that moved.
