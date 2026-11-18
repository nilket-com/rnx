# ADR 0305: A DataFrame reaches the new engine

Status: Proposed

Base: published e4ce4727cff43229a4f1425485782ea7ffbe2620 (ADR 0304).

## Problem

No Polars code runs on the new engine. The removal inventory counts zero
of 6,111 Polars registration sites enrolled for it. Even
`polars::DataFrame::empty().width()` takes the old route
(`adapters/polars/tests/execution_routing.rs`). The adapter registers
ordinary host functions and generated wrappers, and only the old engine
can call them.

The new engine already has a checked path for native host values,
limited to the checked session entry: `Module::opaque_type` (a payload
charge and bounded display), `opaque_factory` (a zero-argument factory
returning a uniquely owned value) and `opaque_method` (a shared receiver
with a closed scalar or certified output). `interchange::Dense` uses it.

This is the first slice of milestone B3 in the removal inventory. It
stays outside the interpreter: ADRs 0301 to 0303 showed that changes
near the interpreter loop move its generated code.

## Decision

1. The existing names reach the new engine: `polars::DataFrame::empty`
   and `width`, as generated scripts already spell them. No new public
   name is added. A new name such as `empty_frame` would admit no
   existing script, and it would later need deprecating.
   - Today `Module::opaque_type`, `opaque_factory` and `opaque_method`
     register their own type, display, factory and associated items.
     They collide with the adapter's existing `DataFrame` type and
     `DISPLAY_FMT` registration and with the generated `empty` and
     `width` bindings. `opaque_factory` also inserts only one name
     component under the module, so it cannot name
     `::polars::DataFrame::empty`.
   - So this record adds a bounded, registration-time change to Rune's
     module API: typed opaque enrollment is carried into the binding's
     sole registration. The API sees the callable before its signature
     is erased, or consumes a proof created at that point and tied to
     the same handler. A post-hoc setter given a path, hash or claimed
     `TypeId` is not allowed: it could bless an owned, mutable,
     wrong-arity or raw handler as a shared read. Each registration
     keeps one owner, and nothing is registered twice.
   - `empty` is a static associated function: its host binding and
     factory enrollment use its existing associated key, the one the
     compiler metadata and HIR's direct call already use. `width` keeps
     its associated key, receiver and closed scalar result. The
     generated Rust bodies and documentation metadata are kept. If the
     generator must change, only those two emitted sites change, and
     regeneration leaves the public surface and generated inventory
     unchanged.
   - The existing typed bounds apply: a synchronous zero-input factory
     returning an owned `T`, and a synchronous shared `&T` receiver
     with a closed scalar output.
   - `DataFrame` becomes an opaque type through that path. Its display
     is one typed renderer, built on the existing bounded preview and
     keeping the current error text; it replaces the duplicate
     registration. Its charge is the fixed owned wrapper plus the
     estimated data, since an empty frame can estimate zero.
   - `empty` is enrolled as the owned producer and `width` as the
     shared scalar read. Both keep their generated semantics, including
     the joined-engine execution, the width conversion and failure
     behavior.
   - The old engine's behavior does not change.
2. A checked session runs a program that creates a frame with `empty`,
   reads `width`, and holds or displays the frame. Its results and
   errors match the old engine, except for reviewed differences.
3. Everything else in Polars keeps its current route:
   - any other function or method on a frame refuses with its typed
     refusal before effects and falls back where the frontend already
     falls back;
   - file runs, `rnx serve` and embedding keep the old route, since
     opaque admission is session-only;
   - frames moving into or out of Series, callbacks and any
     opaque-ABI extension are later slices, each with its own review.

## Constraints

- The only engine-side change is the registration-time metadata API in
  decision 1. It changes no runtime, host-bridge, store or interpreter
  code. Any other engine change found at the boundary review stops and
  splits the record.
- The fixed-route band holds.
- The adapter's existing suites, generated inventory and notebook
  witnesses are unchanged.
- No new crate. Tabs.

## Controls

- Route: the real `rnx-polars repl --engine=execution` (sticky
  execution, the extension installed in the checked session context)
  against explicit legacy:
  - `polars::DataFrame::empty().width()` is equal;
  - a frame retained across inputs, then read, is equal;
  - plain display on the REPL and explicit formatting match the legacy
    baseline, or the difference is named and reviewed;
  - an unenrolled method returns its pinned refusal variant at its
    location and executes no prefix effects.
- File routing: the existing `execution_routing.rs` control for the
  generated names still takes the old route with its effect once. A
  file witness for the enrolled factory reaches `OpaqueEntry` and
  falls back with one legacy effect.
- Charging: an existing bounded `ProgramDomain` admission control
  fails if the adapter's size callback is zeroed. Release and live
  counts are tested separately, with real REPL reset and close
  witnesses.
- Registration: one owner per registration. Owned or mutable
  receivers, an extra input, a wrong factory return type and a raw
  handler are rejected at compile time or registration; a duplicate or
  mismatched enrollment is rejected before any mutation.
- The `OpaqueEntry` file witness asserts its effect count explicitly.
- Inventory: `routing_inventory::host_inventory` admits exactly the
  three Polars enrollment sites, paired with live witnesses, and
  accounts for the automatically installed display method. Every other
  adapter registration and template stays fail-closed.
- A mutation per rule fails its control.
- One complete final.
