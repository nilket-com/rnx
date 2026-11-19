# Implementation plan 0313: A script loads a model on the new engine

Status: Implemented

Paired ADR: ../adr/0313_a_script_loads_a_model_on_the_new_engine.md.

1. Boundary note, read-only. Cover:
   - the typed one-string constructor, its file-only association and its
     pre-erasure check on the ordinary host bridge;
   - the file-admission extension and the epoch re-binding;
   - the session refusal by entry kind;
   - the retained charge;
   - the `dims()` triple output;
   - keeping `Mlp` without a display.
2. Restore the published source guard on all 40 items. Build the
   registration, admission and no-display witness changes with a synthetic
   no-display native shaped like the model, then run the complete early
   fixed-route screen. On a band failure, stop.
3. Integrate Candle. Add the file, boundary, error-string, `model.rn`,
   session-decline, accounting, `dims()`, inventory and mutation
   controls.
4. fmt, Clippy and preflight. Sign. Then run `final.sh --required` on the
   signed commit. Source review, one complete final, Results.

## Results

`candle::Mlp::load(path)?` and `model.dims()` run on the execution engine
in a checked script file's `main`. The load goes through the ordinary host
bridge, which snapshots a checked `String` and imports the
`OpaqueResultString`. It uses the typed unary `String` signature, with a
file-only association recorded before erasure. `Executor::host_call`
still hashes to its published `bf36f1ee`. `execute.rs`, `sliced.rs` and
the tools are unchanged.

`dims()` returns `(i64, i64, i64)` through a sealed triple output.
`Mlp` has no display. Formatting a model is refused before effects with
`OpaqueOperation`, and that exact reason is pinned on the CLI and in the
compiler. A weakened no-display witness changes the reason to
`OpaqueReceiver`. Types with a display keep `DISPLAY_FMT` as their
witness.

The checked synchronous file entry loaded the bundled model with dimensions
(3, 8, 1) and the 67,108,864-byte weights with dimensions (4094, 4095, 1).
All ten failure cases matched the published loader diagnostics, including
the one-byte-over input. Sessions are excluded by entry kind, not by allowance. A pinned execution
REPL refused the load before printing its marker, at both a small and a
very large allowance, and stayed on the execution engine. Sessions get an
inactive handler, and only sealed file compilation binds the file reader. The demo prefix through load and dims selected
execution. Adding forward selected the legacy route before the first effect.

The file-only binding uses the original typed constructor as its nominal
witness. The four CPU tensors and the model box are charged at adoption.
The allocation control measures the tensor allocations freed on drop.
The existing 64 MiB input limit applies to both engines. The file driver
does not bound parser scratch memory. The early fixed-route screen on the
core change passed all 32 workload and mode combinations in both bands.

The 2^24 parameter cap cannot be reached through a 64 MiB file. It is
covered by arithmetic controls at the cap and cap + 1.

Eight planted rule violations were each caught by their control, with the
unmodified controls passing. Signed root-tests passed 718 to 0. A required
final on 673a78ce passed all 17 selected stages. The complete final on
the same commit then passed all 101 stages on its first attempt: 43.2
minutes of accepted run, 44.3 end to end. Both fixed-route bands held,
with no memory.high, memory.max or OOM events.

Scope: loading and `dims()` in a checked file `main`. `forward` borrows two
live natives and is the next record. Session admission needs a proven
transient bound. No old-engine output is pinned.
