# ADR 0262: patterns in constants

Status: Proposed

Base: published 294997ae7892f94ba9aec22e5b5ae62cea7d8590 (ADR 0261).

## Problem

The bounded constant evaluator binds a name or a wildcard and nothing
else. A constant initializer or a constant function that takes a value
apart is refused: a destructuring let, an if with a pattern condition, a
match. ADR 0257 made the pattern condition a refusal only when reached,
and ADR 0259 did the same for the forms inside a constant function, so
the refusals are honest, but they are still where an ordinary constant
function stops. A function that returns an Option cannot be used by
another constant function without a pattern.

Three separate refusals stand in for one missing piece: matching a
constant value against a pattern.

## Goal

One matcher in the bounded evaluator, used by let, by if let, by match
and by function parameters, so that all four accept the same patterns
with the same meaning.

## Required behavior

- Semantics are those of the same form at run time. Before implementing,
  each form is run at run time in both engines and as a constant in the
  legacy session, and the three results recorded. Where a constant in
  legacy differs from the run-time form, the run-time form wins and the
  difference is brought back for a ruling before it is pinned.
- Patterns: wildcard, binding, literal (integer, boolean, character,
  text, unit), tuple, vector with and without a rest, object with and
  without a rest, Some and None, and nesting of these. Other patterns
  the language accepts are listed in the note; those not implemented are
  refused by name when reached. Patterns of typed constructors whose
  values the evaluator cannot hold, Result and Ordering among them, are
  in that group and the results say so.
- Forms: let with a pattern; if let with and without else; match with
  arms in order and guards; function parameters that are patterns.
- A miss is not an error where the form has somewhere else to go. A
  match in which no arm matches, and an if let without else, yield unit,
  as at run time in both engines. A value of another type than the
  pattern is a miss like any other. Only a let and a parameter, which
  have nowhere else to go, fail, with the run-time message.
- One matcher. Let, if let, match and parameters call the same function.
  No second copy for any of them.
- Binding follows the rules ADR 0259 fixed: a value moved into a binding
  is not charged; a part copied out of a value that stays alive is
  charged as a copy, nodes beyond the root. Matching itself is work:
  each pattern node visited is charged to the admission work counter,
  and so is each field passed over in a key search and each byte
  compared of a text or a key. A pattern cannot do unbounded work for
  one unit.
- Depth: a nested pattern counts against the nesting bounds like a
  nested expression. The 2 MiB stack tests of ADR 0259 are extended with
  the deepest admitted pattern and stay passing in both profiles; if a
  frame grows past the reserve, the limits are lowered and the note says
  by how much. Thread stacks are not enlarged.
- Bindings made by a pattern are scoped as at run time: an arm's
  bindings do not outlive the arm; a failed match binds nothing.
- Diagnostics: a let or a parameter that does not match is reported at
  the pattern, as at run time, not at the enclosing block or call.
  Across sources the rules of ADR 0259 apply and a refusal stays a
  refusal.
- Only the bounded evaluator and what it needs change. No inventoried
  executor item, no run-time lowering of patterns.

## Parity

The class of each row is measured. New rows: a destructuring let of a
tuple and of an object, a vector pattern with a rest, if let on Some and
on None, a match with a guard, a match with no arm matching, a constant
function with a pattern parameter, and a constant function that matches
on the Option returned by another.

## Controls

- Every pattern kind matching and not matching, at run time and as a
  constant, compared.
- Arm order, guard evaluated only when its pattern matched, guard
  failing falls through, bindings of a failed arm not visible.
- Evaluation order and single evaluation of the matched expression.
- Work exhaustion inside a match in a recursive function; data charge
  for a copied part at the exact boundary; a moved value charges none.
- Nesting at the limit and one over, on a 2 MiB thread, both profiles.
- A failed match inside a session leaves earlier bindings unchanged.
- Under Miri: partial bindings released when a later part of the pattern
  fails, no leak and no double release.
- Each pattern kind not implemented is refused by name only when
  reached.

## Out of scope

Loops and assignment in constant functions; patterns for values the
evaluator cannot hold, such as bytes and user types; any change to how
patterns run at run time.
