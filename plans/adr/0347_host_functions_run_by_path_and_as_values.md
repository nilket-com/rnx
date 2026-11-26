# ADR 0347: Host functions run by path and as values

Status: Proposed

Base: published e621f958 (ADR 0346).

## Problem

L1 00338, 00339 and 00340 are one source. After ADR 0345 its
method calls run on the new engine and two spellings keep it on
the legacy route with `Native`:

    use std::slice::Iter;
    let values = Iter::collect::<Vec>(it);
    let c = Iter::collect::<Vec>;
    let values = c(it);

    Vec::sort(values);
    let f = Vec::sort;
    f(values);

Measured on the image tested for the published 6336eb2a (ADR 0345),
whose product source ADR 0346 leaves unchanged,
with the automatic route and with the legacy engine forced:

- By path, `i64::abs(-2)`, `i64::max(1, 2)`, `f64::sqrt(4.0)`,
  `i64::to_string(5)`, `Vec::new()` and `String::from("a")` run.
  `Vec::len(v)`, `Vec::push(v, 3)`, `Vec::sort(v)`, `Vec::clone(v)`,
  `Vec::iter(v)`, `String::len(s)`, `Object::len(o)`,
  `Option::unwrap(o)`, `i64::cmp(1, 2)`, `Iter::next(it)` and
  `Iter::collect::<Vec>(it)` are refused with `Native`.
- As a value every host function tried is refused with `Native`:
  `Vec::len`, `Vec::sort`, `Vec::new`, `i64::abs`, `Some`,
  `std::io::println`, `std::io::dbg`, `Iter::count` and
  `Iter::collect::<Vec>`. A script function, a script method and a
  closure as a value already run.

On the old core a host function value is bound, passed, returned,
kept in a vector or an object, picked by an index computed at run
time, called more than once, and mixed with script functions and
closures in one vector. It is passed where a function is expected:
`map(Vec::len)`, `sort_by(i64::cmp)`, and `Iter::fold` as a value
takes a closure. One stored `std::io::dbg` accepts zero, one and
two arguments in one script.

A wrong receiver names the owner of the path, by path and through
a value: "Expected type `std::vec::Vec` but found
`std::string::String`", and for `Iter::collect::<Vec>` on a range's
iterator "Expected type `std::slice::Iter` but found
`std::ops::RangeIter<std::i64>`". A wrong count gives "Wrong number
of arguments 2, expected 1" at the call. `{:?}` of the value
prints "native function" and an address. `{}`, `==` and a method
call on the value fail with the old core's errors for
`std::ops::Function`.

An iterator adapter is a different case. On the new engine
`it.map(f).collect::<Vec>()` runs as one fused chain, and the
adapter's result is never a value there. On the old core
`Iter::map(it, f)` returns a lazy iterator that is stored and
advanced later, and its callback runs at each step. ADR 0345 kept
adapters held as values refused.

The new engine's function values have a body of its own code and
a fixed argument count, and a call through a value enters that
body. The boundary of ADR 0345 found that a wrapper per host
function serves the fixed counts only and loses the checks above.

The boundary of this record found three more things. The design
changes two functions of the gate's source inventory, the
executor's `fixed_intrinsic` and `call_closure`; ADR 0346 lets a
record declare such a change. A named function and a closure that
captures nothing are copies on the new engine, the third
difference of ADR 0337, and a call through a host function value
sees them as a direct call does. And some iterator calls fail on
the base where the old core succeeds: a `chars().skip(n)` or
`take(n)` cursor that leaves its statement and is advanced, and
`len` or `clone` on range, object and option iterators. Those are
bugs of the base and have their own record.

## Decision

1. A host function that the new engine runs as a method call runs
   when it is called by its path, with the receiver as the first
   argument, apart from those of Decision 5. The old core's checks
   of the receiver's type and of the argument count apply, with
   its error text and position.
2. A host function that the new engine can call, apart from those
   of Decision 5, is a value: bound, passed, returned, stored,
   picked at run time and called, alone or among script functions
   and closures, and passed where a function is expected. Each
   call has the old core's outcome.
3. The argument count is checked at the call. A host function
   that takes several counts on the old core takes them here.
4. The host function values a program can make are those its
   paths name. If one of them is a function the new engine can't
   call, the program keeps its refusal at compile time. There is
   no refusal at run time and no fallback at a call.
5. Four kinds of host function are outside this record, as
   values and by path: those that are async; those of host types
   the new engine holds as opaque values; those whose receiver or
   result the new engine has only inside a fused chain, such as
   an iterator adapter whose result would be held; and the
   iterator calls that fail on the base where the old core
   succeeds. The third kind is decided for each owner and
   instantiation, not by the name of a method. A host function
   passed to an adapter inside a fused chain is within the
   record. Each excluded function keeps its refusal at compile
   time. Results list what was measured.
6. A host function whose direct call the new engine admits only
   on a proof about an argument is a value only if that proof
   holds for every value that can reach it in the installed
   context. Otherwise the value keeps its refusal at compile
   time. The direct call's proof is unchanged.
7. A named function and a closure that captures nothing stay
   copies, as ADR 0337 decided. Where a call of this record
   reaches that difference, the contract records the outcome on
   each route, as ADR 0337 did. A host function value moves as on
   the old core.
8. `{:?}` of a host function value prints the old core's text.
   The address in it isn't specified. Other uses of the value
   that fail on the old core fail the same way.
9. The path calls and function values that run on the base keep
   their route and outcome. A script item that shadows a host
   path is selected, as on the base.
10. The old core's measured outcome is the reference for the
    result, the output, the error text and its position. No rule
    is keyed to the names of this source where a general rule
    serves.
11. Scope: the new engine. Call dispatch and the function value
    change as the design accepted at the boundary states: a
    descriptor in the code, a charged value in the store made by a
    checked instruction, and a dispatch on the kind of value in
    the existing cold call path. The path of a plain script
    function is unchanged. The record declares the two source
    transitions it makes, under ADR 0346, and changes no rule of
    the gate. No change to the legacy engine or the host ABI.
    Record the corpus, the command and the count of legacy-only
    cases on the base and on the candidate. The base count is 70
    and the expected count is 67. Any other count is a finding to
    explain.

## Controls

1. Each path call of the Problem with a right receiver, a wrong
   one, no argument and one too many. A path through a `use`
   alias and a full path.
2. Each value of the Problem: bound, passed to a function,
   returned from one, kept in a vector and in an object, picked
   by an index computed at run time, called twice, and called
   with a wrong receiver and a wrong count.
3. A vector that holds a host function, a script function and a
   closure, called in a loop. A function that returns one or the
   other by a run-time condition.
4. A host function value passed where a function is expected:
   to an iterator adapter, to `sort_by` and to a host function
   value that itself takes a closure.
5. `std::io::dbg` as one value called with zero, one and two
   arguments.
6. A script type with a method of the same name as a host method,
   and a script type that shadows a host type's name: the path
   and the value select the script item.
7. The uses of Decision 8, and a call of a value that isn't a
   function.
8. A value of a function the new engine can't call, and one of
   each kind of Decision 5: the program keeps its route and its
   output. An adapter's result that is stored and advanced later
   keeps the order of its callback's effects.
9. A heap control: host function values made, stored and dropped
   leave zero live entries and zero payload bytes.
10. The covered part's old refusal, planted back, breaks its own
    witnesses.
11. The public admission corpus is replayed against the published
    base in both native modes. L1 00338, 00339 and 00340 move to
    the new engine with the same result. Every other source that
    changes is listed with its old and new outcome and route.
12. Pinned verdicts follow the engine. The embedding inventory
    and every contract row whose disposition changes are brought
    up to date through their own mechanisms. An admitted contract
    row compares exactly with the old core, apart from the rows
    of Decision 7, which record both outcomes. Results list the
    rows.
13. The file gate and both project screens, image to image against
    the published base. The rows that call script functions and
    closures stay within their bands; a rise there is explained
    before signing.
14. Before signing, the whole rune-suite stage, every library
    configuration and every contract family, the root-tests stage
    and the final-controls stage, each by its stage command. Then
    one complete final on the signed candidate, under the limits
    of ADR 0338.

## Constraints

- No new third-party crate. Tabs.
- No refusal at run time.
- A program with none of these forms keeps its results, effects
  and route. A move from the legacy route to the new engine with
  the same exit, output and effects is within the record.
