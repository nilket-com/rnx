# ADR 0242: REPL sessions on the program domain

Status: Accepted

Base: published 61116a2d9fd6a7770d00087a9d639ff2f8c779e4 (ADR 0241).

## Problem

A persistent session always executes on the legacy engine. Session::eval
passes no engine options, requires a legacy Value and runs a Vm; only the
one-shot eval path (ADR 0237) can select the new engine. ADR 0238 removed the
blocker: programs that share one Store can call each other's function values,
and ProgramDomain owns program lifetime. The routing inventory already runs the
session witness through a ProgramDomain and gets 0, 7, 2:

	let n=7;let c=||n;let shared=[1];0
	shared.push(2);c()
	shared.len()

That witness is three generated sources, not a session. Nothing yet keeps
bindings, declarations, budgets, diagnostics and reset behavior across inputs
on a domain, and no user can reach it.

## Goal

A synchronous interactive REPL session runs on a session-owned ProgramDomain,
with the same observable behavior as the legacy session for every input the
new engine admits. This is the first user-reachable persistent consumer of the
new engine and the base for the Jupyter worker and :dep in later records.

## Engine policy: sticky per session

A live domain holds closures and aliased values that cannot be moved to a
legacy Vm, and an input's effects cannot be replayed. So the engine is chosen
once per session and never changes while the session holds state:

- A session on the domain that receives an input the new engine refuses
  (an async input, or a form the frontend does not admit) reports the refusal
  as a diagnostic before any effect. Bindings, declarations and live values
  are unchanged. Input text, history, numbering and diagnostic bookkeeping
  advance as they do for any input, as on legacy. Refusals stay compile-time,
  as everywhere else.
- No silent migration, no replay, no per-input fallback of a populated session.
- :reset (and any equivalent) returns the session to its initial engine
  choice with an empty domain.

Default route is unchanged in this record. Async inputs are ordinary in
interactive use (HTTP), the domain cannot run them yet, and a sticky session
that refuses them would be a regression. The domain session is therefore
opt-in through the existing engine option vocabulary; the implementer proposes
the spelling in the boundary note, consistent with --engine and without
adding a second name for the same engine. Making it the default is a later
record, after async input runs on the domain.

## Required behavior

For admitted synchronous input, equal to the legacy session, shown by running
the same transcripts through both:

- Bindings persist with alias identity: a value bound by one input and
  mutated through another binding or closure is the same object later.
- Closures bound by one input are callable from later inputs.
- Declarations (fn, const, struct, enum) persist and may be redefined under
  the rules the legacy session applies today, including its restrictions on
  type redefinition. Declaration forms the session refuses today (impl,
  imports, modules, macros) stay refused. No session syntax is added.
- Publication is transactional for the binding delta and declarations of an
  input; a failed input publishes none. Mutations already made through shared
  objects before the failure remain, as on legacy. An untouched binding is
  never lost because an input, successful or failed, returned a partial delta.
- Diagnostics have the same error category and the same original input,
  span and caller origin as legacy wherever legacy reports them, including
  after :renumber. Budget, interruption and refusal messages keep their
  user-facing text. Any other engine-specific wording is listed per row in the
  transcript corpus and reviewed before results.
- Budget exhaustion and interrupt (Ctrl-C) end the input and leave later
  evaluation usable. A latched memory ceiling refuses evaluation until reset,
  as today, and may relatch at once; read-only commands stay usable at the
  ceiling.
- Plain-text presentation of results, and :vars, :help, :debug and :memory
  as today.
- Reset drops bindings, declarations, programs and history, cancels running
  work and keeps the installed context, runtime and presenters. It does not
  promise a process below the memory ceiling.

## Lifetime

- One ProgramDomain per session epoch. Programs are compiled into it per
  input and released explicitly when their entry use ends; persistent values
  are held through DomainRoot only. Raw Store roots stay sealed.
- Collection runs at idle input boundaries (after success, failure and
  cancellation) and at reset, after any borrowed failure diagnostics have been
  materialized. A collection failure is reported and never loses a live
  binding or replays an input.
- Executable programs, live Store objects and roots are bounded by what is
  reachable from the session's bindings and declarations, plus explicit active
  or diagnostic leases. No unreachable program survives collection. A closure
  may keep several programs alive transitively; that is reachability, not a
  leak. Reusable capacity is bounded. Show each with a counted control taken
  after output and temporary handles are gone, not with an argument.
- Input text and source history grow by the existing policy until reset. They
  are measured and reported separately from the counts above.
- Nothing is retained in process-wide pools or statics.

## Out of scope

- Async input on the domain, and making the domain session the default.
- Jupyter worker, notebook transport, native presenters, :dep and context
  replacement. Where the REPL offers one of these today, the domain session
  refuses it explicitly with a diagnostic; list each such operation.
- Frontend expansion. The known refusal of map(f) for a closure binding of
  unknown arity is disclosed and covered by a test of the refusal, not fixed.
- Changes to the fixed operation loop or its inventory.

## Acceptance

- Differential transcripts: a committed corpus of session transcripts runs on
  both engines with equal outputs and diagnostics under the definition above,
  except rows listed as deliberate divergences with a reason. It includes the
  0, 7, 2 witness, redefinition, failure after a shared mutation, an untouched
  binding across a failed input and across a successful partial delta, an
  error through an old closure, a closure saved through a shared mutation by
  a failed input, :renumber followed by an old-closure error, budget
  exhaustion, interrupt, the memory ceiling, and reset.
- Policy rows are classified separately from equality rows: an input legacy
  runs and the domain refuses (async, unsupported forms, each refused REPL
  operation) is a test of the refusal and of unchanged state, not an
  equal-output row. An async input into a populated domain session leaves
  every binding observable and equal afterwards.
- Lifetime controls: program, object and root counts return to a fixed level
  across repeated non-binding inputs and across repeated refusal and
  cancellation with collection; releasing the last alias of a closure and
  dropping entry leases release the programs only they kept alive; reset
  returns the counts to the initial level.
- Legacy session behavior and the default route are unchanged: existing
  session tests pass unmodified.
- Required stages follow the changed paths. The fixed-route gate runs if the
  path table selects it; the source condition must pass without an inventory
  change.
- Miri covers the new ownership boundary code under the single runner.
