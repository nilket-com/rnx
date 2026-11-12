# ADR 0256: if without else evaluates to unit in the execution engine

Status: Proposed

Base: published 485babedaeaa0911c70cb3f5afce08db8c1406df (ADR 0255).

## Problem

The two engines disagree on the value of an if that has no else:

	let a = if true { 7 }; a

Legacy gives unit, at run time and in constant evaluation, whether the
condition is true or false. The execution engine gives 7 when the
condition is true and unit when it is false. Its lowering passes the value
of the taken block through and writes unit only on the path where no
branch matched.

The execution engine's answer is the wrong one on its own terms. The
value, and its type, depend on a condition the reader cannot see in the
expression's shape; a branch that runs produces a number and the same
expression with the branch skipped produces unit. An if without else is a
statement used for its effect. Its value is unit, as in legacy and as in
Rust, where the form is only accepted when the block already has unit
type.

No test and no row of the pinned comparison covers the form, which is how
it went unseen. It was found by a control written for the constant
evaluator.

## Goal

In the execution engine an if without else evaluates to unit on every
path, and the comparison of the two engines pins it.

## Required behavior

- An if with no else evaluates to unit whether or not a branch is taken.
  The taken block still runs in full, with its effects and its errors.
- The same holds for an else-if chain with no final else, and for an if
  with a pattern condition and no else.
- An if with a final else is unchanged: the value is the taken block's.
- The implementer audits the other forms whose value legacy fixes as unit
  regardless of their body, loops without a break value among them, runs
  each in both engines, and reports every difference found. A difference
  outside the if forms is reported, not fixed here.
- Where the value of the if is not used, the lowering emits no additional
  instruction. The common statement use must not pay for the fix.
- Programs admitted before the change that relied on the old value now
  get unit. The record states this plainly as a correction of the
  execution engine; there is no compatibility switch.

## Fixed route

This record does not edit any inventoried item and does not change the
policy file. The change is in lowering. The Ir bands pass per record and
cumulatively, as for every record, with no exception.

## Out of scope

- Constant expressions; they follow in the next record and rely on this
  one.
- Any other semantic difference the audit finds.
- Legacy behavior.
- Manifest and lock files.

## Acceptance

- New rows in session_parity.json for the form, measured Equal: condition
  true and false, an else-if chain without else with each branch taken and
  none taken, a pattern condition without else matching and not matching.
  Existing rows keep their class.
- Controls: the taken block's effect happens once; an error in the taken
  block is reported as before; the value with a final else is unchanged;
  the form in statement position produces the same instruction count as
  before the change.
- The audit list with the outcome for each form in both engines.
- Miri only if the lowering change adds a path the existing tests do not
  exercise; otherwise the record states why existing coverage holds.
- The fixed-route source condition and bands pass. Required stages follow
  the changed paths.
