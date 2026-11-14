# Implementation plan 0272: a refusal names a remedy that works where it is shown

Status: Accepted

Paired ADR: ../adr/0272_a_refusal_names_a_remedy_that_works_here.md.

1. Pin 21e38e78. Read every place a session refusal is formatted, how
   the terminal session, the worker behind the notebook kernel and a
   `:dep` restart each create the session, how the kernel names are
   defined for installation, and the tests that pin refusal text.
2. Before editing, send a short note: the list of formatting sites;
   where the front end can be stated once and how it reaches them;
   the proposed notebook and neutral wording; whether any engine-crate
   text carries a remedy; and which pinned texts change.
3. Implement the decision.
4. Add the controls of the ADR.
5. Run a required final (complete if gate tooling changes), preflight
   and the commit checker. Bring the packet with the before and after
   text per front end and the per-stage durations. The candidate is
   frozen once the packet is sent.

## Implementation and controls

DomainSession records the closed FrontEnd value at construction. Its
default is Neutral; the terminal explicitly supplies Terminal, and the
notebook launcher supplies Notebook through the worker's closed startup
option. Unknown and duplicate options refuse before control endpoints
open. No request or source text can replace this choice. Reset retains
the field; a terminal dependency restart re-enters the terminal
constructor, and a kernel restart reuses the notebook launcher.

The session remedy module owns all switch-engine wording and the local
await-call formatter. The three existing switch sites provide their
construct/reason and retain their location handling. The terminal suffix
is exactly ``start `rnx repl` without --engine=execution``; the notebook
suffix is ``select the `Rune (rnx)` kernel``; the neutral suffix is
`use the legacy session engine`. The legacy picker label is defined once
in a small source file shared by the session and the kernel installer.
The installed legacy kernelspec is unchanged. Rune's correct generic
await diagnostic, lowering, executor and policies are unchanged.

The neutral advice names an existing choice: the public worker supports
--engine=legacy. A real worker control evaluates the same async form
successfully with that option after the execution worker refuses it.
The internal default constructor is also tested as Neutral. No new
public embedding constructor is implied by the wording.

Controls exercise unsupported forms, unenrolled hosts and compiler
refusals under every front end, checking construct, reason, origin and
the selected remedy. Local await, declaration-reset and size messages
are identical across front ends. Reset controls retain the choice and
restart input numbering. The real installed-kernel control checks shell
and IOPub errors against the legacy kernelspec's display_name, before
and after a kernel restart. The terminal PTY dependency handover checks
the exact terminal remedy after exec and reset. Source ownership is
checked over production literals, excluding test-only modules/goldens.

Parity builders state Terminal explicitly, and notebook replay states
Notebook. The observed table, including the measured class of every
row, matches the committed table without editing the pin. The three
pinned notebook replays have no switch-bearing refused rows. The raw
worker's old terminal advice becomes the neutral advice when no front
end is supplied; its independent transport control now tests that
default, every explicit front end and the existing legacy option.

Development controls pass for the complete session suite, targeted
terminal/worker tests, production string ownership, root and kernel builds
and unit tests, the installed-kernel restart, terminal dependency
restart, and the canonical root/Jupyter Clippy recipes. The root
Clippy screen adds no diagnostic to its published baseline.
