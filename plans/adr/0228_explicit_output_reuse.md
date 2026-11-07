# ADR 0228: Explicit output reuse for column kernels

Status: Proposed

After ADR 0227, warm 100M-row F64 const and add maps are still about 50–105 ms
slower than Arrow and Polars. The chain gap is smaller: about 12–24 ms against
Arrow, whose chain also faults (388–1,286 per warm call). Read-only attribution
of published `7802ce4a` shows that every warm rnx call maps a new ~800 MB
output. It takes 622–1,133 minor faults to populate it and 40–60 ms of
process system CPU. The previous result is then unmapped. Arrow const/add and
Polars warm calls take 0–2 faults because their allocators keep freed pages.
The system CPU is an opportunity envelope, not a measured page-zeroing cost.
Arithmetic, memory bandwidth and validity work remain.

The user's constraint rules out hidden retention: a workflow that peaks at
60 GB and then needs 10 MB must not keep holding 60 GB. A cache inside the
engine or session would need session-wide accounting, eviction before any
unrelated allocation is refused, and an idle timer that runs without another
map. `MADV_FREE` does not release memory promptly. This record does neither.

Decision: add an explicit, consuming reuse call. Plain `map` is unchanged and
always allocates fresh output.

- Script: `kernel.map_reusing(inputs, previous)` takes `previous` (a Series)
  by value. Rust host: the matching adapter function takes `p::Series` by
  value.
- The F64 values buffer of `previous` is reused only when it is exclusively
  owned and whole. That means a single chunk, offset zero, full length, and
  recovered through Polars' safe exclusive conversion (`Buffer::into_mut`),
  with exclusive ownership through the Series and array layers. Extraction
  must not take a shallow clone. Otherwise (a clone, slice, multiple chunks,
  a different dtype, or a foreign owner), this call's ownership of `previous`
  is dropped and the call allocates fresh output. Other aliases keep their
  backing. The result is the same either way. The consumed exclusive backing
  is overwritten. No other live alias is ever mutated.
- The buffer is reused only when the output fits and the capacity is at most
  twice the needed bytes, using checked arithmetic. A zero-length output
  never reuses a buffer. Otherwise the buffer is released before the fresh
  allocation; reuse never allocates and then swaps. No memory is held beyond what the caller holds. When the caller
  drops the result, everything is released.
- Reused capacity is charged against the kernel's limits exactly like a fresh
  allocation, by capacity rather than length, and is charged once.
- NumericOutput invariants hold: length zero until every row is written,
  invalid lanes zeroed, tails masked, publication once. Cancellation, refusal,
  budget exhaustion and any failure after `previous` is taken drop it. Stale
  bytes can never be published.
- Scope: F64 outputs that already publish through `NumericOutput`. These are
  flat F64 outputs and F64-driver plans, including the nullable chain. Reuse
  enters through a separate reuse-only preparation entry. The existing
  `allocate_batch`, executor function bodies and signatures, and batch
  layout stay unchanged. Initialized masked outputs (nullable const and add),
  I64 and Boolean outputs all take the fresh fallback in this record.

Acceptance against published `7802ce4a`:
- `map_reusing` and `map` outputs are bitwise identical across the full 0226
  matrix, both with reuse and with every fallback. F64 values are compared as
  bit patterns, so NaN payloads count; validity is compared separately.
  Non-F64 cases exercise the fresh fallback;
- the plain-`map` program vectors are exact, apart from reviewed fixed
  per-call pins, with no per-row or per-block slope on any unchanged path;
- warm 100M reuse calls have at most 1% of the plain-map minor faults in the
  four reused cells;
- in the ADR 0224 harness at 100M (5 process samples, warm calls), the
  `map_reusing` median is faster than plain `map` in the four reused cells:
  flat const, flat add, flat chain and alternating chain. The two fallback
  cells (alternating const and add) are reported but not required to
  improve. Everything is reported descriptively against Arrow and Polars;
- a retention test passes: after a scaled large-then-small sequence through
  `map_reusing`, the result capacity and the process allocation counter fall
  to the small working set;
- Miri is clean for the reuse and fallback tests, and non-Linux builds are
  unaffected.

If any plain-map path shows per-row growth, stop and bring disassembly
attribution. Do not iterate on codegen tweaks. If any timing cell is not
faster, stop for review before publication.

Deferred: reuse for initialized masked outputs. It would add callers to the
shared masked executor, or reroute those plans.

Rejected for now: a session-owned recycling slot, which would need
session-wide eviction and an idle timer to meet the constraint, and
`MADV_FREE` retention. Plain `map` keeps paying for fresh pages by design.
