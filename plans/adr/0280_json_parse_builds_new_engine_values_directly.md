# ADR 0280: json::parse builds new-engine values directly

Status: Accepted

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
