# ADR 0274: async blocks awaited directly in an execution session

Status: Proposed

Base: published e4076bd335f823389d242eba3543c5e3b901686a (ADR 0273).

## Problem

After ADR 0273 an execution session accepts `async fn` with a direct
await, but still refuses the block form of the same thing:

	async { time::sleep(10).await; 7 }.await

The refusal sends the reader to the legacy session for code whose
meaning is already supported one spelling away.

## Decision

An execution session accepts `async { ... }.await` and
`async move { ... }.await`, where the await applies directly to the
block, in the input and inside async functions and other such blocks.
Inside the block the direct awaits of ADR 0270 and ADR 0273 are
accepted.

An async block that is not awaited where it is written is still a
future as a value and stays refused by name: bound, passed, returned,
stored, or awaited later. The refusal gives the remedy to await the
block directly. Async closures, `select` and joining futures stay
refused.

The implementer measures the legacy behavior of the direct form first
and reports every observable property that the design must keep: what
a `return`, `break`, `continue` and `?` inside the block target; what
the block captures and when, with and without `move`; whether a
variable moved into a `move` block is unusable afterwards; order of
effects; error locations. Any difference the design cannot keep is
sent for a ruling before implementation.

## Constraints

- A `return` inside the block must not return from the enclosing
  function if legacy returns from the block. Control flow that legacy
  rejects stays rejected, with a diagnostic no worse than legacy's.
- Interruption, depth bounds, slicing and the absence of any per-block
  runtime, thread or task are as in ADR 0273.
- The stack bytes of ordinary expression and block lowering do not
  change beside the base.
- No inventoried executor item or policy file changes. If one must,
  send that and stop.
- Refusal wording comes from the remedy module; the class of every
  parity row is measured, and like situations get like classes.

## Controls

- The example above; nesting of blocks and async functions; a block
  across a host wait with effects before and after; `?`, `return` and
  loop control inside the block; captures with and without `move`.
- Each refused form refused before any effect with exact text and
  location.
- Interruption inside a block with a real signal; the nesting bound at
  its exact frontier on both stacks.
- The transcripts that pin today's refusal of an async block are
  re-observed; rows change class only by measurement.
- Miri over any new ownership path at small sizes.

## Out of scope

Futures as values, async closures, concurrency, bytes.
