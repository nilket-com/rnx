# ADR 0342: A constant is checked before it is evaluated

Status: Proposed

Base: published 4edbb2a4 (ADR 0341).

## Problem

The new engine accepts invalid programs that the old core rejects.
In

    const fn f() { if false { break; } 7 }
    const V = f();

the `break` is outside any loop. The old core reports "Break
outside of loop" at the `break` and compiles nothing. The new
engine prints 7.

The old core makes such checks when it assembles a constant's code
for its compile-time machine, before it runs it. The new engine's
constant evaluation walks the HIR and visits only what the value
reaches. Code it doesn't reach is never checked.

The boundary ran 142 sources on the image tested for the published
base, with the automatic route and with the legacy engine forced, native
code on and off. It found nine kinds of error that the new engine
misses in code a constant doesn't reach: BreakUnsupported,
ContinueUnsupported, UnsupportedAssignExpr, UnsupportedBinaryExpr
for a compound assignment, BreakUnsupportedValue for a valued
`break` out of a `for`, ContinueUnsupportedBlock,
UnsupportedPatternExpr for a float literal pattern,
UnsupportedUnaryOp for unary `*`, and BadFieldAccess. They were
seen in the block of a `const` item and in a called const fn.
Where the value reaches the code, the new engine's evaluation
fails and the old core reports the error as before.

Unresolved names and labels, `&`, closures, statics and a `yield`
in a const fn are reported on both routes today. The same forms in
an ordinary function body are refused or reported today.

The old core doesn't check all of a constant's code. It accepts

    const fn f() { return 7; 1 = 2; }
    const V = f();

because its assembly stops at an expression that always leaves. It
skips later statements of the block, later operands of a tuple or
a binary expression, a statement after an `if` whose arms both
return, and a const fn called only from such code. With one arm
returning, the statement is checked even when the call takes that
arm. Rejecting these programs would break programs that run today.

ADR 0341 left out two forms for this reason: a const fn called in
a function body, and a format template given by a constant
expression. Both would evaluate more constants on the new engine
and spread the difference.

## Decision

1. Before a constant is evaluated, its code is checked wherever
   the old core's compilation visits it, whether or not the value
   reaches it: the block of a `const` item, and the body of each
   const fn that visited code calls. A branch isn't skipped for
   the value of its condition, and no condition is evaluated to
   decide what is checked.
2. Code isn't visited when it follows, in evaluation order, an
   expression that always leaves: a `return`, a `break`, a
   `continue`, or a branch whose arms all leave. That covers the
   later statements of a block, the later operands of an
   expression, and a const fn called only from such code. The old
   core accepts these programs and they stay accepted. In a
   conditional, a test that always leaves drops its own arm only:
   the `else` body is still visited, as the old core does.
3. The check is a pass of the shared front end over HIR. It lives
   with neither engine, so it outlasts the legacy one. It doesn't
   run, import or restate the legacy assembler and emits no legacy
   code.
4. The pass reports the old core's error for the same code, with
   its kind, text and position, for the nine kinds of the Problem.
   An error that HIR lowering reports today is still reported,
   also for code the pass doesn't visit.
5. Which constants are checked, and which error is reported when
   several apply, follow the query that evaluates constants, as
   the old core does. An unused `const` item is checked. An unused
   const fn isn't, until visited code refers to it. A body's own
   error is reported before its callees'. An error of HIR lowering
   is reported before one of the pass. The pass isn't a sweep over
   every function or in source order. Within a conditional the old
   core reports an error of the `else` body before one of an arm,
   and the pass does the same.
6. The pass never rejects a program that the old core accepts.
   Stop and report before signing if that can't be kept. The two
   differences below don't stop the record. Results list each with
   its source.
   - The old core rejects a program for one of the nine kinds in
     code that Decisions 1 and 2 don't visit, and no rule over HIR
     as small as the conditional's gives it. The pass accepts it.
   - Several errors of the pass apply to one body and the old
     core's choice among them needs more than such a rule. The
     pass reports the first in source order.
7. The legacy assembler's limits on depth, slots and allocation
   are limits of its construction and aren't reproduced. The
   evaluation's work limit doesn't change; it differs from the
   legacy instruction budget, as ADR 0259 states. The pass doesn't
   recurse on the host stack without a bound. If it has a limit of
   its own, state what reaching it reports and add a witness.
8. Ordinary function bodies aren't covered. The boundary found no
   form there that the new engine accepts and the old core
   rejects. A control keeps the witnesses.
9. Scope: the shared front end, and the new engine's constant path
   that calls the pass. What the legacy engine reports doesn't
   change. No new opcode, and the bytecode format, argument passing
   and host ABI don't change. Record the corpus, the command and
   the count of legacy-only cases on the base and on the candidate.
   The count is 76 on both. A changed count is a finding to
   explain.

## Controls

1. Each of the nine kinds in the block of a `const` item, in a
   const fn called by a `const` item, and in a const fn called by
   another const fn, inside a branch, a match arm and a nested
   block that never run. Each runs with native code on and off and
   with the legacy engine forced, and reports the old core's kind,
   text and position. The route is recorded.
2. Valid constants keep their values: loops with `break` and
   `continue`, a valued `break` out of `loop` and `while`, labeled
   breaks, and assignments to a local, a field and an index.
3. The invalid forms where they are reached keep the error they
   have on the base.
4. Both sides of Decision 2. Accepted, with the value of the base:
   an invalid form after a `return`, after a `break`, as a later
   operand of a tuple and of a binary expression, after an `if`
   whose arms both return, and in a const fn called only from such
   code. Rejected: an invalid form after an `if` with one arm
   returning, when the call takes that arm. A condition that
   always returns: rejected with an invalid `else` body, accepted
   with only an invalid arm. Each structural form has a witness:
   block, operands, call arguments, `if`, `match`, `loop`, `while`
   and `for`.
5. Selection and order, as Decision 5 states: an unused `const`
   item and an unused const fn with an invalid form; that const fn
   referred to from a branch that never runs; a body with an error
   of its own and two invalid callees; two invalid callees called
   in each order; two used constants in each order; an error of
   HIR lowering together with one of the pass; two kinds in the
   arm and the `else` body of one `if`, in each order. Each
   outcome is the old core's, in a new process each time, or is
   listed under Decision 6.
6. Every check in the boundary's inventory has a witness, or a
   reason why it isn't a check of the source.
7. The forms in an ordinary function, used and unused, with their
   outcome and route on the base and on the candidate.
8. The pass, planted out, breaks the witnesses of Control 1. The
   pass with Decision 2 planted out breaks the accepted witnesses
   of Control 4.
9. The public admission corpus is replayed against the published
   base in both native modes. No source changes its outcome or its
   route.
10. The file gate and both project screens, image to image against
    the published base.
11. Before signing, the whole Rune library test binary in separate
    build state. Then one complete final on the signed candidate,
    under the limits of ADR 0338.

## Constraints

- No new third-party crate. Tabs.
- No refusal at run time.
- A valid program keeps its results, effects and route. A program
  that the old core accepts is valid here.
