# ADR 0278: object construction and key lookup independent of object size

Status: Accepted

Base: published 64fd789ddcc0aff02dc42e2884d737728947d900 (ADR 0277).

## Problem

An object in the new engine is a vector of key and value pairs. Every
key operation scans it: construction checks each key against every
earlier key, and get, contains_key, insert, remove, field access, index
access and comparison find a key by walking the fields. Building an
object of N keys therefore costs N squared, and each lookup costs N.

Measured after ADR 0277 (optimized build, native code generation off):

| Operation | Fields | Old engine | New engine |
|---|---:|---:|---:|
| `json::parse` of an object | 100,000 | 33.8 ms | 13.7 s |
| insert loop | 100,000 | 84.4 ms | 24.5 s |
| record construction (heap primitive) | 1,000 to 100,000 | 0.056 to 9.29 ms | 1.20 ms to 12.4 s |
| get, last key | 100,000 | 0.10 µs | 251 µs |

The old engine keeps objects in an insertion-ordered map with a hash
index. A program that parses a large JSON document, or builds a large
object in a loop, takes hundreds of times longer on the new engine, and
rnx's stated target is large data.

## Decision

Objects in the new engine find a key in time independent of the number
of fields. Construction of N distinct keys costs time proportional to
N; get, contains_key, insert of a new or present key, field and index
reads and writes cost time independent of N. Remove keeps the old
engine's semantics (the entries behind the removed one move forward,
order is preserved) and may cost N as it does there. Object comparison
costs time proportional to the field count, not its square.

Iteration order, the observable semantics of every Object operation,
ownership, moves, memory charges and the refusals that happen before
effects do not change. Structs are records with declared fields and
are out of scope unless they share the change at no cost.

## Constraints

- Input cannot choose the cost. Keys come from untrusted text through
  `json::parse` and the web handlers. No sequence of keys may push a
  lookup or an insertion to cost proportional to the field count:
  either the hash is keyed with randomness the input cannot observe,
  or the index's worst case is bounded by construction. Iteration
  order never depends on the hash. The old engine's map uses a fixed
  key, so reusing it as is does not meet this constraint.
- The new engine does not come to depend on a module of the old
  engine. If a structure is shared, it moves to a place both use, and
  the old engine's behavior is unchanged.
- The index is charged to the store's byte budget before it is
  allocated. An allocation or budget failure leaves the object and
  every ownership count as they were, with today's error.
- No user-reachable panic and no new unchecked code without a Miri
  control over it.
- Small objects do not get slower: a size below which the index is not
  built is allowed, if the measurement shows the crossover. The
  fixed-route bands pass with no band change and no inventoried item
  changed. If an inventoried item would change, send that and stop.
- The cost bound is enforced by a committed deterministic control,
  not by a timing: a counter of key comparisons or probes, under test
  configuration, shows bounded work per operation at 100,000 keys,
  including for a key set built to collide under any fixed hash.

## Controls

- The existing Object order, key, method and constructor contracts
  pass unchanged, and the parity table is unchanged or changes only by
  observation.
- Each operation (literal, `json::parse`, insert new, insert present,
  get present and absent, contains_key, remove first, middle and last,
  field read and write, index read and write, comparison, clone,
  iteration, display) at 0, 1, the threshold, threshold plus one and
  100,000 keys, beside the old engine where it can observe.
- Remove then insert of the same key appends it, as in the old engine.
- Probe or comparison counts bounded per operation at 100,000 keys,
  for ordinary keys and for a colliding key set.
- Allocation and budget failure at the index allocation and at its
  growth: object, values and ownership unchanged.
- Miri over the index at small sizes, including growth, removal and
  failure.
- The cost table above measured again at 8, 1,000 and 100,000 fields,
  both builds, for the packet.

## Out of scope

The code size limit that refuses a 100,000-field literal at compile
time; Bytes; struct field layout.
