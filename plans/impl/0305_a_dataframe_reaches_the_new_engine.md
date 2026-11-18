# Implementation plan 0305: A DataFrame reaches the new engine

Status: Implemented

Paired ADR: ../adr/0305_a_dataframe_reaches_the_new_engine.md.

1. Boundary note, read-only: the registration-time metadata API in
   Rune's module system, how the adapter attaches it to the existing
   type, `empty` and `width` bindings, the single display owner, the
   charge, the session admission path, and confirmation that no
   runtime, host bridge, store or interpreter item changes.
2. Record the old engine's outputs for every session control first.
3. The metadata API, then the Polars enrollment of the existing
   bindings; the early fixed-route screen.
4. Session parity, charging and release, refusal and routing controls;
   inventory refresh; mutations.
5. fmt, Clippy, the suites, preflight and the commit checker; sign;
   source review; one complete final; Results.

## Results

`#[rune::function]` gains two opt-in modes, `opaque_factory` and
`opaque_method`. They carry a typed proof, made from the same callable
before its signature is erased, into the binding's existing sole
registration. The factory takes no input and returns its exact owned
type. The read takes a shared reference and returns a sealed scalar.
- Module registration checks the enrolled native `TypeId` and rejects
  conflicting names and keys before publishing.
- The producer and reader use their existing associated keys, so
  context installation, HIR's direct call and the receiver proof need
  no change.
- No context installation, compiler admission, HIR, Store, host bridge,
  interpreter or opaque runtime ABI code changed.
- The generator adds the two attributes on `DataFrame::{empty,width}`.
  `surface.json`, the catalogue and the generated witnesses are
  byte-identical.

`DataFrame` registers through `opaque_type` with one renderer: the
existing bounded preview, with its error text. It is charged
`size_of::<DataFrame>()` plus `estimated_size()`, saturating. The
handwritten-only build keeps its ordinary registration route.
`routing_inventory::host_inventory` admits exactly the two bound marks
and the generated-path type registration. Every other adapter
registration and template stays closed.

In the real `rnx-polars` REPL with `--engine=execution`, against explicit
legacy:
- direct reads and a frame retained across inputs give the same results;
- explicit `format!("{frame}")` matches exactly;
- reset clears the binding, and the same session then admits a new
  frame;
- an unenrolled `height` call is a located `OpaqueReceiver` refusal with
  no prefix output, and the retained frame still answers `width` on the
  next input.

**Intentional difference, bare DataFrame in the REPL:** legacy prints
`<::polars::DataFrame>`, while execution shows the bounded preview table.
That is the same renderer explicit formatting uses on both engines, and
it is more informative while still bounded. The session parity control
pins the complete legacy transcript and permits exactly this
replacement, with stderr equal.

The original generated-name file control still takes the old route. A
new file witness for the enrolled factory records `engine=legacy
refusal=OpaqueEntry` with exactly one effect. A tight
`ProgramDomain::compile_session` witness separates the real charge from
the Store's own overhead, refuses below it, and reuses the allowance
after release with live counts back to zero.
- Missing, mismatched and duplicate enrollments are rejected.
- Owned and mutable receivers, an extra input, a wrong factory return
  type and a raw handler fail to compile.
- Five mutations were rejected: `TypeId`, duplicates, wrapper charge,
  display and a missing factory mark.
- Rune still builds with `alloc` alone.

The early fixed-route screen passed (source condition, both bands
32/32). The first complete final failed in `root-tests` only. The
failing test, `capture_bound`'s
`a_descendant_that_never_stops_writing_does_not_hold_the_call`, reported
`timed_out=true cut_short=false out=0 err=0`. No bytes were captured.
The test's 200 ms command deadline did not establish that the detached
writer was live before cleanup. That
is a timing assumption in an unrelated test, and a readiness-driven
`fix:` commit follows. The unchanged retry v2 was refused at disk
admission before any check, 248 MB short. After 0.8 GB of Cargo output
was removed, the unchanged retry v3 passed all 101 stages: 36.2
minutes of accepted run, 36.9 end to end. Both fixed-route bands held,
with no memory.high, memory.max or OOM events.
