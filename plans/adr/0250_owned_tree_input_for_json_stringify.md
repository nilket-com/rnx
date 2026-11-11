# ADR 0250: owned tree input for json::stringify

Status: Proposed

Base: published 9104d9c385cdb556456d6fa31ed3dfb79b2ebf42 (ADR 0249).

## Problem

json::stringify is refused in an execution session. It takes a value tree:
scalars, strings, vectors, tuples, objects and Option, nested. The bridge
gives a host function only inline values and strings. For any other
argument it supplies a manufactured witness with no contents (an empty
vector or object, a one-element tuple, None), which ADR 0249 could use
because a string-or-rejected argument reads only the type. A tree argument
that read the witness would serialize fabricated data.

A session or notebook user reaches for JSON output early. process::run has
the same need for its arguments but a different validation order and a
process effect; it is not part of this record.

## Goal

json::stringify is callable from an execution session and returns what
legacy returns for the same value: the same text on success and the same
Err text, through the Result channel, on refusal.

## Required behavior

- The host receives a bounded, fully owned copy of the tree, made before
  the host body runs. No store handle, borrow or pointer crosses the call,
  and the copy never reaches a host body partially built.
- The witness is never read as the argument's contents. A typed argument
  of every existing kind keeps its current classification and witness;
  negative controls show that no existing kind reaches the export.
- The export preserves what legacy's walk can observe: container kind
  (tuple is not vector, None is not unit), signedness, string bytes and
  object order. A repeated alias outside the active path is copied again,
  as legacy serializes it again. Aliasing is not preserved across the call.
- Unsupported values keep a descriptor sufficient for legacy's exact
  message (Result without looking inside, function without calling it,
  nominal kind and name without descending), and the path to them is
  legacy's path, including unusual keys.
- Depth and cycles follow legacy: root counts as depth one, the limit is
  legacy's, a container at the limit reports at the container's path, only
  active vector, tuple and object identities count as a cycle, and an
  Option-only chain ends in the depth error. The export's own physical
  limit never fires before legacy's semantic one.
- One validation walk with one set of messages serves both engines, over a
  view that legacy values and the owned copy both implement. Legacy's
  registration and behavior are unchanged and its existing tests pass
  unmodified.
- Resource bound. The copy is bounded by owned bytes with checked
  arithmetic and fallible allocation, counting each expanded occurrence of
  an alias. The bound derives from a limit the session already has (store
  limits or the session ceiling), so that a value legacy serializes is
  refused only for exceeding a resource limit, with a refusal that names
  it. A separate small node cap is not added unless the boundary review
  shows the byte bound is insufficient. Every temporary, including any
  second materialization for the serializer, is counted and released on
  return, error and panic.
- A failed copy, access failure or allocation failure is an error of its
  own; it is never turned into a type rejection or a unit.
- The enrollment API is sealed to one owned-tree argument in final
  position. It is not a general dynamic input, and the input marker cannot
  be declared as an output.

## Fixed route

No inventoried item is edited and the policy file is unchanged. The
alternate handler and ExecutionRuntime identity of ADR 0249 are reused.

The route decides in the uninventoried input helper that a marked argument
is exported rather than witnessed. Where the type system can carry that
decision to the snapshot helper without an inventoried edit, it does; where
it cannot, the implementer names the place and the record relies on the
reachability controls above. The bands pass per record and cumulatively.

## Out of scope

- process::run, process::run_bytes, the web response constructors,
  time::from_parts, web::escape_html, bytes, async.
- Changes to JSON formatting or to legacy's limits.

## Acceptance

- session_parity.json: json::stringify leaves "Refused for a host
  function" and becomes Equal with executed witnesses; host refusals fall
  from 14 to 13. No other row changes class.
- Differential controls, each Equal: scalars; strings from a local, a
  branch and an earlier closure; vector, tuple, object, Option, nested;
  repeated aliases; a cycle through each container kind; the depth
  boundary and an Option-only chain; each unsupported kind nested under a
  key, an unusual key and an index; object order; a moved or inaccessible
  value.
- A value over the resource bound is refused by name with nothing passed
  to the host, and the session stays usable.
- Miri covers the ownership boundary, including cleanup on error and
  panic.
- Source condition and bands pass. Required stages follow the changed
  paths.
