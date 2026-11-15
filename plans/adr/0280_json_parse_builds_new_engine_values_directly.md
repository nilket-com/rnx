# ADR 0280: json::parse builds new-engine values directly

Status: Proposed

Base: published 23b0a6fea6c6f9f677506d70f3fc7eb00da30896 (ADR 0279).

## Problem

`json::parse` reads the text into a serde_json tree, converts that tree
into old-engine values (an Object is the old engine's map with a fixed
hash key), and a script on the new engine then imports those values
into its store. A large document therefore exists three times during
the call, and two of the copies are thrown away. Keys pass through the
old engine's fixed-key hash map, so input can still choose the cost of
building that intermediate, which ADR 0278 removed from the new store
only.

## Decision

On the new engine, `json::parse` builds the result in the store
directly from the parsed document, without an old-engine value in
between. Objects are built through the store's constructors, so they
get ADR 0278's index and charges. The old engine's `json::parse` is
unchanged.

The result is the same as today's for every input: values, numeric
representation (i64, then u64, then f64), key order, the value kept
for a duplicate key, the nesting limit and its diagnostic, and every
parse and conversion error text.

## Constraints

- Input cannot choose the cost: no fixed-key hash on the path.
- Every allocation is charged to the store's byte budget before it is
  made. A budget or allocation failure part way leaves the store with
  no partial value and its counts as they were, with a catchable error.
- The parsed tree is consumed as it is converted; no second complete
  copy is held.
- The new engine does not come to depend on an old-engine module.
- No user-reachable panic.
- If an inventoried executor item would change, send that and stop.
  The fixed-route bands pass unchanged.

## Controls

- The same documents through both engines give the same result and
  the same error text: scalars, the three number forms and their
  edges, empty and nested arrays and objects, duplicate keys, the
  nesting limit and one past it, invalid text, invalid UTF-8 escapes.
- Budget and allocation failure at the first, a middle and the last
  allocation: no partial value, counts unchanged.
- A committed control showing no old-engine value is built on the new
  engine's path.
- `json::parse` of 1,000 and 100,000-field objects and of a 100,000
  element array, both builds, against the published base, in the
  packet.

## Out of scope

`json::stringify`; the HTTP and process helpers that build their
replies through `json::parse` keep doing so.
