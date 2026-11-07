# ADR 0229: Write-only output for masked F64 arithmetic

Status: Accepted

Nullable single-operation F64 maps (`x + c`, `x + y` and the other fast
masked shapes) still take 168–192 ms warm at 100M rows. ADR 0228 could not
reuse their output. A scratch prototype that routed them through the F64
driver cost 3–4× more instructions per row and was dropped. Scratch phase
timing of published `c7d08d9b` shows where the time goes in a warm call:
- 88–94 ms in `filled(len, 0.0)`, a zero-write over the whole 800 MB output,
  including its first-touch faults;
- 72–92 ms in the masked SIMD kernels;
- 7–9 ms in validity intersection;
- about 1.5 ms in publication.

The kernels already write +0.0 into invalid lanes of mixed blocks. Only blocks
with no valid rows are skipped, and they rely on the prefill. So the
prefill is a second full pass over the output that exists only for those
blocks.

Decision: give the masked fast F64 shapes a write-only output.
- The output is a `NumericOutput<f64>` at length zero; there is no prefill.
- All-valid blocks use the existing `floats`/`float_pair` calculation, mixed
  blocks use the existing masked calculation, and all-null blocks are filled
  with +0.0 explicitly. Tails are masked. Publication happens once, after
  every row is written.
- Write-only entry points are separate symbols. They may share macro source
  with the initialized kernels (as in ADRs 0215/0216). The existing
  initialized primitives, their callers, the generic executor and its
  operation helpers are unchanged. No `&mut [f64]` is ever formed over
  uninitialized storage.
- Only the existing masked fast F64 shapes are routed. Every other plan keeps
  its route.
- Semantics are unchanged: strict nulls, +0.0 under invalid rows, NaN bits,
  fuel charged as before, and cancellation or refusal never publishes a
  partial output.
- `map_reusing` admission for this path is out of scope. It is the next
  record, under ADR 0228's ownership and capacity rules.

Acceptance against published `c7d08d9b`:
- outputs are bitwise identical across the full 0226 matrix (F64 bit
  patterns including NaN, validity compared separately), including all-null,
  all-valid, mixed, offset and multi-chunk layouts and lengths around 64 and
  1,024;
- unchanged paths have exact program vectors, apart from reviewed fixed
  per-call pins, with no per-row or per-block slope. The duplicate-instance
  guard passes;
- changed paths cost no more instructions per row than before at 4K, 64K and
  10M rows;
- 100M minor faults are no worse on any call;
- 100M timing (ADR 0224 harness, 5 process samples) improves the warm and
  first medians of alternating and deterministic-random const and add over
  plain `map` on `c7d08d9b`;
- Miri is clean for the new write-only primitives and their tests.

If an unchanged path shows per-row or per-block growth, stop and bring
disassembly attribution. Do not iterate on codegen tweaks; a rule exception
needs the user.

The rnx-bench follower also corrects the ADR 0228 report's Arrow/Polars
context. It cites the ADR 0224 harness figures, states that Polars leads the
flat chain, and has the renderer validate those context numbers.
