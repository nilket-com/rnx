# ADR 0340: A range pattern matches a range value

Status: Proposed

Base: published a42928367581ee7fb216416db059e590dc484405 (ADR 0339).

## Problem

The new engine refuses any program whose pattern names a range
type. In

    match r { Range { start, end } => start + end, _ => 1 }

the pattern names `std::ops::Range`. The lowering in
`lower/patterns.rs` knows `Option`, `Result`, `Ordering` and the
script's own structs and variants. It refuses every other type, and
the whole file then runs on the legacy route. A range written as
the matched expression, as in `match 2..5 { ... }`, is refused
before that, since a range expression isn't an accepted root of a
pattern.

There are six range types: `Range`, `RangeFrom`, `RangeFull`,
`RangeInclusive`, `RangeTo` and `RangeToInclusive`. The new engine
already builds each as its own kind of value and reads `start` and
`end` from it. Only the pattern is missing.

These were measured on the image tested for the published a4292836.
The six L1 cases 00398 to 00403 share one source. Five of its six
functions run on the new engine when given alone, in both native
modes. The sixth, `range_match`, is refused with `Match`, so all
six cases are legacy-only. The same pattern is refused with
`Condition` in `if let` and `while let`, and with `Pattern` in
`let` and in a function parameter.

The old core's outcomes for planted forms were measured the same
way. A range of another type falls to the next arm. So does a
value that isn't a range, including an object and a script struct
that have fields named `start` and `end`. A match with no arm left
yields unit. A `let` that doesn't match panics with "pattern did
not match" at the pattern. A field's pattern may be a literal or a
nested pattern, and the endpoints need not be numbers. The shared
front end rejects four forms at compile time: an unknown field, a
struct pattern that lists some fields without `..`, a tuple pattern
on a range type, and any field on `RangeFull`.

## Decision

1. A pattern that names one of the six range types is lowered on
   the new engine. It matches a value that is a range of exactly
   that type, and nothing else. Each field the pattern names is
   read from the range and matched against the field's own pattern.
2. The lowering uses what the engine has: the type test of the
   other patterns and the field read behind `r.start`. The six
   types share one rule. No type gets its own path.
3. A range expression is an accepted root of a pattern. It is
   evaluated once, start before end, before any arm is tried. No
   other kind of root is added here.
4. Every site that takes a pattern takes the range pattern: a match
   arm, `if let`, `while let`, `let`, a function parameter, and any
   nested position inside another pattern. The boundary step lists
   the sites and the lowering each one uses. A site the shared
   lowering doesn't reach is reported before signing.
5. The old core's measured outcome is the reference for the result,
   the output, the error text and its position. That includes the
   panic of a `let` that doesn't match. The route is recorded apart
   from the output.
6. The four compile-time rejections of the Problem stay with the
   shared front end. Their text, position and route don't change.
7. ADR 0339 holds. A range pattern never covers its input, so a
   match of range patterns alone can miss and proves no receiver.
8. Scope: the new engine's lowering, and its front end where a
   proof must learn the form. No change to the legacy engine. No
   new opcode, and the bytecode format, argument passing and host
   ABI don't change. Building a range, its methods and its
   iteration don't change. A pattern that names any other host type
   stays refused. Record the corpus, the command and the count of
   legacy-only cases on the base and on the candidate. The base
   count is 83. The six cases are expected to move, which gives 77.
   Any other count is a finding to explain.

## Controls

1. Each of the six types, hit: every field bound, some fields with
   `..`, and for `RangeFull` both the bare path and `{ .. }`. The
   type is named through `use` and by its full path. Each runs with
   native code on and off, asserts the new engine's route, and
   equals the old core.
2. Misses fall to the next arm: each range type against a pattern
   of another range type, and a vector, an integer, unit, an object
   with `start` and `end`, and a script struct with fields `start`
   and `end` against a range pattern.
3. Field patterns: a literal, a renamed binding, a nested tuple, a
   guard that reads the bound fields, two arms of one type that
   differ by a literal, and endpoints that aren't numbers.
4. Sites: a match arm, `if let`, `while let`, `let` and a function
   parameter, each on a hit and on a miss, and a range pattern
   nested inside a tuple, a vector and `Some`. The matched value
   comes from a local, a parameter, a function's return, a field
   and a range expression. A range expression whose endpoints are
   calls with a counted effect runs each call once, in order.
5. Ranges whose endpoints are heap values, matched and missed. An
   endpoint bound by the pattern is usable afterwards, and so is
   the range. The existing balance checks hold.
6. A match of range patterns alone that can miss, followed by a
   call of a script method on its value: the receiver isn't proved,
   and the miss reports the missing method as the old core does.
7. The four compile-time rejections, each with the text, position
   and route it has on the base.
8. The type test, planted out so that only the fields are read,
   breaks the object and script-struct witnesses of Control 2. The
   old refusal, planted back, breaks Control 1.
9. The public admission corpus is replayed against the published
   base in both native modes. The six cases move to the new engine
   with the same result. Every other source that changes is listed
   with its old and new outcome and route.
10. The file gate and both project screens, image to image against
    the published base.
11. Before signing, the whole Rune library test binary in separate
    build state. Then one complete final on the signed candidate,
    under the limits of ADR 0338.

## Constraints

- No new third-party crate. Tabs.
- No refusal at run time.
- A program without a range pattern keeps its results, effects and
  route.
