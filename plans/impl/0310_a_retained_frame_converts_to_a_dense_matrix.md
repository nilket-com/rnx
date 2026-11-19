# Implementation plan 0310: A retained frame converts to a dense matrix

Status: Implemented

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

## Results

`df.to_dense(names, dtype)?` on a retained, checked Polars frame
produces a uniquely owned `interchange::Dense` on the execution
engine. The matrix's existing `shape()`, `names()` and `dtype()` read
it.

The carrier leaves all 40 inventoried items at their published
source. `Executor::host_call` still hashes to its published canonical
`bf36f1ee`.

A first carrier edited `host_call` under the user's authorization of
2026-11-19. Its early screen failed: `shared_rows` native-on rose
+0.87% (W) and +0.80% (T). A split build pinned the cause on that body
edit, which outlined two `BVec<u32>::push` sites in native
compilation. They account for all 83 additional calls (7 + 76) with
no change in behavior. That carrier and its transition were
withdrawn.

The carrier now hooks `Executor::budgeted_sliced_intrinsic` within its
existing budget scope, for the exact registered three-argument shared
producer only. Its early screen, on carrier 96e5bd05, passed all 32
per-record and all 32 cumulative comparisons. The complete final below
re-screened the integrated candidate.

Embedders may install Polars first. A checked session refuses
`to_dense` at admission until `interchange::Dense` is installed.
Descriptor conflicts are rejected before a module's ID is claimed or
anything is registered. A rejected install therefore never blocks a
later valid one; both install orders and a retry are covered.

One difference from the old engine is disclosed, not pinned. A names
argument that was already moved reports the execution engine's
existing moved-argument access error. The old engine reports the
ordinary vector-of-names error. Otherwise, the competing-error order
measured once on the old engine is preserved:
- the typed dtype conversion comes before the names body;
- a duplicate comes before a bad dtype token;
- a malformed later name comes before an earlier duplicate;
- all column lookups come before numeric and null preflight, which
  comes before the fill;
- within the fill, request order decides between range and non-finite
  errors.

The execution REPL retained a CSV frame across inputs, produced f32
and f64 matrices, and read their shape, ordered names and dtype.
Numeric and null errors preserve the source frame. Physical arrays and
scalar columns are borrowed directly. The lookup walks the existing
columns without populating a frame schema or a scalar's series cache.

The bridge reserves the two native packets, the Store wrappers and the
bounded name projection. Its duplicate-check scratch is released
before it owns the selected names. The typed call reserves its native
Result and the larger of the native success and error wrappers. After
these debits the adapter's documented admission threshold in bytes is:

- The row-major buffer, rows × columns × element size.
- For each column, two machine words for lookup plus four hash buckets
  of two words and one control byte for the validating constructor.
- Two Arc/Vec headers and 128 bytes for small-table and control tails.

All proportional arithmetic is checked or bounded by the validated
shape. The name buffers move into the result. The Store charges the
retained native once at adoption and releases the charge on drop.
In an isolated integer-to-f64 conversion with 262,144 rows, the
adapter threshold was 2,097,444 bytes. At that credit its measured
allocation peak above setup was 2,097,284 bytes. One byte below the
threshold the producer returned the capacity marker and peaked at
16 bytes. These figures exclude argument projection and teardown.

The first complete final, on b5fa02ce, failed in root-tests: 28 rnx
session tests panicked. On inputs without opaque values, validating a
plain `?` indexed an empty proof table. 2ced6db2 (the install-order
repair) still had that line. The async regression reproduced that panic before
the repair. With the checked lookup, the local root-tests recipe
passed, including both success and error propagation through an
awaited function with plain values.

Sixteen independently planted faults were each rejected by their
intended control, and each unplanted twin passed. The complete final
on 08562798 passed all 101 stages: 47.2 minutes of accepted run, 48.3
end to end. Both fixed-route bands held, with no memory.high,
memory.max or OOM events.

This opens the CSV-to-`Dense` segment of a model workflow in an
execution session. `demos/candle/model.rn` is a file, and its later
steps stay on the old route. Candle `Tensor::from_dense` is the next
record, on this carrier.
