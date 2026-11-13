# ADR 0267: an opaque host value in an execution session

Status: Proposed

Base: published 093e7cafb4a2bbde6bf71246369d87f708455765 (ADR 0266).

## Problem

Four rows of the pinned session table are "Not classified": the methods
of `interchange::Dense` (shape, names, dtype and its display). The
table cannot even ask the question, for two reasons:

- No session input produces a `Dense`. Only an adapter loaded from a
  package does, so the producer of the table has no receiver to call
  the methods on.
- The execution engine has no representation for a value whose type is
  defined by the host and whose contents it cannot see. Its host bridge
  reports "&Dense: no Root representation".

`Dense` is the smallest case of something much larger. Every frame,
series, tensor and model an adapter hands to a session is such a
value. Until one of them can be held in a binding, passed back to its
own methods and displayed, the execution engine cannot run the work
rnx exists for.

## Goal

An execution session can receive an opaque host value from a host
function, keep it in a binding across inputs, pass it by reference to
that type's methods, display it, and release it exactly once; the four
rows are measured and classified.

## Required behavior

- What a session can do with an opaque value is stated and tested:
  bind, rebind, move, read through a method taking a shared reference,
  display at the prompt and through the presenter, drop. What it
  cannot do yet is refused by name at admission or as a compile error,
  never at run time and never by falling back silently: cloning,
  equality, a method taking a mutable or owned receiver, storing it
  inside a vector, object or tuple, capturing it in a closure,
  returning it from a script function, using it in a constant. The
  step-2 note proposes which of these are cheap enough to admit here;
  the rest keep their refusals.
- The value is owned by the session store or its equivalent: its
  destructor runs exactly once, when the last binding goes, when the
  session ends, and when an input fails after creating it. A use after
  move is the existing truthful error.
- Type identity is checked: a method of one host type is never run on
  a value of another, and the failure is a diagnostic naming both.
- A way to obtain a `Dense` in the committed tests that does not need
  a package or the network. It must not be reachable from a shipped
  session; if it has to be registered in a test build only, say how
  that is guaranteed.
- The four rows are produced by measurement on both engines and get
  the class they measure to. "Not classified" reaches zero or the
  results say exactly what remains and why.
- Nothing for types other than `Dense` is claimed here, but the
  mechanism is not written for `Dense` alone: the note says what a
  second type would need.
- No instruction is added to programs that hold no opaque value; the
  fixed-route bands show it.
- No inventoried executor or policy item is edited. The note lists
  every such item the design would touch. If there is one, work stops
  with the note: that edit is the project owner's decision, and the
  note is what the owner needs to decide it.

## Not in this record

Adapters and packages themselves, `:dep` with an adapter under the
execution engine, mutable receivers, asynchronous host functions, the
other refused host functions.

## Acceptance

The capability table in the results with a control per row; the
classified rows; ownership controls under Miri, one compile per test,
including failure paths; a final in required mode with the fixed-route
bands.
