# ADR 0264: loops and assignment in constants

Status: Proposed

Base: published a079db4ab30082ea815cc3d8e0f25cb98e3f80d7 (ADR 0263).

## Problem

The bounded constant evaluator refuses a loop, a break, a continue and
an assignment when one is reached. ADR 0259 gave constant functions
recursion under a work counter, and recursion is bounded by the stack
long before the work counter: about two hundred live calls where the
legacy session admits thousands. A constant function that sums, counts
or searches therefore has to recurse, and stops at a depth that has
nothing to do with how much work it does. The ordinary way to write it,
a loop over a local that is assigned, is refused.

The work counter already exists to bound exactly this. A loop needs no
stack per iteration.

## Goal

A constant initializer and a constant function may loop and may assign
to their own locals, bounded by the admission work counter and the data
budget, with the meaning the same forms have at run time.

## Required behavior

- Semantics are those of the same form at run time. Before implementing,
  each form is run at run time in both engines and as a constant in the
  legacy session, and the three results recorded. Where a legacy
  constant differs from the run-time form, the run-time form wins and
  the difference is brought back for a ruling before it is pinned.
- Loops: loop, while, while with a pattern condition, and for over what
  a constant can hold and the run-time form accepts (ranges, vectors,
  and whatever else the note finds), each with break, break with a
  value from loop, while and a labeled block, continue, and labels. A
  break with a value inside a for is an error with the legacy message,
  not a refusal: the legacy engine rejects it at run time and as a
  constant, and no engine should accept it. A range is iterated where
  it is written; a range kept in a local stays refused by name. Forms
  not implemented stay refused by name when reached,
  and the results list them.
- Assignment: to a local of the current frame, plain and compound. A
  compound assignment evaluates its right side first and reads the local
  after, as at run time. Assignment to a part of a local (an index, a
  field) is refused by name when reached: at run time two locals can
  name one aggregate and a write through one is seen through the other,
  and the evaluator's values are owned trees that cannot show that.
  Offering the write with copy semantics would be a wrong answer, not a
  missing feature. A constant item is never assignable. A local of
  another frame is not reachable, as today. The pattern matcher of ADR
  0262 serves the loop forms that bind; no second matcher.
- Every iteration is charged to the admission work counter, including
  an iteration whose body is empty, so that `loop {}` ends in the named
  work refusal and never in a hang. The counter is the one of ADR 0259:
  shared, never reset.
- Data: a value built in an iteration is charged as today, cumulative,
  not refunded. Replacing a local by assignment drops the old value and
  refunds nothing. The results state plainly what this admits and
  refuses: a loop that assigns a freshly built small vector to
  the same local each time, with the iteration at which the data budget
  refuses, and one that nests the previous value each time, with which
  bound refuses first. No vector mutation is added to make a witness.
- Stack: an iteration adds no native frame. A loop body nests like any
  block against the nesting bounds. The 2 MiB stack tests are extended
  with the deepest admitted loop nesting and stay passing in both
  profiles.
- Control flow leaves cleanly. Break, continue and return from inside
  nested loops and blocks, and an error inside a loop, each leave the
  locals as a block exit does; nothing bound in an abandoned iteration
  survives. Break and continue never cross a function call.
- A failure inside a constant in a session publishes nothing and keeps
  earlier bindings, as today.
- Limits are unchanged. If a measurement argues for changing one, that
  is brought back and is not done here.
- No inventoried executor or policy item is edited. If one would be,
  stop and say so.

## Not in this record

An explicit-frame evaluator for deeper recursion, Result and Ordering
values in constants, text comparison in constants, bytes values.

## Acceptance

Session rows for a counting loop, a while with assignment, a for over a
range and over a vector, break with a value, continue, nested loops
with an early return, and a constant function that loops ten thousand
times, each classified as measured; controls for the work refusal of an
empty infinite loop with its time in both profiles, the data refusal,
cleanup on every exit, and each form still refused; Miri on the
committed inventory with one compile per test; a final in required
mode.
