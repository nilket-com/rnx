# rnx 0006: what a retained value pins

Status: proposed 2026-09-08. The sixth record of rnx. The allocation ceiling
of the fifth made the cost of a long session visible, and the first thing it
showed was that a session which retains closures grows by about half a
megabyte an input and reaches the default ceiling in about a thousand
inputs. This record decides what a retained value is allowed to keep alive.
It corrects an attribution: the cost is not the compiled units.

## Context

### What was measured

Two sessions of a thousand inputs each, on the counting build:

| Session | Growth | Per input |
| --- | --- | --- |
| Each input binds a closure | about 536 MB, reaching the ceiling | about 536 KB |
| Each input binds an integer | 3.7 MB | about 3.7 KB |

The second figure is total growth per input, not a unit size: it includes
the retained input text, the generated source, the source map, the binding
itself, and the session's other bookkeeping, of which the unit is one part
that this cut has not measured on its own. What the two figures bound is the
difference between them, about a hundred and forty five to one, which is
what a retained closure pins beyond what an ordinary input costs.

### What a closure pins

A Rune function value made from a compiled function is a `FnOffset`, and
`FnOffset` holds an `Arc<RuntimeContext>` beside its `Arc<Unit>`. The
session builds a fresh runtime context for every input, in `execute`, from
the compile-time context. One runtime context measures 462,739 bytes. So
every retained function value keeps its own half-megabyte copy of the
standard library's runtime alive, and a session that retains a thousand of
them keeps a thousand copies.

Nothing about that is required. The runtime context does not change between
inputs: the host module is installed once, before the session starts, and
the context is immutable afterwards. Every input builds a copy of the same
thing.

### The smaller question beside it

Even with the contexts shared, the session retains every compiled unit for
as long as it lives, so that a runtime error inside a function defined many
inputs ago can be mapped back to the input that defined it, which the second
record decided. At roughly a hundredth of the other cost that is a much
smaller problem, but it is still growth without a bound on it.

## Decision

### 1. One runtime context per session: attempted, and stopped upstream

This was to be the cut. It is not available in Rune 0.14.1, and the reason
is a defect in the runtime rather than anything about this session.

When a function value is called, `FnOffset::call_with_vm` decides two things
separately. Whether the call frame is isolated is decided by the context
alone, `Isolated::new(!same_context)`, while the fast path that keeps
running on the current frame requires both the context and the unit to
match. With a runtime built per input those two always agree, because the
contexts differ whenever the units do. With one shared runtime they come
apart exactly in this session's case: same context, different unit. The
frame is then not isolated, but the unit still switches, so the callee reads
stack slots laid out for the caller. The session's own checks fail with
"Tried to access out-of-bounds stack entry 13" on the first input that calls
a function value made by an earlier one.

The first release record makes a need that cannot be met downstream a stop
condition and an upstream conversation, and this record's first guardrail
says the same. So the runtime is still built per input, the saving is not
taken, and the defect goes upstream.

What does stand is the ownership half. The session owns the compile-time
context and `eval` no longer takes one per call, so an input cannot be
compiled against one context and run against another, nor against a context
a caller modified between inputs.

### 2. A unit lives as long as something can call into it

The session stops holding compiled units alive. It keeps, for each input, a
weak reference to the unit and the source map, which is small. A unit
therefore lives exactly as long as something can still reach it, a retained
closure or function value, and no longer. When a runtime error names a unit,
that unit is alive by definition, because the error holds it, so the map is
found by upgrading the weak references and comparing identity. A weak
reference that no longer upgrades belongs to a unit nothing can call, which
can raise no error.

The entry is registered before the unit runs, not after it succeeds. An
input that stores a closure through a shared handle and then panics, or is
interrupted, has left something that can still be called; its unit stays
alive through that value, and a later error inside it resolves to the input
that defined it, exactly as a successful input's would.

### 3. Dead entries are pruned at a stated point

A weak reference releases the unit's contents but not the entry that holds
it, nor the map beside it, and both would otherwise grow with every input
and lengthen the search a diagnostic makes. Before each input registers its
entry, the session drops every entry whose weak reference no longer
upgrades. Their maps go with them, because a map whose unit nothing can
call can never be consulted. The bookkeeping is then proportional to the
units still reachable rather than to the number of inputs, and the pass
costs one walk of that list per input. The retained input text is not
pruned by this cut and keeps growing; the Forward names it.

### 4. Diagnostics do not regress

The second record's promise stands unchanged: a runtime error inside a
function or closure defined in an earlier input reports that input's number
and position. Nothing in this cut may weaken it, and its gate is re-run
here rather than assumed.

### 5. What `:memory` reports does not change

The figure stays what the fifth record decided: tracked live allocation
request bytes against a ceiling. The source and map component drops the
generated source it was counting, since a retained unit's generated text is
no longer held; the component's description changes with it.

### 6. What this record does not decide

Any change to snapshot semantics, to what a session retains logically, or to
the ceiling's value; sharing anything else between inputs; and reclaiming a
reference cycle, which the fifth record measured as unreclaimable and which
no change here can reach.

## Acceptance gates

1. **The closure session stops paying per closure.** **Red, and withdrawn
   with its reason.** The saving depended on decision 1, which is stopped
   upstream. The evidence records what it would have delivered, measured on
   a build that takes it and is therefore incorrect, so that the size of the
   loss is on the record rather than assumed.
2. **Units are released and their entries pruned.** After a thousand inputs
   that retain no closure, the number of entries the session keeps is a
   small constant rather than a thousand, measured through the session's own
   count rather than argued; after a thousand that each retain a closure,
   the entries are kept, because those units are still reachable.
2b. **A closure kept by a failed input keeps its unit.** An input that
   stores a closure through a shared handle and then panics leaves that
   closure callable; its unit is not released, and a later error raised
   inside it reports the input that defined it.
3. **Retained values still work.** A closure retained across inputs still
   calls the definition it was created against, a function value likewise,
   and a struct instance from an earlier input still passes its type check.
   These are the first record's session rules and they are re-run.
4. **Diagnostics still point at the defining input.** A runtime error inside
   a function defined three inputs ago reports that input's number, line,
   and column, including after other units have been released.
5. **Every ordinary error finds its map; the fallback is only a fallback.**
   Across a battery of runtime errors, in the current input, in a function
   from an earlier input, and through a retained closure, every one reports
   a position: a missing position during ordinary execution is a defect, not
   an accepted outcome. Separately, with an entry artificially removed, the
   failure path reports the position as unavailable rather than panicking or
   guessing, which is the defensive path and is tested as such.
6. **Nothing regresses.** The gates of records 0002 through 0005 pass
   unchanged.

## Guardrails and stop conditions

1. The runtime context is built once and never rebuilt per input; if
   anything requires a fresh one, stop and record why before proceeding.
2. A unit is never resurrected from a weak reference for any purpose other
   than identifying the map of an error that already holds it alive.
3. No change to snapshot semantics rides on this cut.
4. If releasing units costs a diagnostic that record 0002 promises, the
   release is wrong and the gate is red.

## Risks

- **A shared context changes what a retained closure sees.** It does not:
  the context is immutable and identical between inputs today, so sharing
  it removes copies rather than changing behaviour. Gate 3 is the control.
- **Weak references make a diagnostic depend on liveness.** By construction
  the only units that can raise an error are alive; gate 5 covers the case
  where the reasoning is wrong anyway.

## Forward

The upstream report for the isolation defect, and the shared runtime once it
is fixed. The prelude a unit carries, which restores every published binding
by name and so grows with the number of names, making a unit that a closure
pins grow with the session: that is what a thousand distinct closures now
cost, and it is a separate cut. The retained input text, kept for
diagnostics and growing without a bound of its own. And per-binding
attribution of memory, still waiting on a size that Rune does not expose.
