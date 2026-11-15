# Implementation plan 0278: object construction and key lookup independent of object size

Status: Implemented

Paired ADR: ../adr/0278_object_construction_and_lookup_by_key.md.

1. Pin 64fd789d. Read the record payload and every operation that
   finds a key in it (construction, owned input and JSON import, field,
   index, methods, comparison, cloning, display, roots and views), the
   old engine's insertion-ordered map, the store's byte budget and
   rollback, and the fixed-route inventory.
2. Before editing, send a note: the index design and its hash or
   worst-case bound, where it lives and what of the old engine it
   reuses, the threshold below which no index is built and the
   measurement that sets it, the payload size before and after, every
   operation's new cost, what remove does, and every inventoried item
   touched, by name. If any is touched, send the note and stop.
3. Implement the design.
4. Add the controls of the ADR and regenerate any pinned table.
5. Run both format checks, every lint recipe and the full root suite
   by hand; then the fixed-route screen, Miri, a required final,
   preflight and the commit checker. Bring the packet with the cost
   table measured again, the class of each changed row, totals before
   and after and the per-stage durations. Results state what the code
   does; no paths under target/ and no development history. The
   candidate is frozen once the packet is sent.

## Results

Objects keep their insertion-ordered field vector and add a boxed
critical-bit index from 32 fields. Below 32, key lookup scans the
bounded small vector and no index is allocated. The optimized
prototype selected 32 as the first measured size, among 16, 24, 32,
48 and 64, where index construction and present and absent lookup
all cost no more than scanning. At 24 fields, construction still
cost 697 ns against 619 ns; at 32 it cost 1,010 ns against 1,116 ns,
and present/absent lookup cost 6.2/7.4 ns against 78.4/81.2 ns.
These prototype numbers select the threshold, not engine timing.

The index encodes key bytes and a terminator without hashing. Branch
positions strictly increase, bounding lookup by key length instead
of the field count. It uses 32 bytes per internal branch, N-1
branches, plus a 40-byte header on 64-bit targets. Spare capacity is
charged in full. Payload, Entry and Slot remain 40, 72 and 80 bytes.
Constructors and geometric growth check the staged byte budget
before allocating. Child ownership changes only after preparation;
failures leave fields, order, index and strong counts unchanged.
An invalid private constructor input containing duplicate names may
encounter a capacity or allocation error before InvalidCode.

Get, contains_key, field/index access and insert use the same index.
Total key capacity is derived from the admitted charge rather than
rescanned on each insertion. Geometric growth has linear cumulative
copying work. Object equality looks up the other object's keys;
clone, iteration and views retain the ordered fields. Debug output
keeps its existing key sorting. Remove shifts the following entries,
fixes the index in one linear pass, preserves order and costs O(N).
Reinsertion appends. The index is released below 32 fields. Declared
Struct field layout and its native snapshot route stay unchanged.

The JSON reader still first constructs a legacy fixed-hash Object
and imports it into the new Store. This record removes the new
Store's quadratic cost; it does not remove that intermediate legacy
allocation, its construction cost or its fixed-hash exposure. Direct
JSON import is a named follow-up.

Committed controls cover ordinary and fixed-fingerprint-colliding
100,000-key sets, short absent queries against long-prefix trees,
embedded NUL and UTF-8, linear growth work, a failing +1 work-bound
control, order-preserving removal, promotion/demotion, index undo,
and allocation/budget rollback. The Miri inventory names the small
insertion, removal and failure controls; large fixtures run natively.

Struct field reads and writes keep their direct scan; Objects take a
separate cold path. The fixed-route measurements put awfy_permute
at -0.12% with native code off and the vector-iteration workloads
at about -6.9% against the base, within the unchanged bands.
