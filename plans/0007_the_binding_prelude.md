# rnx 0007: the binding prelude

Status: proposed 2026-09-08. The seventh record of rnx. Every input the
session compiles begins with a line restoring each published binding by
name, and ends by naming every published binding again to publish it, so a
unit grows with the number of names a session has published rather than with
the size of the input a person typed. This record measures what that costs
and decides to restore only what an input may reference and to publish a
delta.

## Context

### What was measured

Two workloads, at four sizes, each input a `let` that publishes a value.
One publishes a new name every time; the other rebinds a single name. No
closures are retained, so record 0006 releases every unit. Generated source
is the last input's, taken from `:debug`. Growth is tracked live allocation
request bytes across the session. Time is the whole session, wall clock, on
a debug build.

| Inputs | Names | Generated source of the last input | Growth | Time |
| --- | --- | --- | --- | --- |
| 100 | distinct | 3,434 B | 25,842 B | 54 ms |
| 100 | one rebound | 115 B | 9,319 B | 35 ms |
| 200 | distinct | 7,134 B | 47,846 B | 147 ms |
| 200 | one rebound | 115 B | 13,697 B | 56 ms |
| 400 | distinct | 14,534 B | 91,150 B | 444 ms |
| 400 | one rebound | 115 B | 22,241 B | 98 ms |
| 800 | distinct | 29,334 B | 177,854 B | 1,530 ms |
| 800 | one rebound | 115 B | 39,329 B | 185 ms |

Three things follow.

**Two generated parts grow with the session, not one.** The generated source
of one input is linear in the number of published names, about 37 bytes
each, and constant when a single name is rebound. The restoring prelude is
the larger part, but the publication expression that ends the unit lists
every published name too, so both halves grow with the session. An earlier
draft of this record called the prelude the whole difference; it is not.

**Time grows superlinearly, and every session pays it.** Each doubling of the
input count multiplies the distinct-name session's wall clock by about three
and a half, against about two for the rebound-name session. These are whole
sessions, process start to exit: they measure what a person waits for, and
they do not separate compilation from execution, publication, or the loop
around them. The shape matches the generated source growing with the
session, and the two workloads differ in nothing else, but attributing it to
compilation specifically is not measured here and is not claimed.

**Memory is no longer quadratic, but only because units are released.**
Growth is linear in both workloads, since record 0006 lets a unit go as soon
as nothing can call it. The quadratic returns the moment a function value
pins its unit: a session with N published names that retains K function
values retains K units of O(N) each. That is what record 0006 measured as a
thousand closures reaching the ceiling.

### What could not be measured

A unit's own storage size. `UnitStorage::bytes` exists but is documentation-
hidden, and the `Logic` that holds the storage exposes none of its fields, so
0.14.1 offers no public way to ask a unit how large it is. The figures above
are generated source, which the session owns, and allocation, which the
counter sees. The unit's internal size is inferred from neither and is not
claimed.

## Decision

### 1. Restore only what an input may reference

The adapter emits a restoring line only for a published name the input may
reference. An input that may reference three names compiles a prelude of
three lines whatever the session has published.

### 2. Publication becomes a delta, merged into the session's state

The unit can no longer end by naming every published binding. A name it did
not restore is not a local, so that expression would not compile, and the
expression is itself proportional to the session, so narrowing the prelude
alone would move the cost rather than remove it.

The session holds its bindings on the host side rather than inside a Rune
object, and hands each input only the bindings it restores. That is what
makes publication atomic: building the object an input receives and reading
the delta it returns are both fallible and both happen before anything is
committed, and the commit is an insert into a map on this side, which cannot
fail. A partly published delta is therefore not a state the session can
reach.

The unit returns only the bindings this input could have changed: the ones
it restored, which it may have reassigned, and the ones it declared. The
session merges that delta into the state it holds, entry by entry, rather
than replacing that state wholesale.

What a person can observe is unchanged. A binding the input never mentioned
keeps its value and its identity, including its identity as a shared handle,
because its entry is never touched. A binding the input reassigned or
shadowed is replaced by the value the delta carries. On failure nothing is
merged, so no rebinding is published, while a mutation already made through
a shared handle stands, because the session's entry and the input's local
were the same handle throughout.

### 3. The scan is conservative, syntactic, and covers format captures

It lexes the input with Rune's own lexer and takes any published name that
appears as an identifier token, wherever it appears and whatever it means
there.

That alone is unsound. `format!("{x}")` reads `x`, and `x` appears only
inside a string literal, never as an identifier token; the same holds for
`println!` and the other builtin format macros. So the scan also reads the
contents of every string literal and takes the leading identifier of each
`{...}` group. This over-approximates deliberately: a name inside a string
that is not a format argument costs one restoring line, which is correct and
cheap.

Reading the raw literal is not enough either, because the formatter reads
the decoded string: `format!("\\u{7b}x}")` decodes to `{x}` and captures
`x`, which a raw scan misses. So the literal is decoded first. Rune's own
decoder is crate-private, so this one mirrors the escape table in its lexer
and recognises nothing beyond it; an escape it does not know means the
string's contents are unknown, and an unknown string could name anything, so
that input restores every published name. Widening is always available and
is the only honest answer to a string this scan cannot read.

This is not completion's scan. Completion suppresses strings, comments, and
template interpolations on purpose, and every one of those suppressions
would be a defect here. Template interpolations reach this scan as
identifier tokens already, because Rune desugars a template before the
tokens are seen.

### 4. What this record does not decide

Any change to snapshot semantics, to what a session publishes as a person
observes it, or to the diagnostics of record 0002; the retained input text,
which grows with the session for its own reasons; and `run`, which compiles
one unit from a file and has no prelude at all. `eval` goes through this
same session adapter against an empty session, so its prelude and its delta
are both empty; it is affected only in that it shares the code.

## Acceptance gates

1. **The prelude and the publication follow the input.** After a hundred
   published names, an input mentioning two of them generates a prelude of
   two lines and a delta of the names it touched, and its whole generated
   source is within a small constant of the same input in a session with two
   names.
2. **The session stops slowing down.** The eight-hundred-input distinct-name
   session, 1,530 ms before this cut, comes close to the rebound-name
   session's 185 ms, with both figures in the evidence, the same machine, and
   the same measurement as the table above.
3. **A pinned unit stops carrying the session.** A session that publishes
   many names and retains one closure retains a unit whose generated source
   is the size of the input that made it, not the size of the session.
4. **State semantics are preserved, case by case.** Controls for
   assignment to a restored name; shadowing a restored name; destructuring;
   a closure capturing a restored binding; two names aliasing one shared
   handle, mutated through one and read through the other; a binding no
   input has mentioned for many inputs; and an input that mutates a shared
   handle and then fails, which publishes no rebinding while the mutation
   stands.
5. **Diagnostics are unchanged.** The origins of record 0002 pass, including
   a runtime error inside a function defined many inputs earlier and one
   through a retained closure.
6. **Format captures are restored.** An input whose only mention of a
   published name is `format!("{name}")`, and one using `println!` the same
   way, both read the binding.
7. **Nothing regresses.** The gates of records 0002 through 0006 pass.

## Guardrails and stop conditions

1. If a published name can be read without appearing either as an identifier
   token or as the leading identifier of a `{...}` group in a string literal,
   this decision is unsound; stop and record the case.
2. The scan may over-approximate and may never under-approximate: when in
   doubt a name is included.
3. What a person can observe about the session's state does not change; only
   the mechanism does.
4. If narrowing changes any diagnostic position, the mapping is wrong and
   the gate is red.
5. This scan depends on two things Rune could change under an upgrade: its
   escape table and its format-string syntax. On any change to the pinned
   Rune version, re-run the escape and capture cases before trusting the
   scan, and keep the widening fallback: an escape or a group this code
   cannot read must restore every published name rather than guess. A new
   escape that the decoder does not know already widens, which is the
   failure this leans on, but a new capture syntax would not, and that is
   the case to check by hand.

## Risks

- **An identifier scan is not a resolver.** It is deliberately not one: it
  over-approximates, and gate 4 is the control that an unmentioned binding
  is untouched rather than dropped.
- **A delta merge is a new way to lose a binding.** Gate 4 exists for that,
  and it tests the cases where a merge could go wrong rather than the happy
  path alone.
- **A name in a string costs a line.** That is the price of soundness
  against format captures, and it is one line.

## Forward

The retained input text, still growing with the session; a bound on how
much of it a session keeps for diagnostics; and the shared runtime context,
still waiting on the upstream fix that record 0006 reproduced.
