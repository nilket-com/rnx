# Implementation plan 0280: json::parse builds new-engine values directly

Status: Implemented

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

## Results

The execution JSON reader consumes the existing serde parser result directly
into Store values. A single opaque ABI carrier is consumed once at the cold
host-output boundary; JSON nodes do not pass through legacy Object, Vec,
String, Result or per-node Value representations. The original legacy
handler and its conversion remain installed for the legacy runtime.

Numbers keep the signed, unsigned, then floating-point decision order,
including negative zero. Objects keep the parsed map's consuming order and
last duplicate value. Parsing and recursion-limit diagnostics use the same
parser and text in both runtimes. Store capacity and allocation errors stay
in the existing execution/API error channel; parser errors remain script
Err(String).

The builder uses a checked traversal stack bounded to 127 containers and a
local ownership ledger. Converter buffers, keys, strings, index, traversal
and ledger storage check the remaining Store byte budget before allocation.
Objects use the existing crit-bit payload constructor without a fixed-key
hash or an unbounded duplicate scan. Failed construction releases every new
value and preserves preexisting aliases, live entries and payload charges.
Successful construction transfers the final root once. Incorrect or reused
carriers produce named host-contract errors, and a producer panic does not
retry the host call.

The parser's own allocations and the constant-size ABI carrier are host
scratch, not charged to the Store byte budget. Failed conversions may leave
reusable Store bookkeeping capacity, with no live value or payload charge.
The HTTP/process JSON helpers and json::stringify retain their routes. No
inventoried executor item changes.

Arrays adopt the checked child buffer directly, with its actual capacity
charged as payload; there is no second list allocation or copy. Active
container frames remain in the traversal stack while scalar children
complete in the loop.

The legacy route keeps its algorithm and code. Median time per call,
published base against this change, in two separately built pairs: on
the new engine, a 100,000-field object 48.8 to 31.2 and 47.9 to 30.8 ms,
and a 100,000-element array 7.2 to 5.0 and 7.3 to 5.2 ms. On the old
engine, a 100,000-field object 25.4 to 25.3 ms in one pair and 24.9 to
29.8 ms in the other, and a 100,000-element array 3.6 to 3.6 and 3.7 to
3.5 ms. The 1,000-field rows on both engines are within the spread of
single-process calls.
