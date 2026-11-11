# ADR 0244: opt-in Jupyter worker on the program domain

Status: Proposed

Base: published 76e376a3ec2a49b2233e32186b4d71f8e7f527f9 (ADR 0243).

## Problem

The Jupyter worker (src/worker.rs) constructs a legacy Session directly and
depends on two legacy-only things: Session::eval_with_armed, and a legacy
Value that it renders and presents itself. The domain session of ADR 0242 is
reachable only from the terminal REPL, so no notebook can run on the new
engine.

## Goal

A notebook kernel can be installed or started so that its worker runs a
synchronous domain session. The default kernel is unchanged. Worker protocol
1 is unchanged for the default kernel, and the opt-in kernel differs only
where this record says so.

## Required behavior

- Engine choice is sticky per worker process and survives reset and kernel
  restart. It is a typed value validated where the worker and the kernel
  parse their arguments. Unknown spellings are errors.
- The worker drives one engine-neutral session operation that returns owned,
  bounded text, a structured Failure and any secondary cleanup notes. No
  legacy Value and no DomainRoot leaves the session through it.
- Arming: after interrupt clear, admission and input numbering, and before
  any script instruction runs, the worker sends `armed`. If that send fails
  the input has no effect. This holds on both engines; show it on the domain
  with a control that fails the transport at that point and then observes
  unchanged state.
- An input the domain refuses (async, unadmitted form, native value) is
  refused before `armed` effects and before any script effect, with the same
  diagnostics as the REPL of ADR 0242, and settles as an ordinary failure.
  The session stays usable.
- Ordering, stdout/stderr barriers, nonce/id/epoch association,
  acknowledgement, retirement and state_lost, ceiling sampling, reset and
  shutdown behave as on the legacy worker.
- Results are plain text. text_html is null on the domain route. Cleanup
  notes are written before the stderr marker of the input they belong to.
- Interrupt: before `armed`, and during a pure loop after `armed`, both leave
  the session usable.
- The ready message states the engine and that HTML and async are
  unavailable, so a front end or a test can tell which worker it has.

## Out of scope

- Native presenters and HTML on the domain, async input on the domain, and
  making the domain worker the default kernel.
- Protocol version change. If the ready addition cannot be made without
  breaking a protocol-1 reader, stop and bring that to review.
- The fixed operation loop and its inventory.

## Acceptance

- One worker test table is driven through both engines over the real
  transport: results, failures with origin, stream association, reset with
  an old closure origin, ceiling, shutdown. Rows are Equal, Wording or
  Policy as in ADR 0242; no row passes without comparing.
- Controls on the domain worker: interrupt before and after `armed`, lost
  transport at `armed`, refused input before effects.
- Kernel level: an opt-in kernel started through the installed kernelspec
  executes a cell, restarts, and is still on the domain; the default
  kernelspec is byte-identical to today's.
- Existing worker and kernel tests pass unmodified.
- The routing inventory row for the worker is updated, not duplicated.
- Required stages follow the changed paths.
