# Implementation plan 0282: the complete final stops repeating work

Status: Proposed

Paired ADR: ../adr/0282_the_complete_final_stops_repeating_work.md.

1. Pin bbf0a941. Read rnx's build scripts (`build.rs`,
   `jupyter/build.rs`, `tools/build_identity.rs`), the final's stage
   list (`tools/column/accept.py`), the selection rules
   (`required_stages.py`, `package_gates.py`), the suite runner
   (`suite_stage.py`), the native contract runner
   (`native_contracts.py`) and the final controller (`final.py`).
2. Build identity: remove the always-rerun markers and the directory
   watch; the build scripts rerun on the checkout's HEAD, its branch
   reference, `packed-refs` and every tracked file, and never watch the
   index. Add the committed check that no build script deletes a file,
   with a planted negative control.
3. Remove `suite-adapter-polars` from the stage list and its
   prerequisites. Selection and package gates map the Polars adapter to
   `polars-suite`; the suite runner refuses the removed stage by name.
4. Native contracts: exclusive cases run first, one at a time; the rest
   run in worker processes at a width derived from cores and memory.
   Every case runs; rows return in manifest order to the unchanged
   catalogue check. Committed controls: the exclusive case overlaps no
   other, the shared cases overlap in more than one process, and a
   failing case leaves every other case run.
5. Replace every fixed job count the final reaches with one derived from
   cores and available memory, kept out of recorded stage commands.
6. Add the docs successor (`final.py --docs-successor PRIOR`) and its
   revalidation, and the 60-minute wall error for a complete final, with
   the committed controls from the ADR.
7. Concurrent stages (`stage_scheduler.py`): each selected stage's
   resources, resolved from its commands, its manifests' target
   directories (`cargo metadata`) and the declared targets of the
   wrapper stages, and the nested builds of the Polars tests and the
   inventory documentation; declared producers pass before their
   consumers start; a returned failure collected before each start; commit
   discipline and the fetches first; the fixed-route screen alone. A
   fork worker per stage runs the existing supervised child in its own
   main thread. A fifo jobserver sized by the final's job count (the
   build count with a quarter of the memory held back), its
   environment given to every stage but the fixed-route screen and the
   Python test suites; the controller takes each stage's weight in
   tokens whole (its widest test run, `RUST_TEST_THREADS` where a
   command names none) and tells the stage what it holds; the native
   contract runner uses those idle tokens and the pool, every token for
   an `exclusive` case. The Polars oracle report goes to an owned path
   named by `RNX_ORACLE_RESULTS` (generator template and regenerated
   test), and `GIT_OPTIONAL_LOCKS=0` for the controller and every
   stage. Stage-conditional prerequisites, and disk admission by
   declared bulk and target-directory growth per stage, rechecked before
   each stage. Committed controls as in the ADR.
8. The interrupt tests of `tests/async_execution.rs` that await
   `rnx_test::test_pending` signal on its readiness file
   (`RNX_TEST_PENDING_READY`, written by the fixture), with a startup
   bound; a never-ready control.
9. On the signed candidate: one complete final, then preflight and the
   commit checker. Bring each stage's duration beside its duration in
   ADR 0281's final. Results state what the code does; no paths under
   target/ and no development history.
