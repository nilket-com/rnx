# Implementation plan 0275: host functions that take an object or a vector in an execution session

Status: Implemented

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

## Results

- Execution sessions admit `process::run(program, args, options)`,
  `time::from_parts(parts, zone)` and
  `web::response(status, body, content_type, headers)` through three sealed
  mixed-position owned snapshot signatures. The single-input API stays
  separate. No inventoried executor item or fixed-route policy is changed.
- The snapshot contract is narrower than the plan's phrase “anywhere
  inside”: the part read consists of scalars, strings, vectors, tuples,
  objects and Option, down to the existing depth and frontier bounds.
  Result, struct, enum, closure, function and host values, and values beyond
  a bound, are unentered diagnostic leaves. A moved value within the part
  read is an access error at the call, including an ignored field. Hidden
  descendants are not inspected and cannot be obtained by the handler.
- Each handler shares its parser between borrowed stock values and the
  owned packet view. Diagnostic type metadata keeps calendar integer and
  wrong-root errors distinct. The two closed terminal output shapes keep
  runtime errors separate from script Result errors; arbitrary nested
  outputs remain unenrolled.
- Process launch preserves validation, path and environment rules, strict
  UTF-8 capture, limits, timeout and supervision. Packets are released
  before the child starts. Interruption uses the existing structured host
  interruption marker, consumed once and cleared at invocation entry; the
  child is reaped before a terminal interrupt reaches the session. Timeout
  remains an ordinary result. A saved-and-restored invocation scope limits
  this terminal conversion to sessions; ordinary file runs keep their
  published cancelled-result behavior. An isolated Linux control measured 1.93 ms
  from the ready child's interrupt signal to return; this is an observation,
  not a tighter wall-clock guarantee. Late completion and a later unrelated
  runtime error have separate controls. No front-end-specific path is added.
- A response remains an owned object with an integer status, string body
  and object of string-vector headers. It can be indexed and rendered like
  the existing string responses. Bytes remain refused before effects.
- Two packet arguments are individually bounded, not one aggregate budget.
  With a 65,536-byte limit per packet and 48,000 bytes of copied string
  payload in each, the isolated invocation allocator peak was 121,270 bytes
  above baseline. Overflow in the second packet invokes no handler and
  releases the first packet and source temporaries.
- The measured table grows from 296 to 318 rows: Equal 247 to 262,
  Equal by property 3 to 3, Intentional difference 15 to 23,
  Wording 21 to 22, Refused at admission 1 to 2, and
  Refused for a host function 9 to 6. The new intentional differences are
  the exact moved-snapshot pairs; the new wording row retains the calendar
  runtime type-error channel. Seven leaf-boundary observations are Equal.
  Remaining named host refusals concern Bytes.
- Source events and measured rows each occupy one line in the pin, which
  remains within its unchanged 163,840-byte cap.
- Live census hashes for the reviewed time and web sources are refreshed.
  Census cases 0007 and 0072 now reach their object-method refusals after
  `process::run` is admitted. Their admission and compile-only companion
  rows use the existing Method class; all other admission classes stay the
  same, and these two examples are not executed.
