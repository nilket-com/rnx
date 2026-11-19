# ADR 0310: A retained frame converts to a dense matrix

Status: Proposed

Base: published a2139a51d457795ff8ff143ba5f71a9c6ddc7aff (ADR 0309).

## Problem

An execution session can load a CSV and keep the frame (0306, 0307),
and read its width and height (0305, 0308). The next step of a model
workflow converts the frame to the numeric matrix Candle trains on:
`df.to_dense(names, dtype)?`, followed by the matrix's `shape()`,
`names()` and `dtype()`. `to_dense` still takes the old route, so no
script can produce a `Dense` on the new engine. That also blocks Candle
`Tensor::from_dense` (the next record).

`to_dense` is hand-written (`adapters/polars/src/dense.rs`, registered in
`lib.rs`):
- Its signature is `(&DataFrame, names, &str) -> Result<Dense, String>`.
- It validates the names (a vector of 1..4096 strings of 1..256 bytes,
  without duplicates), the dtype (`f32` or `f64`), the shape (nonempty,
  at most 2^24 cells), the columns and their nulls.
- It fills one zeroed row-major buffer and checks the invariants again
  in `Dense::new`.
- Range and non-finite errors arise during the fill, after the buffer is
  allocated.

`Dense` is already a registered native type, and `shape()`, `names()`
and `dtype()` are already checked reads.

Charging the copy before it happens takes more than the element bytes:
- An integer column is cast to a 64-bit temporary.
- The first-null search can allocate a validity bitmap.
- Pinned Polars 0.55.2 can memoize a whole scalar column on the source
  frame. That growth outlives the call, on a frame already charged.
- The argument snapshots (names, dtype) must be bounded before they are
  copied. Exporting an arbitrary tree would first copy an oversized
  rejected name or list.

ADR 0307's credit travels only on an owned tree argument. A shared
receiver has no carrier, and the cold proof admits a shared receiver
only with no other arguments.

## Decision

1. `DataFrame::to_dense(names, dtype)` on a retained checked frame is a
   checked producer of a uniquely owned `Dense`. The method is sealed by
   exact arity (receiver plus two plain arguments), the input
   `DataFrame` and the distinct output `Dense`. Each is verified by
   installed-context hash and Rust `TypeId`, across modules.
   - The output path is 0306's: a `String` error stays an ordinary
     `Err`, `OpaqueProducerLimit` is `StoreCapacity`, with unique
     adoption and unwind.
   - The cold proof is extended for this typed method only: two plain
     arguments, and a direct `?` that yields exactly `Dense`.
2. The carrier leaves every inventoried item, `Executor::host_call`
   included, at its published source. A first carrier inside
   `host_call` failed the early screen: `shared_rows` native-on rose
   +0.87% (W) and +0.80% (T). A split build pinned the cause on the
   `host_call` body edit, which outlined two `BVec<u32>::push` sites in
   native compilation, accounting for the increase from 86 calls to 169. The
   carrier instead uses the session's existing host boundary.
   - Inside `Executor::budgeted_sliced_intrinsic`, within its existing
     budget scope, the exact registered three-argument shared producer
     alone is handed to a cold helper. Every other call is unchanged.
     This function is not inventoried, and `executor_identity` excludes
     it by name; the change is disclosed and screened like any other.
   - The helper authenticates the active handler by its code binding
     and allocation identity, the type pair and the slots. It fails
     closed on a stale or replaced handler, an unregistered producer, a
     foreign receiver, a nominal or Rust type mismatch, wrong arity, or
     invalid slots. `HostBinding`, the Store and the frame gain no
     fields, and nothing dispatches by spelling.
   - It retains the receiver's existing native handle with one
     reference-count increment. It never clones the Polars frame, whose
     `Vec<Column>` can copy.
   - It projects the names straight from the live Store, shallow and
     bounded: root kind and count, then lengths and types, then the
     first duplicate, then the dtype. Nothing oversized or rejected is
     copied. A bad dtype's escaped error text is counted against the
     cap before it is allocated.
   - It builds a private single-use request, an ordinary Rust local, and
     reserves its scratch first. The request holds the retained native,
     the owned bounded names, the validated dtype or a bounded error
     descriptor, and the callback identity. It holds no Store handle,
     borrowed guard, slot pointer, global or script-visible ticket, and
     no reference across a suspension.
   - The cold output helper consumes it immediately, in the same budget
     and panic boundary. It re-reads `host_remaining_bytes()`, deducts
     the live request scratch, and checks the documented conversion
     peak and the output wrappers before the adapter copies anything.
     It then adopts the result through 0306's machinery.
   - The first carrier's `host_call` transition and its fixed-route
     transition machinery are withdrawn.
   - The same carrier is meant to serve a sole `&Dense` in the next
     record.
3. The adapter charges a documented peak before any allocation that
   scales with the input. The peak covers:
   - the output buffer;
   - the largest simultaneous temporary;
   - metadata;
   - the result wrapper;
   - the `Dense` logical charge.
   The conversion reads backing values and validity without
   materializing, so the source frame does not grow. If that cannot be
   kept, the peak covers the growth that persists after the call.
   - Arithmetic is checked.
   - Store admission and release stay authoritative, and the payload is
     charged once.
   - A miss is `StoreCapacity` at the call, with the frame unchanged.
   - Ordinary bad-input errors keep their text and their order when
     the budget is sufficient.
4. Scope: the demonstrated segment is the CSV-to-`Dense` path in an
   execution session. `demos/candle/model.rn` is a file, and its later
   steps remain on the old route; nothing more is claimed. The
   non-generated build keeps its current registration. No old-engine
   output is pinned.

## Constraints

- All 40 inventoried items stay at their published source. There is no
  source-condition transition in this record, and the published guard
  runs on all 40 first.
- The user authorized a `host_call` edit on 2026-11-19, with a stop on
  any band failure. That edit failed its screen, was attributed, and is
  withdrawn. The failed candidate stays as the measurement reference.
- The early gate is the complete fixed-route screen on the compiled
  carrier with a synthetic method, before any Polars work. If either
  band fails, stop and attribute. The band is never widened for this
  record.
- No other interpreter, opcode or Store layout change. No new crate.
  Tabs.

## Controls

- Real `rnx-polars repl --engine=execution`:
  - `read_csv` in one input, then `let d = df.to_dense(names, "f32")?`
    in a later one;
  - `d.shape()`, `d.names()` and `d.dtype()`;
  - row-major values and order checked on the host side;
  - `f32`, `f64` and an integer column converted with the frame live;
  - reset.
- Carrier, on the synthetic method:
  - unchecked ingress and handler substitution;
  - a moved argument, and the competing error order;
  - no copy of an oversized rejected input;
  - the cap checked before producer work;
  - panic, fuel and interrupt;
  - exactly-once destruction;
  - non-session admission refused before a call.
- Credit:
  - at the documented peak minus one byte, `StoreCapacity` before any
    copy, with the frame usable afterwards;
  - at exactly the peak, admitted;
  - a second conversion sees the reduced credit;
  - dropping the `Dense` restores it.
  - Allocation is measured in isolation from setup and teardown.
- Refusals and errors:
  - a wrong receiver, or a moved input;
  - malformed, oversized or duplicate names, without the oversized
    input being copied;
  - a bad dtype;
  - a missing column;
  - null, range and non-finite errors, in their existing order.
- Ordinary checked reads (`width`, `height`) still pass. File routing is
  unchanged.
- The inventory admits exactly the new sites, each with a live witness.
- A mutation per rule fails its control.
- One complete final.
