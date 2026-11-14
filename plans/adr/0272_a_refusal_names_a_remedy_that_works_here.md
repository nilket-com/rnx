# ADR 0272: a refusal names a remedy that works where it is shown

Status: Accepted

Base: published 21e38e787baf9e433b55cc80acf344771925ec5f (ADR 0271).

## Problem

When an execution session refuses a form, the message ends with a
remedy: start `rnx repl` without `--engine=execution`. That is correct
at the terminal prompt. The same text is shown in a notebook running
the `rnx-execution` kernel, including after the kernel is restarted,
where there is no command line to change. The working remedy there is
to select the other kernel. A diagnostic may not recommend something
the reader cannot do.

The text is also written out in several places, so the wording can
drift between kinds of refusal.

## Decision

The remedy shown with a refusal is the one that works in the front end
that started the session. The terminal session keeps its present text
exactly. The notebook kernel names the kernel to switch to, by the name
a user sees in the kernel picker. An embedding that states nothing gets
a remedy that names no front end.

The front end is stated once, when the session is created, as a value
of a closed type. No refusal site formats a remedy itself, and no site
decides by inspecting a process name, an environment variable or the
message text.

## Constraints

- Terminal refusal text does not change by one byte. The pinned
  parity table changes only if a row is observed through the notebook.
- The class of every parity row is measured, not assumed.
- No inventoried executor item, policy file or lowering path changes.
  The remedy is chosen where the session reports a refusal, not in the
  engine crate, unless the step-2 note shows why that is impossible.
- A session recreated by a terminal `:dep` restart, a notebook kernel
  restart or a reset keeps the front end of the session it replaces.
  The notebook has no `:dep` today and this record adds none.
- Only refusals that carry the switch-engine remedy today change.
  Messages with their own correct remedy (await the call, `:reset`,
  size limits) stay byte-identical in every front end.
- No diagnostic loses its construct name, location or reason.

## Controls

- Every refusal path that carries the switch-engine remedy
  (unsupported form, host function, compiler refusal) is exercised
  once per front end and shows that front end's remedy and no other.
  The messages with a local remedy are exercised in every front end
  and are identical.
- The notebook control runs through the real kernel and checks that
  the kernel named in the remedy is an installed kernel name, taken
  from the same definition the installer uses.
- A terminal `:dep` restart control, a real notebook kernel restart
  control and a reset control.
- A source control: the remedy strings exist in one place.

## Out of scope

Changing which forms are refused, and the wording of the legacy
session's own diagnostics.
