# ADR 0243: :dep restart keeps the session engine

Status: Proposed

Base: published fa46bd029c3f460f9356387c08037a2c52eae33e (ADR 0242).

## Problem

:dep and :depv prepare a dependency outside the session and then replace the
process (src/dep_transition.rs). The replacement is started as plain `repl`
with only the color and splash flags, so it is always a legacy session. ADR
0242 therefore refuses both commands in an execution session. A user who
opted into the new engine cannot add a dependency without leaving it.

## Goal

:dep and :depv work in an execution session, and the replacement process runs
the same engine as the session that issued the command. This holds for the
scratch session and for a project session.

## Required behavior

- The engine choice is carried as a typed value from the running session
  through preparation, the transition protocol and the replacement arguments.
  It is validated at each boundary that parses it. A replacement never starts
  on a different engine than the one that issued the command, and never
  silently falls back to legacy.
- Engine arguments follow `repl`, separate from global flags, in the executed
  command and in every command shown or logged for reopening.
- Until the commit point the old session is intact: a preparation failure, a
  declined prompt or a cancellation leaves every binding, alias and
  declaration observable and equal, on both engines.
- At the commit point the old session is closed exactly once. The existing
  semantics for failures after that point (cleanup, recheck, exec) stay as
  they are and stay explicit; they are the same on both engines.
- The replacement starts with an empty session and a rebuilt context. :reset
  in the replacement keeps the engine.
- An already-installed dependency behaves as it does today on legacy.
- Legacy behavior, output and protocol are unchanged apart from the new field.

## Limits stated to the user

Installing a native adapter in an execution session does not make its native
values admitted there. Their constructors are still refused before effects,
as in ADR 0242. The README says so next to the :dep description.

## Out of scope

- Live context replacement and any conversion of existing values.
- Native-value admission and presenters on the domain.
- The Jupyter worker, async input on the domain, the default route.
- Session syntax, the fixed operation loop and its inventory.

## Acceptance

- PTY transition tests run on both engines: success with state loss and the
  same engine afterwards, already installed, preparation failure, declined
  prompt and cancellation with aliases intact, and the existing cleanup,
  recheck and exec failure cases.
- Scratch and project reopen commands carry the engine; a test reads the
  executed arguments, not only the displayed text.
- Negative control: a transition request naming an unknown or mismatched
  engine is rejected by the protocol check.
- The 0242 refusal test for :dep is replaced by these tests; no other 0242
  transcript row changes.
- Required stages follow the changed paths.
