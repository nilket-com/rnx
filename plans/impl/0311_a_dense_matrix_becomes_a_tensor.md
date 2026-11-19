# Implementation plan 0311: A dense matrix becomes a tensor

Status: Proposed

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
