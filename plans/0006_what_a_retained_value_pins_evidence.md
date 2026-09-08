# rnx 0006 evidence: what a retained value pins

Implementation of `plans/0006_what_a_retained_value_pins.md`, 2026-09-08, on
Linux x86_64 against upstream Rune 0.14.1 with no patch. Status of the
record stays proposed until reviewed. Figures are from a debug build on this
machine, with the counting allocator of record 0005.

The cut this record set out to make is not available, for a reason that
belongs upstream. What landed is the smaller half, and it is worth having.

## The primary decision is stopped

Sharing one runtime context across a session's units breaks calls into a
function value made by an earlier input. `FnOffset::call_with_vm` in
`runtime/function.rs` decides isolation from the context alone,
`Isolated::new(!same_context)`, while the fast path that stays on the
current frame requires both the context and the unit to match. Built per
input, the two never disagree, because contexts differ whenever units do.
Shared, they come apart precisely here: same context, different unit. The
frame is not isolated, the unit switches anyway, and the callee reads the
caller's stack.

The symptom is the session's own checks, which fail on the second input,
the first that calls a function value made by the first input:

```
Error: Runtime { message: "Tried to access out-of-bounds stack entry 13" }
```

That input is `(old(), f(), c(), b.value, b is Boxed)`, where `old` and `c`
are a function value and a closure from the previous input. Reverting the
one line that shares the runtime makes it pass again, which is the control.

This is a stop condition under the first release record and under this
record's own first guardrail, so the runtime is still built per input and
the defect goes upstream.

### What the stop costs, measured

One thousand inputs, each binding a distinct closure, both builds run to
completion with the ceiling raised so that neither run is truncated:

| Build | Growth over the thousand inputs |
| --- | --- |
| Per-input runtime, shipped | 796,278,335 bytes |
| Shared runtime, incorrect for the reason above | 333,314,335 bytes |

The difference is 462,964,000 bytes, which is 462,964 bytes for each of the
thousand closures. That agrees with the runtime context measured directly at
462,739 bytes, which is the check that the two figures are measuring what
they claim: each retained function value keeps one copy alive.

An earlier draft of this evidence compared the shared-runtime run against a
shipped run that the ceiling had truncated, and read a saving of two hundred
megabytes from it. That was not a comparison: one run had stopped early. The
figures above are the same workload, the same input count, and both runs
completed.

## The reproducer

`tests/upstream_isolation.rs` is a standalone reproducer that needs nothing
from the session: two units compiled from one context, a function value made
by the first and called from the second, run once with a runtime context
each and once with one shared. The separate-context arm returns 42. The
shared arm fails, here with an out-of-bounds instruction pointer, where the
session failed with an out-of-bounds stack entry; both are the
callee running against the caller's frame, and the shape of the two units
decides which surfaces.

It is kept as a test rather than a scratch file, so the day upstream changes
this the shared arm starts returning 42 and that test fails. Its message
says what that means: revisit this record, and re-run the session-semantic
gates of the first and second records before enabling sharing. One passing
reproducer says this call shape works; it does not say the session's rules
survive, and it is a trigger to look rather than permission to change.

## What landed

- `src/session.rs`: the session owns the compile-time context, and `eval`
  no longer takes one, so an input cannot be compiled against one context
  and run against another, nor against a context modified between inputs.
  Units are held weakly beside their source maps. Entries whose unit nothing
  can call are pruned before each input registers its own. `reset` empties
  the session and keeps the context, so a reset clears what the session
  holds rather than the interpreter it runs on.
- The error lookup upgrades weak references and compares identity. A unit
  that raised an error is alive by definition, because the error holds it.

## Gates

| Gate | Result | Where |
| --- | --- | --- |
| 1 the closure session stops paying per closure | **red, withdrawn with its reason**: it depended on the stopped decision, and the cost of the stop is measured above | above |
| 1b the defect is reproducible outside rnx | pass: a two-unit reproducer with a passing separate-context control, kept as a test that fails when upstream fixes it | `tests/upstream_isolation.rs` |
| 2 units are released and their entries pruned | pass: after a thousand inputs that retain no closure, no unit is alive and one entry remains, the one the last input registered; after twenty inputs each retaining a closure, all twenty units are alive and all twenty entries kept; rebinding those closures releases every one | `a_unit_nothing_can_call_is_released_and_its_entry_pruned`, `a_unit_something_can_still_call_is_kept` |
| 2b a closure kept by a failed input keeps its unit | pass: an input that stores a closure through a shared handle and then panics leaves exactly one unit alive, and an error raised inside that closure later reports the input that defined it | `a_closure_kept_by_a_failed_input_keeps_its_unit_and_its_position` |
| 3 retained values still work | pass: the first release record's session rules run unchanged in the self-check, including a retained closure and function value calling their original definitions | `rnx` with no arguments |
| 4 diagnostics still point at the defining input | pass: the 0002 mapping tests are unchanged and green, including an error in a function defined two inputs earlier and one through a retained closure | `src/session.rs` tests |
| 5 every ordinary error finds its position; the fallback is only a fallback | pass: four runtime errors, in the current input, in a declared function, through a retained closure, and an index out of range, all report a position. Separately, with the entries artificially cleared, the failure path reports the position as unavailable rather than panicking | `every_ordinary_runtime_error_finds_its_position`, `a_missing_entry_reports_the_position_as_unavailable` |
| 6 nothing regresses | pass: 46 unit tests, 13 pseudo-terminal tests, and 2 reproducer tests green, formatting clean, spike self-checks pass | whole suite |

### Gate 2, measured

One thousand inputs that rebind a single name, which retain no closure:

| Build | Growth over a thousand inputs |
| --- | --- |
| Before this cut | 3.7 MB |
| After | 41,732 bytes |

About ninety times less, and the same workload on the same machine both
times.

## A correction to the fifth record's comparison

The fifth record's evidence set a thousand closure inputs against a thousand
integer inputs and read the difference as what a closure pins. The two
workloads differ in a second way that was not noted: the closure workload
binds a thousand distinct names, while the integer workload rebinds one. A
unit's prelude restores every published binding by name, so the closure
workload's units grow with the session and the integer workload's do not.
The difference between those two figures therefore mixes what a closure
pins with what a thousand names cost, and only the first was named. The
before-and-after figures in this record are the same workload against
itself, which is the comparison that carries.

## Forward, from this cut

The prelude is the next thing to look at, and it now has a number: a
thousand distinct closures still cost the ceiling, because each pins a unit
whose prelude lists every binding published before it. That is quadratic in
the number of names, and it is a separate cut from either of this record's.
