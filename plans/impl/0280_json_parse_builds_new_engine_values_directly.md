# Implementation plan 0280: json::parse builds new-engine values directly

Status: Proposed

Paired ADR: ../adr/0280_json_parse_builds_new_engine_values_directly.md.

1. Pin 23b0a6fe. Read `src/json.rs`, how a host function's old-engine
   return value reaches the new store, the store's Object, List and
   String constructors with their charges and rollback, and the
   fixed-route inventory.
2. Before editing, send a note: where the direct route lives and how
   a host function returns a store value without an old-engine value,
   how key order and duplicate keys stay as today, how a failure part
   way rolls back, and every inventoried item touched, by name. If any
   is touched, send the note and stop.
3. Implement the design and add the controls of the ADR.
4. Run both format checks, every lint recipe, the full root suite and
   the full Rune suite by hand; then the fixed-route screen, Miri if
   unchecked code is added, a complete final, preflight and the commit
   checker. Bring the packet with the cost table and the per-stage
   durations. Results state what the code does; no paths under target/
   and no development history. The candidate is frozen once the
   packet is sent.
