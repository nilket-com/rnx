# ADR 0257: constant indexing, blocks and if in an execution session

Status: Proposed

Base: published 84560a7c7de913bfaa604936b34e6ad7db1b2240 (ADR 0256).

## Problem

Six constant expressions are refused when an input is admitted to an
execution session. Three of them are ordinary structure, not calls:

	const A = [1, 2]; const B = A[0];
	const A = { let x = 2; x + 1 };
	const A = if true { 1 } else { 2 };

Legacy evaluates a constant by compiling it and running it on the old
virtual machine. The execution session has its own bounded evaluator that
runs at admission, with limits on expanded nodes, bytes and depth, and
that already produces scalars, text, vectors, tuples and objects. It stops
at an index expression, a block and an if. A session user who writes a
table of constants and reads one entry is sent back to the legacy engine.

## Goal

The three forms are evaluated at admission by the bounded evaluator, with
the same value as legacy, and never by the old virtual machine.

## Required behavior

- Scope is constant indexing, constant blocks with local bindings, and
  constant if. Constant functions, constant calls including Option
  construction, and byte-string constants stay refused with their present
  reasons.
- Evaluation happens once, at admission, inside the existing limits. A
  constant is never lowered into code that evaluates the expression when
  the program runs: errors, budget and cycles belong to admission, and a
  failed admission commits nothing. Lowering stays the materialization of
  an already evaluated value.
- No fallback to the legacy constant evaluator, in any case.
- Indexing covers the containers the evaluator already produces, vectors,
  tuples and objects, with the index kinds legacy accepts for each. An
  index out of bounds, a missing key and a wrong container or index type
  are refused at admission with a diagnostic at least as specific as
  legacy's, naming the index and the bound where legacy does. The selected
  element is charged to the limits like any other expansion.
- Each use of a constant yields a fresh value. Changing the result of one
  use never changes another use or the constant.
- Blocks have lexical scope: local bindings, shadowing, a tail value or
  unit. A local binding is a plain name or the wildcard; a destructuring
  or refutable pattern is refused by name. Only statements the evaluator
  supports are admitted; any other statement is refused by name. No host
  call and no effect is admitted.
- If requires a Boolean condition and evaluates only the selected branch.
  An overflow, a refusal or a limit in the branch not taken does not
  occur. A constant has the value the same expression has at run time:
  an if without else is unit on every path (ADR 0256), and its taken
  block is still evaluated for its checks. If with a pattern condition is
  refused by name.
- Constant cycles are detected as today, including through the new forms.
- The limits on nodes, bytes and depth apply to the new forms, with
  nesting counted. Exceeding one is a named admission refusal.
- Inline constant blocks in expression position are admitted only if they
  take the same evaluator path; otherwise they stay refused and the record
  says so.

## Fixed route

This record does not edit any inventoried item and does not change the
policy file. The evaluator and the materialization are in the execution
core, so code layout may move; the Ir bands pass per record and
cumulatively, as for every record, with no exception.

## Out of scope

- Constant functions, general constant calls, Option construction, bytes.
- New runtime instructions.
- Changes to legacy constant evaluation.
- Manifest and lock files.

## Acceptance

- session_parity.json: the three rows leave "Refused at admission" and
  become Equal; that class falls from 6 to 3 and Equal rises from 173 to
  176. The other three rows keep their reasons. Any row that does not
  come out Equal is sent before the table changes.
- Controls, each compared with legacy: indexing a vector, a tuple and an
  object; nested containers; out of bounds, a negative index, a missing
  key, a wrong type; an indexed constant used twice and one use changed;
  a block with shadowing, with a unit tail, nested blocks; a refused
  statement named; if with each branch taken, a failing dead branch, a
  non-Boolean condition, an if without else beside its run-time form,
  a nested if inside an untaken branch, an index whose container is a
  block or an if; one differential control per diagnostic family so that
  type names match legacy for the same input; a cycle through each new
  form; each limit
  reached through the new forms; a failed admission leaving earlier
  bindings unchanged.
- Miri covers the new selection, copying and materialization paths and
  repeated independent uses.
- The fixed-route source condition and bands pass. Required stages follow
  the changed paths.
