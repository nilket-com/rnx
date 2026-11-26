# ADR 0345: Vectors sort and held iterators collect

Status: Proposed

Base: published 5aeda4b2 (ADR 0344).

## Problem

One source keeps three legacy-only cases, L1 00338, 00339 and
00340, from the new engine. Its three tests collect an iterator
into a vector and into an object, and sort a vector, each in three
spellings:

    let it = [4, 3, 2, 1].iter();
    let values = it.collect::<Vec>();
    let values = Iter::collect::<Vec>(it);
    let c = Iter::collect::<Vec>;
    let values = c(it);

    values.sort();
    Vec::sort(values);
    let f = Vec::sort;
    f(values);

Measured on the image tested for the published 5aeda4b2 (ADR 0344),
with the automatic route and with the legacy engine forced:

- A method of an iterator held in a local is refused with `Method`:
  `collect::<Vec>`, `collect::<Object>`, `collect::<String>`,
  `count`, `rev` and `map`. So is `collect` on an iterator that
  arrives as a parameter. `next` on a held iterator runs, and so
  does the chain `[..].iter().collect::<Vec>()`.
- `sort` without a function is refused with `Method` on any
  vector. `sort_by` with a closure runs.
- A host method called by its path and a host function as a value
  are refused with `Native`.

The boundary of this record found that the third spelling needs
more than a rewrite of the call. On the old core one stored
`std::io::dbg` value accepts zero, one and two arguments in one
script, a host value is called through a parameter after an index
chosen at run time, and a wrong receiver by path names the owner
type of the path. The new engine's function values have a body of
its own code with a fixed argument count. Host functions by path
and as values therefore get their own record, the next one. This
record takes the two method forms. The three cases use all three
spellings, so they keep their legacy route until that record.

On the old core a held iterator is one value: a second `collect`
from it yields an empty vector or object, and a `collect` after
some `next` calls yields the remainder. A repeated key keeps the
last value. Collecting items that aren't pairs into an object
fails with "Expected type `std::tuple::Tuple`".

`sort` changes the vector under all its names and returns unit.
It doesn't compare as the `cmp` method does: `[1i64, 2u64]` fails
under `sort` with "Cannot apply `CMP`" and sorts under
`sort_by(|a, b| a.cmp(b))`, and `[1, "a"]` fails under both with
different errors. A NaN fails with "Cannot perform a comparison of
the floats", and a sort during an iteration over the vector with
"Cannot write the receiver: it is being read". After the first
failed comparison the old core goes on ordering by address before
it raises the error.

## Decision

1. `sort` on a vector runs on the new engine with the old core's
   comparison, order, result, error text and error position. The
   comparison is the one the old core's `sort` uses, not the `cmp`
   method.
2. A failed sort raises the old core's error. The order in which
   it leaves the vector isn't specified: the old core's depends on
   addresses. Every element is still in the vector, once.
3. The forward consumers that the new engine already runs on a
   direct chain run on an iterator that is held as a value: in a
   local, a field, an element, a capture or a parameter. They
   include `collect` into a vector and an object. The iterator is
   one value, as on the old core: what one use consumes is gone
   for the next, under every name the value has.
4. Other methods of a held iterator keep their refusal: adapters
   kept as values, access from the back, and anything that needs
   the length. Results list each method measured and its route.
5. A receiver that isn't proved an iterator, and a script type
   with a method of the same name, keep the rules for the provider
   and for a missing method that hold on the base.
6. Sort and held iterators are independent parts. A part whose
   cause needs a change beyond Decision 8 is reported, leaves this
   record before signing and gets its own.
7. The old core's measured outcome is the reference for the
   result, the output, the error text and its position. The route
   is recorded apart from the output. No rule is keyed to the name
   `sort` or `collect` where a general rule serves.
8. Scope: the new engine's front end, lowering, library
   enrollment and a helper in its store off the hot path, reached
   through an existing opcode. No change to the legacy engine. No
   new opcode and no change to call dispatch. The bytecode format,
   argument passing and host ABI don't change. Record the corpus,
   the command and the count of legacy-only cases on the base and
   on the candidate. The base count is 70 and the expected count
   is 70. Any other count is a finding to explain.

## Controls

1. `sort` on integers, strings, floats, nested vectors and an empty
   vector; through an alias, a field and an argument; its unit
   result; equal elements keep their order where the old core's
   do, `0.0` and `-0.0` among them.
2. Failed sorts: mixed integer widths, an integer with a string, a
   NaN, and a sort during an iteration over the vector. Each gives
   the old core's error and position, and the vector keeps every
   element. The same vectors under `sort_by` with `cmp` keep their
   base outcome.
3. Each covered consumer on an iterator held in a local, a field,
   an element, a capture and a parameter. A second use of the same
   iterator, a use after some `next` calls, a use through a second
   name, and a consumer that stops early followed by another.
   Items that aren't pairs and a repeated key for an object.
4. A write to the vector while a held iterator over it is live,
   and after the iterator is used up or dropped, with the old
   core's outcome.
5. Each method of Decision 4 that was measured keeps its route and
   output. An unproved receiver and a script method of the same
   name keep theirs.
6. A heap control for a held iterator that is collected and one
   that is dropped unused: the store returns to zero live entries
   and zero payload bytes.
7. Each part's old refusal, planted back alone, breaks its own
   witnesses and leaves the others passing.
8. The public admission corpus is replayed against the published
   base in both native modes. Every source that changes is listed
   with its old and new outcome and route. L1 00338, 00339 and
   00340 are expected to stay on the legacy route, with the
   refusal named.
9. Pinned verdicts follow the engine. The embedding inventory and
   every contract row whose disposition changes are brought up to
   date through their own mechanisms, and an admitted contract row
   compares exactly with the old core. Results list the rows.
10. The file gate and both project screens, image to image against
    the published base.
11. Before signing, the whole rune-suite stage, every library
    configuration and every contract family, and the root-tests
    stage, each by its stage command. Then one complete final on
    the signed candidate, under the limits of ADR 0338.

## Constraints

- No new third-party crate. Tabs.
- No refusal at run time.
- A program with none of these forms keeps its results, effects
  and route. A move from the legacy route to the new engine with
  the same exit, output and effects is within the record.
