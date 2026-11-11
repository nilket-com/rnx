# Implementation plan 0249: string input for host functions with a dynamic parameter

Status: Proposed

Paired ADR: ../adr/0249_string_input_for_dynamic_host_calls.md.

1. Pin d783768c. Read the host bridge, the witness path for unsupported
   arguments in host_call, the enrollment API and certificates in
   src/host.rs, the legacy bodies of the four functions, and the fixed-route
   inventory in tools/column/fixed_route_policy.json.
2. Before editing, send the route: how a string in the dynamic position
   reaches the host as owned text and how every other value yields the
   legacy Err, with no edit to an inventoried item; what the witness can
   observe on that route; and the binding identity of any alternate
   handler. If no such route exists, send that instead and stop.
3. Implement the input contract and enroll the four. Keep the legacy Err
   text in one place used by both engines.
4. Add the executed witnesses and controls of the ADR, the test that bytes
   are not representable, and the Miri test. Give the six remaining
   functions their specific reasons and regenerate the pinned table.
5. Run the required stages, preflight and the commit checker. Bring the
   packet with the before and after class totals. The candidate is frozen
   once the packet is sent.

Results are added here by the implementer.
