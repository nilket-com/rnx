# ADR 0313: A script loads a model on the new engine

Status: Proposed

Base: published 6e96db9815971789cc4d9e1b25d8337b4728252f (fix after ADR 0312).

## Problem

ADR 0312 runs a script file's `pub fn main` on the new engine through
`read_csv`, `to_dense`, `Tensor::from_dense`, `rank` and `elem_count`.
The next call in `demos/candle/model.rn` is `candle::Mlp::load(path)?`,
which loads the demo's fixed-weight model. On published main:
- a standalone load selects the old engine (`Native`);
- the execution REPL refuses it by name;
- the full demo runs on the old engine (`OpaqueArgument`).

What `Mlp::load` does (`adapters/candle/src/lib.rs`):
- It takes `load(path: &str) -> Result<Mlp, String>`, registered with
  `build_associated::<Mlp>()`.
- It opens the file, checks that it is regular, and reads at most
  `MAX_FILE` = 64 MiB.
- It parses safetensors on the joined worker.
- It requires exactly the keys `fc1.weight`, `fc1.bias`, `fc2.weight` and
  `fc2.bias`, each F32 of the right rank, with dimensions from 1 to 4,096,
  compatible shapes, and at most 2^24 parameters.

The old engine's contract was measured at 6e96db98 on 12 inputs:
- The bundled 436-byte weights give `dims()` = (3, 8, 1).
- A real 67,108,864-byte file (dims (4094, 4095, 1), 16,773,121
  parameters) loads.
- One more byte gives "67108865 bytes, at most 67108864" before parsing.
- A short file gives "header too small".
- A missing path, a directory, a FIFO, wrong keys, dtype, rank, the
  dimension bound and mismatched shapes each have their exact strings
  recorded.

A valid file with 2^24 F32 parameters needs more than 64 MiB, so that cap
cannot be reached through a file.

No existing registration shape fits:
- 0306's free-name factory takes a string and an owned tree.
- 0311's qualified factory takes one shared native.
- `mlp_dims` returns a tuple of three integers, not a closed scalar.
- `Mlp` has no display. Formatting one explicitly is refused on the
  old engine.

Bounding the memory of a load is hard. Before the four tensors are even
validated, the parser allocates:
- structures sized by the header;
- possibly aligned or widened conversion scratch;
- new CPU storage, while the input buffer is still live.

A fixed multiple of the file size is not a proof for malformed input.

## Decision

1. The qualified call `candle::Mlp::load(path)` is a checked producer of a
   uniquely owned `Mlp`, in a checked script file's `main` only.
   - `build_associated::<Mlp>()`, with its original function and
     associated key, is preserved.
   - A typed one-string constructor returning `OpaqueResultString` is
     registered and checked before erasure against the nominal `Mlp`
     identity. The factory fact stays `ResultOfNative(Mlp)`.
   - `Mlp::load` is not put in `producer_inputs`, which proves an exact
     native argument.
   - Mismatched handlers, duplicate or foreign registrations, method-style
     use, wrong or moved paths, and a native escaping `main` are refused.
2. It runs through the ordinary host bridge, which already snapshots a
   checked `String` and imports an `OpaqueResultString`. Only the typed
   unary `String` signature is permitted, with a file-only association
   recorded before erasure. The original typed load is called through an
   exact binding of the original and checked handlers.
   - No credit packet is used, because sessions are out of scope.
   - `Executor::host_call`, the shared-producer classifier, the sliced
     dispatch and the receiver packet are unchanged, as are the
     interpreter loops.
   - The script still runs through the sealed sliced file driver.
3. Sealed file admission is extended for this constructor explicitly.
   - Only `PublicFileMain` analysis with the exact enrollment admits the
     call.
   - `FileModules::compile` re-binds the original and alternate
     allocations and the epoch, even for a standalone script containing
     only `Mlp::load`, which is admitted without `read_csv`.
   - Ordinary, async, one-shot and session entries never acquire the
     ability. That holds through a reused file-installed `Context`, a
     function value, or a stray registered reader.
   - Installing some other adapter never supplies its authority.
4. The load reuses the published body. That covers the file I/O and its
   checks, the parse and construction on the joined worker, the `String`
   errors and their path formatting, and their order:
   - open, metadata, regular file, length;
   - bounded read and growth check;
   - deserialize and materialize;
   - key set;
   - dtype and rank per tensor;
   - dimension ranges in input, hidden, output order;
   - shape compatibility;
   - parameter count.
   The 64 MiB and 2^24 rules are unchanged. Capacity stays its own
   execution error.
5. Sessions do not admit `Mlp::load` in this record. This is by entry
   kind, not by allowance, so it holds even with a very large allowance.
   - A pinned execution REPL refuses the input before any effect, with
     the named refusal recorded in the audit, and stays on the execution
     engine. It never runs that input on the old engine.
   - An explicit old-engine session still loads models as today.
   - Session admission needs a proven transient bound, or a bounded parser
     that keeps the error order. It is a later record.
   - A file `main` keeps the unbounded file allowance, as in 0312. If an
     embedder prepares a checked main with finite `Limits`, `Limits::bytes`
     bounds the retained payload at adoption only, consistent with 0312.
     Candle's parser scratch is not bounded, and no claim is made that it
     is.
6. The retained charge is:
   - the four tensors' actual CPU buffer capacities;
   - their `Arc` and storage headers and rank-specific layout and
     strides;
   - the owned `Mlp` box with its two `Linear` values.
   Each backing allocation is counted once. The parser input, temporary
   map clones, the worker and the `Result` wrapper are not model data.
   0306's adoption, variant rollback and last-owner release are reused.
7. `model.dims()` becomes a checked read returning `(i64, i64, i64)`. It
   follows the existing `I64_PAIR` pattern: a sealed host output for just
   this triple, a `Tuple([I64, I64, I64])` output certificate, the
   existing transactional importer, and a typed `opaque_method`
   registration. No general tuple ABI is added.
8. `Mlp` gets no display. An explicit format keeps the old engine's
   refusal, so `opaque_type` must not install a visible `DISPLAY_FMT` for
   it. Adding a display would be a separate, deliberate decision.
   - `frontend/opaque.rs::has_checked_display` and
     `RuntimeContext::opaque_descriptor` both use pointer equality with
     `DISPLAY_FMT` as the positive enrollment witness. A display-less file
     value instead uses a separate nominal and exact-handler witness,
     tied to the sealed constructor in its epoch. Other types keep their
     display checks.
   - A missing display handler is never accepted wholesale.
   - Unsupported formatting is refused before effects, and the existing
     route may render its usual diagnostic.
   - No descriptor callback exposes a new script-visible display.
9. Scope: loading and `dims()` in a checked file `main`. `forward` borrows
   two live natives and is the next record. The full `model.rn` must
   refuse before effects at `forward`. No old-engine output is pinned.

## Constraints

- All 40 inventoried items stay at their published source, and the
  published guard runs on all 40 first.
- An early complete fixed-route screen runs on the carrier and
  registration changes before the Candle integration. Stop on either band
  failure, and never widen a band.
- `final.sh --required` passes on the signed candidate before the
  complete final.
- No new crate. Tabs.

## Controls

- A file that loads the bundled weights and prints `dims()` selects the
  execution engine exactly once and prints (3, 8, 1).
- The real 67,108,864-byte file loads on the execution engine with dims
  (4094, 4095, 1). One more byte gives the existing size error before
  parsing. The generator is committed and the large input deleted, as in
  0312.
- Each recorded failure string comes out unchanged through the checked
  file route:
  - a malformed (short) file;
  - a missing path, a directory, a FIFO;
  - wrong keys, dtype and rank;
  - the dimension bound and a shape mismatch.
- The parameter arithmetic is tested at the cap and cap + 1, without a
  file.
- The full `model.rn` refuses before effects at `forward`. A marker before
  it is printed exactly once.
- In a pinned execution REPL, an input that calls `Mlp::load` is refused
  before effects. A preceding marker in that input is absent, and the
  next ordinary input in the same session succeeds on the execution
  engine. The same holds with a very large allowance.
- Ordinary, async, one-shot and session entries, a reused file-installed
  `Context`, a function value and a stray reader each fail to acquire
  the load. So does installing another adapter. The bare-call syntax
  and the actual qualified hash each have live controls.
- The reduced `read_csv`, load and `forward` prefix is refused first at
  `forward`. This is distinguished from any later refusal in the full
  `model.rn`.
- No-display witness: a foreign or wrong handler and a missing
  registration are each refused. Formatting a model is refused before
  effects.
- Retained accounting, with a synthetic Store, measuring the retained
  charge only:
  - aliases, last-alias drop and teardown;
  - the same model loaded twice;
  - exact and one-byte-short credit;
  - a failed variant publication, with no leak on an error;
  - the freed allowance reused.
- `dims()` on a moved model, and formatting a model, keep their existing
  errors.
- The inventory admits exactly the new sites, each with a live witness.
- A mutation per rule fails its control.
- One complete final.
