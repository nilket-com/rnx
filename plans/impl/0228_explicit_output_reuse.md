# Implementation plan: ADR 0228

Add explicit output reuse beside plain `map`, starting from `7802ce4a`.

Reuse enters through a sibling, reuse-only preparation entry beside
`PreparedColumn::allocate_batch`. Existing function bodies and signatures
stay unchanged, as does the batch layout. Do not touch the generic executor,
its operation helpers, masked.rs, or the Boolean or predicate drivers. New
callers of shared code are not assumed codegen-neutral; the exact plain-map
vectors decide. Initialized masked outputs take the fresh fallback.

1. Adapter: recover the F64 values buffer from a consumed Series through safe
   Polars APIs (unique Series/array ownership, then `Buffer::into_mut`). Any
   non-exclusive or non-whole case returns no buffer. Add no `unsafe` code.
2. Core: in the reuse-only entry, hand the recovered `Vec<f64>` to flat F64
   and F64-driver outputs. Reuse it only when it fits and its capacity is at
   most twice the needed bytes (checked arithmetic, never for a zero-length
   output). Otherwise drop it before allocating fresh. Never allocate and then
   swap. Charge its capacity once, with the same refusal boundary as a fresh
   allocation. Start it at length zero inside `NumericOutput`.
3. Script: `ColumnKernel.map_reusing(inputs, previous)`, with the same
   budget, refusal and discard behavior as `map`.

Tests, all committed:
- bitwise equality of `map` and `map_reusing` over the 0226 matrix: F64 bit
  patterns including NaN payloads, with validity compared separately;
- reuse and pointer identity when the buffer is exclusive;
- a fresh-allocation fallback for a clone, a slice, multiple chunks, I64 and
  Boolean previous outputs, an oversized buffer, an undersized buffer, a
  zero-length output and the initialized masked plans, with
  the cloned Series left unchanged;
- cancellation, refusal and budget exhaustion after `previous` is consumed;
- capacity accounting, including the limit refusal boundary with a reused
  buffer;
- the scaled large-then-small retention test;
- a cross-thread final drop.

Gates: the exact local clippy first. Then core controls, Miri for the
reuse tests, and the exact-vector comparison of plain `map` against
`7802ce4a` across the 0226 protected matrix. Next, a fault capture comparing
reuse with plain map at 100M warm. Then the 0224-harness timing with reuse
cells, 5 process samples. Finally the host, integration, generated and cross
gates and the commit checker. Publish the bench report only if every gate
passes.
