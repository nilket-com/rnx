# ADR 0275: host functions that take an object or a vector in an execution session

Status: Proposed

Base: published 404fd15dd1bbcd2db204dce4060bfe35b5d59323 (ADR 0274).

## Problem

Three core host functions are still refused in an execution session
because an argument is an object or a vector that the handler reads
field by field:

- `process::run(program, args, options)`,
- `time::from_parts(parts, zone)`,
- `web::response(status, body, content_type, headers)`.

ADR 0270 gave `http::request` a bounded owned snapshot of its options
object, read once at the call, and showed that such a function can be
enrolled without changing the executor. The same route should close
these three. `process::run` is the one that appears in the project's
own examples.

## Decision

The three functions are enrolled for execution sessions with their
object and vector arguments passed as the bounded owned snapshot of
ADR 0270, in whatever argument position they occur, and their results
returned through an explicit output certificate.

The snapshot access rule is the one already published: an argument
taken by snapshot is read in full at the call, so a moved value
anywhere inside it is an access error at the call, before the handler
and before any effect, even in a field the handler would ignore. This
settles the question left open for `time::from_parts`. Live ignored
fields keep the handler's own error selection.

Where a function accepts either a string or bytes (a response body,
process input), the string form is admitted and the bytes form is
refused by name as bytes, before effects. `process::run_bytes` and the
other bytes functions stay refused.

Each function shares one parser between the legacy and execution
views, as `http::request` does, so validation and error text cannot
drift. The implementer reports first what each function reads, what it
returns, and any place the shared route does not fit.

## Constraints

- No inventoried executor item or policy file changes. If one must,
  send that and stop.
- `process::run` keeps every protection of the legacy path: no shell,
  the same supervision, limits, timeout and interruption, and the
  child is gone when the call ends by error or interrupt. Interruption
  of a running child is terminal and prompt.
- Tests start only fixed local programs with fixed arguments inside a
  private directory, reach no network, and commit no real environment
  or argument vector of the machine.
- Snapshot size counts against the existing bounds; an oversized
  argument is a named error before effects.
- Results are owned values of existing execution types; no host value
  object is introduced for a response unless the note shows one is
  needed, in which case send that first.
- Parity rows are observed and classified by measurement, and like
  situations get the classes used in ADR 0270.

## Controls

- Each function: success, each documented error, each option, beside
  legacy. Moved values in read and ignored fields. Oversized input.
- `process::run`: exit status, output capture, working directory and
  environment options as the handler defines them, a missing program,
  timeout, a real interrupt of a running child with the child reaped
  and the next input usable.
- `web::response`: status range, header validation, string body, a
  bytes body refused by name; the result usable where legacy uses it.
- `time::from_parts`: valid and invalid parts and zones, round trip
  with `time::parts`.
- Miri over the new input and output ownership paths at small sizes.

## Out of scope

Bytes, new host value types, and any change to legacy behavior.
