# Implementation plan 0310: A retained frame converts to a dense matrix

Status: Proposed

Paired ADR: ../adr/0310_a_retained_frame_converts_to_a_dense_matrix.md.

1. Boundary note, read-only. Cover:
   - the `budgeted_sliced_intrinsic` hand-off and the authentication of
     the handler, type pair and slots;
   - the sealed shape's registration and the proof extension;
   - the bounded names and dtype projection, and the precedence when
     argument errors compete;
   - the conversion's allocation sites and the documented peak, with
     whether the source frame can grow;
   - the single-use request's lifetime, and the cold output helper's
     credit re-read.
2. Measure the old engine's outputs for the success and error controls
   once, as a record only. Measure the peak with an isolated allocation
   test.
3. Restore the published source guard on all 40 items. Build the
   carrier and a synthetic method, then run the complete early
   fixed-route screen. On a band failure, stop.
4. The adapter's checked conversion, then the session, credit, refusal,
   inventory and mutation controls.
5. fmt, Clippy, the suites, preflight and the commit checker. Sign.
   Source review, one complete final, Results.
