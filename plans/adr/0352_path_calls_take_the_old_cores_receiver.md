# ADR 0352: Path calls take the old core's receiver

Status: Proposed

Base: published 285862bd (ADR 0351).

## Problem

ADR 0350 pinned 422 combinations of a host path, an argument
position and a kind of input that succeed on the old core and
fail on the new engine. They are the last part of what ADR 0347
broke. Two are an integer `clone`:

    std::i64::clone(1u64)

The old core returns 1 as an `i64`. The new engine reports
"Expected type `std::i64` but found `std::u64`". The other 420
are a sum or a product named through one iterator type and given
another, 84 of them an empty iterator:

    std::option::Iter::sum::<i64>([2, 3].iter())

The old core returns 5. The new engine rejects the receiver.

The cause is one test. Before a host function called by path or
held as a value runs, the engine checks the first argument
against the type the path names. The old core has no such test.
Its integer `clone` converts either integer type to the named
one by a checked conversion. Its sum and product call `NEXT` on
whatever value they are given, convert each item, and start from
the typed zero or one. Measured with 24 programs, each as a
direct call and through a stored function, on both engines and
in both native modes.

The pinned list counts the inputs that were sampled. A slice of
floats given to an object iterator's `product::<f64>` fails the
same way and isn't on it.

## Decision

1. What a host function's first argument accepts is the
   contract of the function that was selected, not the type its
   path names. The engine takes that contract from what the
   function registered, as ADR 0350 did for the parameter that
   takes a string or rejects. A function whose contract is the
   named type keeps the test it has.
2. An integer `clone` by path converts either integer type to
   the named one by the old core's checked conversion, and
   returns a value of the named type. A value out of range
   fails as on the old core.
3. A sum or a product by path pulls from the value it is given
   by `NEXT`, converts each item and reduces with the checked
   operation, from the typed zero or one. An empty, an advanced
   and an exhausted cursor have the old core's result. After
   the call an alias of the cursor is where the old core leaves
   it.
4. The order of effects and failures is the old core's: the
   arguments are evaluated left to right, the count is checked
   before any pull, and a failure after progress leaves the
   items already pulled consumed.
5. A direct call and a call through a stored function agree.
6. What fails on the old core still fails: a conversion out of
   range, a value without `NEXT`, an item that doesn't convert,
   an overflow, a moved value and a wrong count. A pull is
   charged as the old core charges it, so an open-ended cursor
   ends at the budget. The differences of ADR 0264 and the
   refusals of ADR 0349 stand.
7. Both pinned lists of ADR 0350 become empty and are removed.
   The control of ADR 0350 gains the boundary values this
   record measured, so the rule is checked beyond the inputs
   that were pinned.
8. Where reaching the real operation changes a message on a
   call that fails on both engines, Results list the old and new
   text. A message may come to equal the old core's. It doesn't
   get further from it.
9. The paths a program took on the base keep their cost. A new
   error detail doesn't change how the no-detail case is
   encoded.
10. Scope: the new engine. No change to the legacy engine or
    the public host ABI. If the fix changes a function of the
    gate's source inventory, the record declares the transition
    under ADR 0346. The count of legacy-only cases is expected
    to stay 67. Any other count is a finding to explain.

## Controls

1. The programs of the Problem and the 24 of the boundary, each
   direct and stored, with the result, its type, the output and
   the state of an alias compared with the old core in both
   native modes.
2. The control of ADR 0350 with no pinned entry, and with added
   inputs: a nonempty slice of floats, an advanced cursor, an
   exhausted cursor, items of both integer types, an item that
   doesn't convert after one that does, and an overflow. Results
   give the counts before and after.
3. The order of Decision 4, with an argument that prints and a
   cursor whose items print: the output up to the failure equals
   the old core's.
4. An open-ended cursor given to a sum ends at the budget with
   the old core's outcome, and leaves no live entry.
5. A function whose contract is its named type still rejects
   another type. Results name the functions checked.
6. Each fix's old behavior, planted back, breaks its own
   witnesses and the control of ADR 0350.
7. The public admission corpus is replayed against the published
   base in both native modes. Every source that changes is
   listed with its old and new outcome and route.
8. Pinned verdicts follow the engine. Results list the rows.
9. The file gate and both project screens, image to image
   against the published base. Results give the encoding of the
   no-detail case on the base and the candidate.
10. Before signing, the whole rune-suite stage, the root-tests
    stage, the cross stage and, if `tools/` or `ci/` change, the
    final-controls stage, each by its stage command. Then one
    complete final on the signed candidate, under the limits of
    ADR 0338.

## Constraints

- No new third-party crate. Tabs.
- No refusal at run time.
- A program that succeeds on the base keeps its results, effects
  and route.
