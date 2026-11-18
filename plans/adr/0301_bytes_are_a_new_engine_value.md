# ADR 0301: Bytes are a new-engine value

Status: Withdrawn

Base: published 13150c86c401fe2f99c30cf1b3f935e21a6278f2 (fix after ADR 0300).

## Problem

The new engine has no Bytes value. `Type` has no Bytes variant, the
store has no byte root, and lowering refuses byte strings (`b"..."`)
with `Refusal::ByteStr` (`lower.rs`). A byte literal (`b'x'`) already
lowers to an unsigned value, and the frozen scalar cases return U64,
255 included. `Bytes::HASH` is in `receiver_domain::UNREPRESENTED`,
which skips native providers during dynamic-method certification and
bars a nominal with that hash. This is milestone A1 of the old-core
removal inventory (2026-11-14, refreshed 2026-11-17).
- In the session parity pins, Bytes accounts for 8 of the rows that keep
  sessions on the old engine: 2 admission refusals (a constant byte
  string and a response byte string) and 6 host refusals.
- The host refusals are the last 4 unenrolled stock host declarations:
  `fs::read_bytes`, `process::run_bytes`, `http::get_bytes` and
  `http::request_bytes`.
- The HTTP envelope of `rnx serve` (B5) is Bytes as well.

Real use is small. The inventory's lexical scan finds no byte names or
byte literals in the demos, notebooks, project READMEs or recorded eval
traffic. A1 is about removal, not demand: the session default (A3) needs
it.

The first attempt, C16g, was parked on 2026-10-19 for cost. A 16th
outer `Payload` variant pushed `Payload::tag`, `index_tag` and
`Store::tag` past inlining: shared rows rose 1.6%, outside the 0.5%
per-record band. Neither of its alternatives fit: a cold-union refactor
shifted `iterator_next`, and there was no opaque host payload to reuse.
The record was left to a measured representation, such as tag dispatch
that does not depend on the variant count.

Its oracle remains, at 491dea52:
`experimental_core/tests/gap_hunt/byte_strings`. It holds 109 frozen
cases with the old engine's observations:

| Family | Cases |
|---|---|
| compare | 30 |
| methods | 21 |
| index | 15 |
| mutation | 14 |
| surface | 13 |
| observation | 10 |
| chain, identity, boundary, effects | 6 |

The frozen stock observations are 73 values, 35 runtime errors and one
compile error (boundary/u8-type), all at 200,000 fuel.

## Decision

1. Bytes is a value of the new engine: a type, a store representation
   with the ownership, sharing and mutation rules the store applies to
   its other mutable sequences, and lowering for byte-string literals.
   Byte literals keep their unsigned route and type identity.
   - Representation, to be confirmed by the boundary review: a dedicated
     heap payload of `alloc::Vec<u8>` behind the existing branded
     handle. It is a leaf with no children for ownership and entry
     admission, and `Value`, `Scalar` and the `Payload`, `Entry` and
     `Slot` sizes are unchanged.
   - Accounting: actual live capacity counts. Allocation and work are
     preflighted with checked arithmetic. Rejected growth and consuming
     conversions are transactional, and consuming operations leave a
     typed moved tombstone.
   - Sharing: alias mutation shares the handle, and slices and clones
     get independent storage.
2. The representation must not regress existing types. Before the full
   surface, a measured first step adds the type and payload and runs
   the fixed-route band. If the band fails, as C16g did, the record
   stops for a ruling between doing variant-count-independent tag
   dispatch first and parking again. No tuning loop runs without a
   ruling.
3. Constants: the session's constant byte string needs const
   evaluation, byte accounting for the existing `ConstNodeKind::Bytes`,
   and fresh materialization at each use. Byte-string patterns and new
   const operations are outside this cut.
4. Methods: adding native Bytes stops skipping its receiver in
   dynamic-method certification, while keeping the nominal and
   enum-parent collision refusal. Each reachable provider in the
   existing table is certified or refused at compile time.
5. The language surface the oracle covers runs on the new engine with
   the old engine's observable semantics: construction, comparison and
   ordering, indexing and its errors, the methods the oracle exercises,
   mutation and aliasing, formatting and debug display, and the
   boundary and budget behavior it records. A difference the oracle
   shows to be a stock defect may be decided deliberately, as an
   intentional difference with its reason, never silently. An admitted
   negative case keeps its stock error (for example concatenation and
   direct iteration) or records the exact reviewed difference.
6. Bytes crosses the existing host boundary in both directions: as a
   host input and output, through dynamic outputs, and through session
   state, import and export, so a session can hold and display it. The
   concrete witness is `web::response`, which already takes an
   `OwnedTreeInput` and has a string-only output certificate. Its
   response-bytes row needs an owned byte leaf at the input and a
   certified Bytes leaf in that response's dynamic output, keeping the
   raw bytes and the type. `web::html` and `web::text` keep their
   separate `StringOrRejected` adapters unless the boundary review says
   otherwise. Generic callers are not broadened. The four byte host
   functions are not enrolled here; each follows as its own cut, one
   family at a time, on this representation.
7. The fixed-route source condition (ADR 0239) stops on any change to an
   inventoried item and has no path for an edit that review has
   approved. The user authorized the protected run edit for this
   record. That authorization is the review ADR 0239 calls for, but
   only for exactly three items:
   - `Executor::fixed_intrinsic`: the certified Bytes method dispatch to
     its cold helper;
   - `Executor::invoke_args`: the fixed `Op::Bytes` arm beside
     `Op::String`;
   - `Executor::host_call`: passing the bound output certificate into
     the cold output conversion.

   This record adds a source-condition transition, bound to the
   declared baseline 13150c86 and to the exact canonical hashes of
   those three items before and after the edit. Nothing else relaxes:
   - all 40 selectors stay;
   - every other item still needs exact equality;
   - the mirror guard stays;
   - the per-record and cumulative bands and the anchor are unchanged.

   Extra source drift, a mismatched hash, or a missing or altered
   transition fails closed. The transition names its baseline, so it
   does not carry over to a later record: the next baseline already
   contains these edits.
8. Anything outside the oracle's families keeps a typed compile-time
   refusal and falls back before effects, as today.

## Constraints

- No change to the behavior or measured cost of existing types. The
  fixed-route band holds.
- The store, value and run paths are protected execution code. They get
  a boundary review before production edits and a source review before
  the final. The boundary review names every executor arm the change
  touches, including the fixed and sliced guard hooks and the
  conversion routes. The user's authorization is required for the
  protected run edit, as when C16g was parked.
- No new crate. Tabs.

## Controls

- The C16g oracle is promoted from parked to enforced, through a
  portable projection tied to the frozen source digests. The projection
  keeps:
  - exact value, type and alias graphs;
  - ordered effects and script stdout;
  - diagnostic class, text and source sites;
  - recovery and reuse.
  It defines precisely what is engine-specific and therefore dropped:
  VM frame debug text and the harness prologue in stdout. Every
  difference is shown and reviewed.
- Size, allocation and work exhaustion have their own controls. The
  oracle's fixed 200,000 fuel supports no claim of instruction-level
  budget parity.
- A byte-literal control, an ordinary byte-string control and a
  repeated-constant mutation control.
- Receiver controls before effects: a forged nominal with the Bytes
  hash, a supported dispatch and an unsupported provider.
- The `web::response` witness: nested byte headers, a repeated dynamic
  node whose snapshot is detached from the stock value, and cleanup
  after a failed admission or import. The existing `web::html` and
  `web::text` negative contracts hold.
- The measured first step's fixed-route band, before the full surface.
- The source-condition transition: these three items at the declared
  hashes pass, while any other item's drift, a changed hash, another
  baseline and a removed transition each fail closed. The integrated
  screen then runs the bands normally and stops again on failure.
- The session parity pins: the 2 Bytes admission rows become Equal or
  reviewed differences. The 6 host rows stay host refusals, unchanged,
  until their own cuts.
- Store controls for the new representation: ownership and aliasing
  under mutation, release accounting with no leaked root, and limits on
  size and work.
- The census and the routing inventory are refreshed.
- A mutation per semantic family and per store rule fails its control.
- One complete final.
