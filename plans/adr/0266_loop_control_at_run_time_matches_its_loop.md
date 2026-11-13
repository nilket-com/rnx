# ADR 0266: loop control in the execution engine belongs to its loop

Status: Proposed

Base: published b32bbaa6203575266d46290b54cc441298ffdcc4 (ADR 0265).

## Problem

ADR 0264 compared loops in constants against both engines at run time
and found two places where the execution engine disagrees with the
legacy engine, outside constants:

- `break` with a value inside `for` is accepted. The legacy engine
  rejects it at compile time, as Rust does: a `for` has no value to
  give.
- The condition of a `while` is compiled as if it were outside the
  loop. A `break` in the condition leaves the enclosing loop, and a
  `continue` there is refused.

Both were logged and left. The first accepts a program that should be
rejected; the second runs a program with the wrong control flow, which
is the worse kind of difference because nothing reports it.

## Goal

At run time in the execution engine, every `break` and `continue`
resolves to the loop the language says it does, with or without a
label and with or without a value, and what is rejected is rejected at
compile time with a diagnostic at the offending expression.

## Required behavior

- The reference is the legacy engine where it is right. For each form
  in the table below the step-2 note gives what legacy, the execution
  engine and the constant evaluator of ADR 0264 do today, and what Rust
  does. Where legacy itself looks wrong, the note says so and the
  ruling decides; the execution engine does not copy a legacy mistake.
- The table covers at least: `break` with a value in `for`, in `while`
  and in `while let`; `break` and `continue`, plain and labeled, in the
  condition of a `while`, in the scrutinee of a `while let`, and in the
  iterator expression of a `for`; the same inside a nested loop, where
  the wrong target is observable; a labeled `break` to an outer loop
  from each of those positions; a label that names no enclosing loop.
- The constant evaluator agrees with the corrected run time for every
  row it admits. If it disagrees, it is corrected here.
- A rejection is a compile error at the expression, never a run-time
  refusal and never a silent choice of another loop. Its text is at
  least as specific as the legacy text.
- The fix does not add an instruction to a loop that uses none of
  these forms; the fixed-route bands show it.
- Each row of the table is a parity row in the pinned session table,
  with its measured class.
- No inventoried executor or policy item is edited; if the fix needs
  one, the note says so and work stops there.

## Not in this record

Valued `break` from `loop` and labeled blocks, which already agree.
Ranges kept in a local. Legacy's panic on an open range at the largest
integer.

## Acceptance

The table in the results with before and after for all three
evaluators; the parity rows; Miri on the 0264 loop inventory and on
any new inventory; a final in required mode with the fixed-route
bands.
