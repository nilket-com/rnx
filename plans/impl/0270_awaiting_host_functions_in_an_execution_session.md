# Implementation plan 0270: awaiting a host function in an execution session

Status: Proposed

Paired ADR: ../adr/0270_awaiting_host_functions_in_an_execution_session.md.

1. Pin ee4cf69e. Read how the asynchronous host functions are
   registered and tracked by the session's request scope, how the
   legacy session drives and interrupts them, where the execution
   session refuses await and the five functions, the typed host bridge
   and its enrollment, the interrupt check between work slices, and
   the fixed-route inventory.
2. Before editing, send a note with: what legacy does, observed, for
   each input class of the ADR, including a call that is not awaited
   and an interrupt while pending; the alternatives for waiting (for
   example the host call waits on the session's runtime with
   cancellation, or the invocation suspends and the session resumes
   it) with what each costs a program that awaits nothing; the one
   proposed; how the form is recognized at compile time and what each
   refused neighbor reports; how interrupt reaches a pending call; the
   fate of `http::request`; and every inventoried item touched, by
   name. If any is touched, send the note and stop.
3. Implement the design.
4. Add the controls of the ADR and regenerate the pinned table. State
   the class of each new or changed row and the totals before and
   after. Run the paired fixed-route screen before the full gates.
5. Run Miri on a committed inventory with no case near the time limit,
   a required final (complete if gate tooling changes), preflight and
   the commit checker. Bring the packet with the interrupt delay, the
   fixed-route result, the three slowest Miri cases and the per-stage
   durations. The candidate is frozen once the packet is sent.

Results are added here by the implementer.
