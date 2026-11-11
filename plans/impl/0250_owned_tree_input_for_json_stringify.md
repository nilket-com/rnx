# Implementation plan 0250: owned tree input for json::stringify

Status: Proposed

Paired ADR: ../adr/0250_owned_tree_input_for_json_stringify.md.

1. Pin 9104d9c3. Read the bridge's input and snapshot helpers and the
   witness path, the 0249 alternate-handler and ExecutionRuntime code, the
   walk and serializer in src/json.rs with their tests, and the store and
   session limits.
2. Before editing, send: the representation of the input marker and why it
   cannot be declared as an output; whether the export decision travels by
   type or by reachability, and where; the byte bound and the existing
   limit it derives from, with the size of the largest value legacy
   serializes that the bound would refuse; and the shape of the shared
   view. Wait for the ruling.
3. Implement the export and the shared walk. Keep JSON policy out of the
   engine's exporter.
4. Add the differential controls of the ADR, the negative controls for
   every existing argument kind, the resource refusal, and the Miri tests.
   Regenerate the pinned table.
5. Send the working tree for source review before signing. Then run the
   required stages, preflight and the commit checker, and bring the packet
   with the before and after class totals. The candidate is frozen once
   the packet is sent.

Results are added here by the implementer.
