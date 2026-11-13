# Implementation plan 0269: admitting a session entry does not scan its arguments' data

Status: Proposed

Paired ADR: ../adr/0269_entry_admission_cost_independent_of_argument_size.md.

1. Pin b251e1b1. Read the formatting certificate and what it permits,
   the admission walk and its caller in the executor, both places the
   domain prepares an entry, how a session passes bindings as
   arguments, every producer of struct values in the store, and the
   fixed-route inventory.
2. Before editing, send a note with: what the check protects; a phase
   timing that splits the two preparations for the measured inputs;
   whether the second preparation is redundant and why; the
   alternatives considered with their cost on ordinary heap operations
   (for example a fact kept per store, a fact kept per value, or a
   proof from how the values were produced); the one proposed; the
   list of ways a struct becomes reachable that the ADR asks for; and
   every inventoried item touched, by name. If any is touched, send
   the note and stop.
3. Implement the design.
4. Add the controls and measurements of the ADR. Large fixtures run
   natively; Miri covers the ownership paths at small sizes, with no
   single case near the time limit.
5. Run Miri, a required final (complete if gate tooling changes),
   preflight and the commit checker. Bring the packet with the
   measurements before and after, the fixed-route result, the three
   slowest Miri cases and the per-stage durations. The candidate is
   frozen once the packet is sent.

Results are added here by the implementer.
