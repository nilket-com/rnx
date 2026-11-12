# ADR 0259: constant functions in an execution session, with bounded work

Status: Proposed

Base: published a4dcb2d8ed1b4c9acb0b9724c23808f9a368bdf1 (ADR 0258).

## Problem

A constant function is refused when an input is admitted to an execution
session:

	const fn f(x) { x + 1 }
	const A = f(2);

The declaration itself is refused, before its body is looked at. Legacy
evaluates such a call by compiling the function and running it on the old
virtual machine under a budget of 1,000,000 instructions, which is also
what stops a function that never returns.

The execution session's evaluator runs at admission and is bounded in the
size and depth of the data it builds. Until now it could not repeat work:
it has no calls and no loops, so the size of the source bounded the work.
A call changes that. A function can call itself, and a few lines can ask
for unbounded evaluation. Admitting calls needs a bound on work that does
not depend on the source being short, and a bound on how deep the
evaluator itself may recurse on the host stack.

## Goal

A constant initializer can call a constant function, with the same value
as legacy, evaluated at admission by the bounded evaluator, and every
evaluation ends in bounded time with a value or a named refusal.

## Required behavior

- Scope is calls from constant evaluation to constant functions whose
  parameters are plain names or wildcards and whose bodies use the forms
  the evaluator already admits, plus return. Direct and mutual recursion
  are admitted within the bounds. Loops, assignment, destructuring
  parameters, pattern conditions and match stay refused by name when
  reached. Calls to a constant function from ordinary run-time code stay
  refused as today; the record states where that refusal is now reported.
- Never the legacy evaluator and never the old virtual machine. The
  function's lowered form is collected and read; nothing is assembled or
  run. A body that is not available is a named refusal at the call.
- A call checks arity, evaluates its arguments once, left to right, in
  the caller, and enters the callee with its own bindings. A callee can
  never read a caller's local, whatever their internal numbering. A
  wildcard parameter still evaluates its argument.
- Return leaves the current function only, from any depth of blocks,
  conditions, operands and arguments, without evaluating what follows. A
  return in an argument position returns from the enclosing function
  without making the call. Return outside a function stays refused.
  Normal return is not carried as an error.
- Work is bounded by one counter for the whole admission, not per
  initializer, block or call. It is charged before each evaluated
  expression, block entry, function entry and reached condition, and is
  never refunded or reset. Reaching zero is a named refusal that states
  the limit. The existing limits on nodes, bytes and nesting stay and may
  refuse first.
- The default for the counter is chosen by measurement and the record
  shows it: the units a set of reference functions spend beside the
  instructions legacy spends on them, and the wall time to exhaust the
  counter in the worst admitted shape. The default admits the reference
  functions legacy admits and exhausts within a time stated in the
  record. If the existing node count already charges every evaluated
  expression, the record separates work from data so that the work bound
  is the counter and the node bound is about data.
- Host stack use is bounded by a limit on live calls and on evaluator
  nesting that holds across calls. The limits are derived from measured
  stack use per level with a stated margin and are tested on the smallest
  thread stack the tests run on. The record states the deepest simple
  recursion admitted, beside legacy. Exceeding a depth limit reports that
  limit, not the work limit.
- A failed admission, for arity, a body error, work, depth or a
  dependency, commits nothing and leaves earlier bindings unchanged.
- Dependencies between constants and functions use the existing cycle
  rules. A terminating recursion is not a cycle. The diagnostics for a
  constant that reads itself through a function, and for mutual
  dependencies, are measured in both engines before a class is chosen.
- A diagnostic raised inside a callee keeps the callee's own origin. If
  the origin cannot be carried, the diagnostic is reported at the call
  and names the callee; it never points into the wrong input.
- The stock constant budget option and its meaning are unchanged.

## Fixed route

This record does not edit any inventoried item and does not change the
policy file. The Ir bands pass per record and cumulatively, as for every
record, with no exception.

## Out of scope

- Loops and assignment in constant functions; they need mutable locals
  and break and continue and are their own record.
- The shared constant-pattern evaluator.
- Run-time calls to constant functions.
- An explicit-frame evaluator; it follows only if the measured depth
  limits prove too low.
- Bytes. Manifest and lock files.

## Acceptance

- session_parity.json: the constant-function row leaves "Refused at
  admission" and becomes Equal; that class falls from 2 to 1. New measured
  rows for recursion, early return and a wildcard parameter, with their
  class stated. Any row that does not come out Equal is sent before the
  table changes.
- Controls, each compared with legacy where legacy admits the form:
  argument order with two failing arguments; arity; caller and callee
  locals with equal internal numbers; return from nested blocks,
  conditions and argument position; direct and mutual recursion; a result
  that is a container or an Option used twice with one use changed; the
  work limit reached by a shallow branching recursion, at its exact
  boundary, shared across sibling calls and across initializers; each
  depth limit reached and named; loops and assignment refused by name
  when reached and not when in an untaken branch; the run-time call
  refusal; every failure leaving earlier bindings unchanged.
- The measurements that fix the default and the depth limits.
- Miri covers frame isolation, argument and return control, shallow
  recursion, independent materialization of results and cleanup after a
  failure, with small test limits.
- The fixed-route source condition and bands pass. Required stages follow
  the changed paths.
