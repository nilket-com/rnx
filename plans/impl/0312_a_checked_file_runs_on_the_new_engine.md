# Implementation plan 0312: A checked file runs on the new engine

Status: Implemented

Paired ADR: ../adr/0312_a_checked_file_runs_on_the_new_engine.md.

1. Boundary note, read-only. Cover:
   - the sealed entry: its owned domain, the argv construction and
     cleanup;
   - the exact-local file policy and the escape checks;
   - where the file runner selects the entry, and the pre-effect
     decision;
   - failure and diagnostic mapping, without borrowing `Program`;
   - the work-budget, signal and lifecycle handling;
   - the unbounded file allowance and the injected test limit;
   - the purpose-aware extension registration and the private file
     context, and whether isolating it stays narrow (otherwise opt-in);
   - the Polars diagnostic settings, re-measured on the path reader.
2. Measure the old engine's outputs for the witness and controls once,
   as a record only.
3. Restore the published source guard on all 40 items. Build the entry
   with a synthetic file control, then run the complete early
   fixed-route screen. On a band failure, stop.
4. Wire in the file reader and the witness. Add the admission, escape,
   failure, signal, binding-isolation, routing, inventory and mutation
   controls. Run the large-file CLI witness; automatic routing changes
   only on its pass.
5. fmt, Clippy, the suites, preflight and the commit checker. Sign. Then
   run rnx root-tests on the signed commit. Source review, one complete
   final, Results.

## Results

A synchronous `pub fn main(args)` that needs checked native calls now runs
on the execution engine. It runs inside a sealed, private one-member
`ProgramDomain`. The engine is chosen at compile time, before any effect:
`FileModules::compile` returns either an ordinary program, which keeps the
existing driver, or a checked main, which runs only on the checked path.
`Executor::host_call` still hashes to its published `bf36f1ee`, and no
interpreter loop or inventoried item changed.

Files read CSV with the published explicit-schema path reader. The frame's
logical retained size is charged once at adoption. Parser peak memory is
not bounded on this path, exactly as on the old route. Only the
purpose-aware CLI run install (`Extensions::install_file`) binds that
reader. Sessions and ordinary embeddings keep the 0307 snapshot reader and
its `i32::MAX - 1` cap. The file allowance stays unbounded, as for ordinary
files, and `RNX_SESSION_STORE_BYTES` stays session-only.

The execution engine selected the combined demo file once and produced a
rank-two tensor with 128 elements from the two selected feature columns.
The full model and sales scripts retained pre-effect fallback.

With the default file allowance, the combined CLI read a 2,147,484,169-byte
CSV through an argv path. It reported two columns, tensor rank two and
8,388,610 elements. An independent reader verified both columns across the
entire file. Peak CLI RSS was 4,480,108 KiB on the integrated run. A separate
execution session with 8 GiB of Store credit refused the same CSV at the
snapshot limit. The input was deleted after the explicit check.

The synthetic file-entry screen at 55e9559d passed both fixed-route bands.
All 40 protected source items matched the published base. The real CLI
preserved the parser diagnostic with backtraces enabled. A forced Polars
panic (`POLARS_PANIC_ON_ERR=1`, alone or with backtraces) selected the
legacy reader before effects. The old engine exits 101 there. Focused checks covered
the one-byte Store boundary, argument and result ownership, file location,
fuel across slices, and a SIGINT sent after the read.

Two older Polars session tests had pinned the pre-0312 file route
(`engine=legacy refusal=OpaqueEntry`). They now assert the whole selection
line. The CSV `width()` file selects `engine=execution`. A file using
`height()?` falls back with `refusal=OpaqueOperation`, because only
session entries may propagate 0308's typed native error.

Four complete-final attempts failed before the passing run:
- On 2ade9347, clippy failed with `too_many_arguments` in the
  non-`project-sources` `runner::run`. The fix grouped the file inputs
  as `FileContext` and removed the wrapper and its old `allow`.
- On e8e5d384, polars-suite failed on the two route pins above.
- On 989c50dc, rune-clippy failed with `items_after_test_module`. The fix
  moved the unchanged `opaque_result_handler` above the test module,
  byte for byte.
- On 4188c180, root-tests failed in 0304's
  `nested_module_files_use_execution_and_keep_layout_precedence`.
  Its bare `!text.contains("999")` matched the scratch path's
  nanosecond nonce (`...359997603`). That test is unchanged by 0312 and
  is fixed separately.

On 4188c180, a required final passed all 18 selected stages. A rerun of
the complete final on the same unchanged commit then passed all 101
stages: 35.9 minutes of accepted run, 36.5 end to end. Both fixed-route
bands held, with no memory.high, memory.max or OOM events. Signed
root-tests passed 717 to 0 before it, and 13 planted faults were each
caught.

Scope: the checked synchronous file-main prefix. `model.rn` (`Mlp`,
`forward`, `Tensor::to_dense`, `with_dense`, sort, column reads) and
`sales.rn` (lazy queries) still select the old engine before effects.
No old-engine output is pinned.
