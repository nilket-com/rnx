# Implementation plan 0282: the complete final stops repeating work

Status: Implemented

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
   index, and emit only paths that exist, adding the loose branch
   reference's directory. Add the committed check that no build script
   deletes a file, with a planted negative control, and unit tests of the
   watched paths in each reference state, run by the controller tests.
3. Remove `suite-adapter-polars` from the stage list and its
   prerequisites. Selection and package gates map the Polars adapter to
   `polars-suite`; the suite runner refuses the removed stage by name.
4. Native contracts: exclusive cases run first, one at a time; the rest
   run in worker processes at a width derived from cores and memory.
   Every case runs; rows return in manifest order to the unchanged
   catalogue check. Committed controls: the exclusive case overlaps no
   other, the shared cases overlap in more than one process, and a
   failing case leaves every other case run.
5. Replace every fixed job count the final reaches, except the
   fixed-route screen's pinned recipe, with one derived from the CPUs and
   the memory the final's control groups allow, kept out of recorded
   stage commands.
6. Add the docs successor (`final.py --docs-successor PRIOR`), whose
   selection scope `docs` holds only the stages that read the plans, and
   its revalidation; and the 60-minute wall error for a complete final,
   measured after release. Committed controls as in the ADR.
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

## Results

rnx's build scripts rerun when the checkout's HEAD, its branch reference,
that reference's directory, `packed-refs` or a tracked file changes, and
at no other time; each watched path exists, so a build with no change
compiles nothing. No build script deletes a file, which a committed check
enforces. The Polars package suite runs once, as `polars-suite`, and
selection maps the Polars adapter to it. The native contract cases marked
`exclusive` run alone; the rest run in worker processes, as many as half
the CPUs and the final's memory allow, and every case runs before the
unchanged catalogue check reads them in manifest order. Cargo's job count
in the final follows the detected CPUs and the smaller of the host's
available memory and its control groups' headroom, and is not part of
any recorded command; the fixed-route screen's paired build keeps its
pinned four jobs. A docs successor accepts an amend to a passed complete
final that has the same parent and changes only files under `plans/`,
counting renames as both paths, runs commit discipline alone on it, and
revalidates with the earlier final. A complete final whose run,
finalization and release take longer than 60 minutes fails.

Stages run concurrently, each in a forked worker whose main thread
supervises its child. Each stage's resources are the Cargo target
directories and profiles its builds write, resolved by `cargo metadata`
from its commands and from the declared targets of the wrapper stages;
selection refuses a stage it cannot resolve. The Polars suite's
resources include the builds its tests start, and the inventories' the
documentation builds. A stage starts once every
earlier stage sharing a resource has finished; commit discipline and the
fetches run first, and the fixed-route screen runs with nothing beside
it; a stage also waits for its declared producers to pass. A fifo jobserver
holds the final's job count in tokens, the CPUs and memory allowing a
quarter of the memory to stay free, and the acceptance receipt records
that count with the control group's memory events: the controller takes each stage's
tokens whole before it starts, one or its widest test run's threads
(four where a command names none), and Cargo and rustc draw further jobs
from the same pool; a stage wider than the budget is refused before any
stage starts. The native contract cases run on their stage's idle
tokens and the pool, and an `exclusive` case holds every token. Python
test suites and the fixed-route screen run outside the pool. Inside a
final, job and case widths come from that pool rather than a new memory
reading. The first failure stops admission, and a failed stage is collected
before each new start;
running stages finish with their receipts, rows return in selection
order, and that failure stays the cause; a signal drains every running
stage before release. The Polars suite writes its oracle report to an
owned path named by `RNX_ORACLE_RESULTS`, so no stage writes tracked
source, and git runs without optional locks (`GIT_OPTIONAL_LOCKS=0`)
throughout the final, so no status refreshes the index under a stage
that fingerprints `.git`. The tests that interrupt a pending await
signal it once the fixture reports, from inside that await, that it is
running; an input that never reaches it fails before any signal. Selection checks Valgrind and the Jupyter client only when a
selected stage uses them. Each stage starts only when the free disk
covers a floor plus the declared bulk of every unfinished stage and the
growth of the Cargo target directories they write, estimated from the
measured warm configuration, and the
acceptance receipt records that estimate, the filesystem and the lowest
free space seen.
