# ADR 0343: Const calls and templates are constants

Status: Proposed

Base: published f562c486 (ADR 0342).

## Problem

Two forms keep three of the 76 legacy-only cases from the new
engine. Each was measured on the image tested for the published
8f4d4c09 (ADR 0340), and again by the boundary on the image tested
for the base.

A call of a `const fn` in a function body. The lowering refuses
`hir::Call::ConstFn` outright, with `ConstCall`. L1 cases 00404 and
00405 share the one source that does this. The same call in a
`const` item already runs on the new engine. The old core evaluates
the call when it compiles, once for each call site. A `println!` in
the const fn prints once, before the program starts, even when the
call sits in a loop or in a branch that never runs. A local
variable as an argument is a compile-time error, and so is an
overflow in the body, reported at the call. Each run of the site
yields a fresh value: a push to the vector one call returned
doesn't show in the next. A const fn used as a value is a
compile-time error.

A format template given by a constant expression. L1 case 00291
ends with `format!(const { "{" + "}" }, 10)` and is refused with
`FrontendDiagnostic`. So are `format!(T, 10)` with `const T = "{}"`
and `println!` with a constant block. The old core formats 10 in
each. A template that lacks an argument is a compile-time error.
The constant block alone, outside a template, already runs on the
new engine. A constant block that prints while it is evaluated,
used as a template, prints twice on the old core.

ADR 0341 left both forms out because the new engine's constant
evaluation accepted invalid code that it didn't reach. ADR 0342
checks a constant before it is evaluated. Its check starts from a
`const` item's expression or block. A const fn called from a
function body isn't such a root yet.

ADR 0342 records two places where the old core and the new engine
differ for a `const` item. A match guard that always returns makes
the old core pair later arms with the wrong bodies:

    const fn f(x) { match x { 0 if { return 7; } => 5, _ => 9 } }

gives 9 for `f(1)` on the new engine and 5 on the old core. When
the wrongly paired body prints, the old core prints its line too.
And the old core rejects an invalid form after a `continue` in a
loop, which the check accepts. Today a file with `f(1)` in a
function body takes the legacy route and shows the old core's
outcome. Once the call runs on the new engine it shows the new
engine's.

## Decision

1. A call of a const fn in a function body is lowered as the
   constant it evaluates to. The evaluation is the one that serves
   `const` items. The new engine gains no second evaluator. Each
   run of the call site yields a fresh value.
2. The check of ADR 0342 applies to a const fn called from a
   function body and to a constant template, with the same rules
   and the same one-sided limit: it never rejects a program that
   the old core accepts.
3. The program's output equals the old core's, including an effect
   of the evaluation, exactly once, whichever route the file takes
   in the end. If that can't be had for a const fn whose evaluation
   has an effect, that form keeps its refusal and Results count it.
4. The compile-time errors of const calls stay with the shared
   front end: a variable argument, an error in the body, and a
   const fn used as a value. Their text, position and route don't
   change. Where the evaluation fails, the call keeps its refusal,
   so the error is reported as on the base.
5. A format template given by a constant expression is accepted
   wherever a literal template is, when the new engine's constant
   evaluation yields its string without an effect. It then behaves
   as the literal of the same text does. Where that evaluation
   declines, as for a template that prints while it is evaluated,
   the form keeps its refusal, and its route, output and errors
   stay as on the base. The macros that take a template are
   `format!`, `print!`, `println!`, `panic!`, `assert!`,
   `assert_eq!` and `assert_ne!`. A template error stays a
   compile-time error with its text and position.
6. The old core's measured outcome is the reference for the result,
   the output, the error text and its position. The route is
   recorded apart from the output. One difference is accepted, as
   it already is for `const` items since ADR 0259. The new engine's
   constant evaluation counts its own work, not the legacy
   instruction budget. An evaluation that the legacy budget stops
   with an error may complete on the new engine and yield its
   value. No threshold is invented to imitate the legacy budget.
   The two differences that ADR 0342 records for `const` items are
   accepted for the new forms as well. Where a guard always
   returns, the new engine's match is the reference for the value
   and for the effects: the old core's pairing is a defect and
   isn't kept. A program with an invalid form after a `continue`
   is accepted. No analysis is added to hold such a file on the
   legacy route.
7. The two parts are independent. A part whose cause needs a change
   beyond Decision 8 is reported, leaves this record before signing
   and gets its own. The other part goes on.
8. Scope: the new engine's front end and lowering, and the roots
   from which the check of ADR 0342 starts. No change to the legacy
   engine, and none to what the shared constant evaluation yields.
   No new opcode, and the bytecode format, argument passing and
   host ABI don't change. Record the corpus, the command and the
   count of legacy-only cases on the base and on the candidate. The
   base count is 76. Const calls are expected to move two cases and
   templates one, which gives 73. Any other count is a finding to
   explain.

## Controls

1. Const calls with literal arguments, constant expressions and
   nested const calls as arguments, recursion, string and vector
   results, a call in a struct literal, in a loop and in a branch
   that never runs, a const fn declared inside a function, and one
   const fn used by a `const` item and by a function body. Each
   runs with native code on and off, asserts the new engine's
   route, and equals the old core.
2. A vector returned at one call site is changed. The next run of
   that site and another site of the same call are unchanged.
3. A const fn that prints. The output equals the old core's and
   the line appears once, on the new engine's route and when the
   file falls back for an unrelated form planted beside it. If the
   form keeps its refusal under Decision 3, the control pins that
   route and output.
4. The compile-time errors of Decision 4, in a branch that runs and
   in one that doesn't, each with the text, position and route it
   has on the base. A pure const fn that exceeds the legacy budget
   and not the new engine's, as a `const` item, as a call in a
   body and as a template: the item is unchanged from the base, and
   the other two now yield the value. One that exceeds the new
   engine's own budget keeps its refusal and the base's error.
5. The check of ADR 0342 from the new roots: an invalid form of
   each of its kinds in code that a const fn called from a function
   body doesn't reach, and in a constant template's block. Each
   reports the old core's error. The accepted programs of that
   ADR's Control 4, called from a function body, stay accepted with
   their value.
6. Templates from a constant block and from a named constant, in
   each of the seven macros: no argument, positional and named
   arguments, a format specification and escaped braces. A missing
   argument and a bad specification keep their compile-time errors.
   A template that prints while it is evaluated keeps its route and
   prints as often as on the base.
7. Each of the two old refusals, planted back alone, breaks its own
   witnesses and leaves the others passing.
8. The two differences of Decision 6 from ADR 0342. The guard
   source, pure and with a printing body, and the `continue`
   source: as a `const` item, as a call in a body, as a template
   direct and named, and as an item beside an unrelated refused
   form. Each has its value, output and route on the base and on
   the candidate, and the old core's.
9. The public admission corpus is replayed against the published
   base in both native modes. Cases 00404, 00405 and 00291 move to
   the new engine with the same result. Every other source that
   changes is listed with its old and new outcome and route. A
   corpus source whose result or output changes under Decision 6
   is reported before signing.
10. The file gate and both project screens, image to image against
    the published base.
11. Before signing, the whole Rune library test binary in separate
    build state. Then one complete final on the signed candidate,
    under the limits of ADR 0338.

## Constraints

- No new third-party crate. Tabs.
- No refusal at run time.
- A program with neither form keeps its results, effects and
  route. A move from the legacy route to the new engine with the
  same exit, output and effects is within the record. So is a
  move with one of the differences Decision 6 accepts.
