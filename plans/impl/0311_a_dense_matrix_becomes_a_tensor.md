# Implementation plan 0311: A dense matrix becomes a tensor

Status: Implemented

Paired ADR: ../adr/0311_a_dense_matrix_becomes_a_tensor.md.

1. Boundary note, read-only. Cover:
   - the carrier's new shape (one shared native argument) in
     `budgeted_sliced_intrinsic` and its authentication;
   - the associated-producer registration metadata and the proof
     extension for one `Fact::Exact(Dense)` argument;
   - `Tensor`'s move to `opaque_type`, and `rank`/`elem_count` as checked
     reads;
   - the documented charge and where it is reserved before
     `Vec::clone`.
2. Measure the old engine's outputs for the success and error controls
   once, as a record only. Measure the charge with an isolated
   allocation test.
3. Restore the published source guard on all 40 items. Build the
   carrier change and a synthetic control, then run the complete early
   fixed-route screen. On a band failure, stop.
4. The Candle integration, then the session, credit, refusal, inventory
   and mutation controls.
5. fmt, Clippy, the suites, preflight and the commit checker. Sign.
   Then run rnx root-tests on the signed commit. Source review, one
   complete final, Results.

## Results

`candle::Tensor::from_dense(dense)?` on a retained `interchange::Dense`
produces a uniquely owned Candle `Tensor` on the execution engine.
`rank()` and `elem_count()` read it as checked methods, and the tensor
renders through its bounded Candle display. A session can therefore
run `read_csv`, `to_dense`, `from_dense`, `rank` and `elem_count` on
the new engine.

The producer is the qualified call alone, with exactly one `Dense`
argument. Method syntax, indirect calls, function values, files and a
raw `Result` are refused before any call. Candle installed without
`Dense` keeps the alternate dormant, and conflicts fail in either
install order.

It runs on 0310's shared-producer carrier, extended to one shared
native argument. `sliced.rs`, `execute.rs` and the tools are unchanged,
and `Executor::host_call` still hashes to its published `bf36f1ee`.

The published guard matched all 40 items. The complete early fixed-route
screen passed both bands on the synthetic carrier at 4d8105bf.

The combined Polars and Candle REPL converted the 64-row demo frame
through a retained Dense, in both adapter install orders. The two-column
f32 tensor reported rank 2 and 128 elements. The one-column f64 tensor
reported rank 2 and 64 elements. Reset removed the retained bindings.

On this 64-bit build the fresh CPU conversion allocated one values buffer
plus 264 bytes for Candle's two Arc allocations, shape and strides.
Isolated f32 and f64 measurements at 1, 257 and 65,536 rows with two
columns each matched this charge at peak and while the tensor was live.
One byte below the conversion charge the allocator observed zero growth.
The bridge separately budgets the private packet, native return boxes
and Store wrappers. A Store control released one byte and retried the
same input successfully, then recovered the output charge at last drop.


Sixteen planted faults were each caught by their intended control,
including a direct moved-slot witness added after the session test
alone missed that plant. rnx root-tests passed on the signed commit,
715 to 0, before the complete final. The complete final on 6eb36cad
passed all 101 stages: 47.9 minutes of accepted run, 48.8 end to end.
Both fixed-route bands held, with no memory.high, memory.max or OOM
events.

Scope: the session segment from CSV to tensor. `demos/candle/model.rn`
is a file, and `Mlp` and the later training steps stay on the old
route. No old-engine output is pinned.
