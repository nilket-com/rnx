# Implementation plan 0275: host functions that take an object or a vector in an execution session

Status: Proposed

Paired ADR: ../adr/0275_host_functions_with_object_and_vector_inputs.md.

1. Pin 404fd15d. Read how `http::request` is enrolled with an owned
   tree input and an output certificate, the sealed input signatures,
   the three handlers and what each reads and returns, the process
   supervisor and its interruption path, and the fixed-route
   inventory.
2. Before editing, send a note with: per function, the argument
   shapes, every field read, the result shape and how it is
   certified; how the sealed signatures extend to these positions;
   how `process::run` is interrupted inside a synchronous host call
   and what bounds its duration; the measured legacy table for moved
   and ignored fields; what a response is as an execution value; any
   observable difference; and every inventoried item touched, by
   name. If any is touched, send the note and stop. If one of the
   three does not fit, say which and propose the cut.
3. Implement the design.
4. Add the controls of the ADR and regenerate the pinned table.
5. Run the fixed-route screen first, then Miri, a required final
   (complete if gate tooling changes), preflight and the commit
   checker. Bring the packet with the class of each changed row, the
   totals before and after and the per-stage durations. Results state
   what the code does; no paths under target/ and no development
   history. The candidate is frozen once the packet is sent.

Results are added here by the implementer.
