# ADR 0349: Held iterators match the old core

Status: Proposed

Base: published 0f5efd6b (ADR 0348).

## Problem

ADR 0345 let an iterator be held as a value on the new engine.
Two kinds of program are admitted there and then fail at run
time, where the old core runs them. Measured on the image tested
for ADR 0348, in both native modes, with the automatic route and
with the legacy engine forced:

- A text cursor made by `skip(n)` or `take(n)` on `chars()` or
  `split(..)` that leaves the body that made it and is advanced
  later fails with `store: InvalidCode`:

      fn make() { "abc".chars().skip(1) }
      let it = make();
      it.next()

  The old core returns `Some('b')`. The same failure follows
  when the cursor is passed as an argument, kept in a vector or
  a tuple, captured by a closure, or picked by an `if`, and when
  it is advanced by `for`. Made and advanced in one function
  body it runs. The cause, from the boundary: the lowerer knows
  the cursor's shape only where it was made, and the generic
  advance knows text leaves and ranges but no adapter. The same
  cursor on a vector's or a range's iterator is refused at
  compile time with `Method` and runs on the legacy route.
- A method the old core resolves on a held iterator fails with
  "no method": `len` on the iterator of a range, of an inclusive
  range and of an object, and `clone` on the iterator of an
  option, by method and by `std::clone::clone`. The old core
  returns 4 for `let it = (0..5).iter(); it.next(); it.len()`.
  The cause: `Len` and `Clone` test a broad value tag and reject
  every iterator before its concrete type is looked at.

Where both engines fail, the text differs in 27 measured cases.
In 25 the free `clone` names `std::slice::Iter` whatever the
receiver is. In two, `len` on a skip or take cursor over
`chars()` reports a missing method where the old core reports
the child's missing `LEN`.

Neither kind is in the public admission corpus, so its replay
didn't show them.

## Decision

1. A program the new engine admits doesn't fail at run time with
   an error of the engine's own, and doesn't fail with "no
   method" on a call the old core performs. It runs with the old
   core's outcome, or it is refused at compile time and keeps
   the legacy route.
2. A text skip or take cursor that can reach an advance where
   its shape isn't known is refused at compile time, as the
   vector and range forms are. Running it needs a charged,
   resumable advance for a held chain, which is a later record.
   The rule is one rule for both adapters, nested chains and
   every text owner the store represents, and it is complete by
   construction: no admitted program delivers a held adapter to
   the generic advance. A cursor made and advanced in one body
   keeps the route and outcome it has on the base.
3. Each call of the Problem's second kind runs with the old
   core's outcome, chosen by the receiver's concrete type. The
   receiver can be typed only at run time, so a refusal can't
   cover these. The record doesn't newly admit the iterator
   library: a call refused at compile time on the base stays
   refused.
4. Decision 3 is checked from the installed context, not from a
   list written by hand. The join is the concrete receiver hash
   with the method identity, including its specialization. Each
   pair the new engine admits is equal to the old core in its
   measured outcome, or it is on a pinned list of intentional
   differences, each entry naming the pair, the input, both
   outcomes and the earlier ADR. The check fails if an entry
   stops differing or an unlisted pair differs. The entries are
   `next` on the three open range iterators at the limit of the
   type, where ADR 0264 keeps "Numerical overflow" and the old
   core wraps or panics. A new entry needs a ruling.
5. Where both engines fail on a held iterator, the new engine's
   message and position are the old core's. The 27 cases of the
   Problem follow.
6. The paths a program took on the base keep their cost. The
   `Len` arm keeps its inline container test and length, with
   the iterator case behind what was the error branch. A new
   error detail doesn't change how the no-detail case is
   encoded.
7. The old core's measured outcome is the reference for the
   result, the output, the error text and its position. No rule
   is keyed to the names of these programs where a general rule
   serves.
8. Scope: the new engine. No change to the legacy engine or the
   host ABI. `invoke_args` is in the gate's source inventory, so
   the record declares the transition under ADR 0346. The count
   of legacy-only cases is expected to stay 67. Any other count
   is a finding to explain.

## Controls

1. The cursor of the Problem on each way out: returned, passed,
   kept in a vector, kept in a tuple, captured, picked by an
   `if`; advanced by `next` and by `for`; with `skip`, `take`
   and a nested chain; on each text owner. Each is refused
   before any effect and has the old core's outcome. The local
   forms keep the execution route.
2. Every program whose route changes is listed with its old and
   new route. A program that escaped a cursor and only dropped
   or observed it may move to the legacy route with the same
   output; Results name each as an exception to keeping routes.
3. Each call of Decision 3 on its iterator, before and after a
   `next`, including an exhausted take and a moved inner
   adapter. `len` on a vector's iterator keeps "no method" on
   both engines.
4. The check of Decision 4 over every iterator type the heap
   holds. Results give the number of types, registrations and
   pairs in each class, name the types, and say how the
   installed `Split` owners map to those the store represents.
   The open ranges compare equal on finite prefixes.
5. The 27 cases of Decision 5, each equal to the old core.
6. Each fix's old behavior, planted back, breaks its own
   witnesses.
7. A heap control: cursors made, stored and dropped leave zero
   live entries and zero payload bytes.
8. The public admission corpus is replayed against the published
   base in both native modes. Every source that changes is
   listed with its old and new outcome and route.
9. Pinned verdicts follow the engine. An admitted contract row
   compares exactly with the old core. Results list the rows.
10. The file gate and both project screens, image to image
    against the published base. Results give the instructions
    for each call of the store's `get` and `len` on the base and
    the candidate, and the encoding of the no-detail case on
    each.
11. Before signing, the whole rune-suite stage, the root-tests
    stage, the cross stage and the final-controls stage, each by
    its stage command. Then one complete final on the signed
    candidate, under the limits of ADR 0338.

## Constraints

- No new third-party crate. Tabs.
- No refusal at run time.
- A program with none of these forms keeps its results, effects
  and route.
