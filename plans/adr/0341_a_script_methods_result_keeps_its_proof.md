# ADR 0341: A script method's result keeps its proof

Status: Proposed

Base: published 8f4d4c09b4833460436a2ecc64339c38833d0977 (ADR 0340).

## Problem

The new engine refuses a program that calls a method on the result
of a script method, when the method called has the name of a host
method. L1 case 00357, a linked list, does `let it = ll.iter();`
and then `it.next()`, where `iter` and `next` are script methods.
It is refused with `Method` and is one of the 77 legacy-only cases.

This was measured on the image tested for the published 8f4d4c09.
The refusal needs two things together: the receiver is the value of
a script method's call, and the script method called on it has the
name of a host method, such as `next` or `len`. Under the name
`step` the same program runs on the new engine. So does `next` on
the value of a script function's call, on a parameter, and on a
host iterator. With no script method `next` at all, the call runs
on the new engine and reports the missing method.

The proof graph in `frontend/receiver_proofs.rs` gives the call of
a script function an edge from the function's return to the call's
value. It gives the call of a method none, so that value is
unknown. It already follows the arguments of such a call into the
methods it may reach, and not what they return.

## Decision

1. The value of a method call carries the proof of the method's
   return when the receiver is proved to be of one script type, and
   that type has a script method of that name and argument count.
   Otherwise the value stays unknown, as it is today.
2. The new edge carries the obligations that the edge of a script
   function's call carries, recursion among them.
3. ADR 0339 holds through the edge. A method whose result can be
   unit proves no receiver.
4. No rule is keyed to a method's name, and nothing is dispatched
   by name alone. A call on a proved result is the direct call. A
   call on an unproved result keeps the route it has today.
5. The old core's measured outcome is the reference for the result,
   the output, the error text and its position. The route is
   recorded apart from the output.
6. Scope: the new engine's proof graph, and its lowering only where
   the proof is used. No change to the legacy engine. No new
   opcode, and the bytecode format, argument passing and host ABI
   don't change. Record the corpus, the command and the count of
   legacy-only cases on the base and on the candidate. The base
   count is 77. The one case is expected to move, which gives 76.
   Any other count is a finding to explain.
7. Not here: a call of a `const fn` in a function body, and a
   format template given by a constant expression. Work on them
   found that the new engine's constant evaluation accepts a const
   fn whose unreached code is invalid, such as a `break` outside a
   loop, where the old core reports a compile-time error. Published
   `const` items already do this. Checking a constant before it is
   evaluated gets the next record, and the two forms follow it.

## Controls

1. The linked list of the L1 case runs with native code on and off,
   asserts the new engine's route, and equals the old core.
2. Script methods named as a host method is, such as `next`, `len`,
   `iter` and `push`, and not. The first receiver from a struct
   literal, from an associated function and from `self` inside a
   method. A chained call. A counted effect in each method body
   runs once.
3. On a result that can be unit the call gives the old core's
   outcome and never enters the script method: `next` reports the
   missing method and `len` yields 0. The hit and miss witnesses of
   ADR 0339, made through a method instead of a function.
4. Two script types with a method of one name that return
   different types. A receiver proved to be one of them reaches its
   own method. A receiver that may be either stays unknown and
   keeps its route. So does a call with another argument count. A
   method that returns the result of calling itself, and two that
   call each other, end the proof as functions do.
5. A method missing on a method's result is reported as on the
   base.
6. The edge, planted out, breaks the linked list. The edge made
   without the condition on the receiver, planted in, breaks the
   either-type witness of Control 4.
7. The public admission corpus is replayed against the published
   base in both native modes. Case 00357 moves to the new engine
   with the same result. Every other source that changes is listed
   with its old and new outcome and route.
8. The file gate and both project screens, image to image against
   the published base.
9. Before signing, the whole Rune library test binary in separate
   build state. Then one complete final on the signed candidate,
   under the limits of ADR 0338.

## Constraints

- No new third-party crate. Tabs.
- No refusal at run time.
- A program with no method call on a script method's result keeps
  its results, effects and route. A move from the legacy route to
  the new engine with the same exit, output and effects is within
  the record.
