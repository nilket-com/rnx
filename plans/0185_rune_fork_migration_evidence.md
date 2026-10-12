# 0185: stock Rune fork migration — evidence

## Result and scope

The stock runner, evaluator, session, embedding interfaces and Jupyter metadata
now use Rune 0.15.0 at `03b1aa4dc29e4125392d0e03dab12bc408bf701d`.
The same exact Git source is resolved by the root, project tool, adapters,
servers and demos. Historical probes retain their original dependency.
Neither `fresh-core` nor the native tier is enabled by this migration.

Implementation: Codex. Plan and independent code review: Claude, over chatd.
This is a local implementation checkpoint, with no push or main update.
The isolated Rune branch `integration/0185-stock` contains one signed correction
commit, `03b1aa4d`, above `55d50133`. Publish that fork commit **before** publishing
rnx: clean machines cannot fetch this currently local revision from GitHub.
Validation used a locally seeded Cargo Git cache with the public source identity;
there is no path override or uncommitted Rune dependency.

## Reviewed behaviour

- The synchronous session resumes only `VmOutcome::Limited`; errors terminate
  once. A hard allowance charges VM instructions, native iterator permits and
  nested executions. The separate soft threshold yields the outer VM, while
  native calls keep their whole remaining hard allowance. Finite totals of 0,
  1 and exact minimum/minimum-minus-one are tested. The unlimited sentinel is
  clamped to a finite value in rnx.
- Cancellation is observed between slices. One long native call can delay it
  until returning or exhausting its remaining total. This is an instruction
  bound, not a wall-clock guarantee. A realistic sequence of short native calls
  is interrupted and the following session input still succeeds.
- Native exhaustion preserves a preceding print exactly once and reports the
  budget once. A separate sliced-session test counts an observable host effect,
  verifies one execution and verifies the session remains usable.
- Recursive rendering and JSON serialization share Rune's `MAX_VALUE_DEPTH=64`,
  counting root and leaves. Arrays, objects, tuples and options accept 63
  wrappers around a scalar and refuse 64; an empty container at level 64 is
  valid. Aliases remain legal and cycles are refused. The JSON reader keeps
  serde_json's existing independent recursion guard. Interactive previews keep
  the old visible depth through a one-level presentation-limit adjustment.
- Empty parser recovery spans now point at the next significant token rather
  than preceding dropped whitespace. Existing rnx caret expectations remain
  unchanged. Nonempty recovery retains its consumed-token span on the corrected
  parser. Missing-delimiter/arrow guards also pass on the unchanged `55d50133`
  parser, temporarily restored for a test-only confirmation and restored cleanly.

Claude accepted the fork implementation in chatd `01a1310a-80a8` (seq 8698),
and reviewed the rnx changes in `01a1310f-a67c` (seq 8707). The latter requested
native cancellation/no-retry regressions and unchanged preview presentation;
all three are implemented here.

## API port and changed expectations

Rune's fallible `rune::sync::Arc` owns Units and RuntimeContexts. Standard Arc
continues to own host data. Source loaders take source IDs; diagnostics expose
compile errors directly; execution uses the new resume outcomes; raw shims use
`Address`, `IntoReturn` and `Memory::store`. Polars emitter and oracle templates
were changed first, then generated files were regenerated and checked. The
support module is handwritten. Allocation failures continue to propagate.

The only existing output/acceptance expectation changes are:

- `tests/modules.rs`: anonymous-source module loading is still refused, now
  with “Source loading is not supported”.
- `tests/source_fmt.rs`: the fork correctly preserves three formerly corrupt
  backtick-literal fixtures. They now have exact successful-output assertions;
  the remaining corruption fixtures still refuse publication.
- `tests/eval_outcome.rs` and `tests/json.rs`: the reviewed shared 64-level
  contract replaces the old renderer/serializer 256-level boundary. Exact
  root/leaf boundary tests supplement the existing stack and cycle checks.
- Release/version metadata identifies the Git revision, rather than registry
  0.14.2. Licence-fetch failure controls use a still-resolved registry package,
  since a Git checkout is not a registry archive with `.cargo_vcs_info.json`.
- Polars callback budget errors match the new “Exhausted the budget it was
  given” message; the actual callback test suite verifies propagation.

Notice generation now separates `cargo tree`'s Git-source suffix from the
version field. Seven new fork packages have copied MIT/Apache licence texts
and exact revision/hash provenance in `third-party/licenses/SOURCES.tsv`.
Historical licence entries are retained. No executable or build output is
included in this change.

## Validation

Detailed logs, including failed attempts, are retained locally in
`/home/me/work/.reviews/0185-rnx-migrate1/logs/`. Commands use bounded `timeout`
invocations and dedicated/reused Cargo targets. The final standalone workspace
checks record argv, exit status and wall time in `final-workspace-checks.json`.

| Check | Evidence | Result |
| --- | --- | --- |
| Unchanged rnx baseline, all targets/doc tests | `codex-baseline1.log` | 523 passed, 3 existing ignored |
| Migrated full root suite, before final review tests | `codex-root-tests-final.log` | 527 passed, 3 existing ignored |
| Final root suite on Rust 1.95.0 | `codex-msrv-final.log` | 530 passed, 0 failed, 3 existing ignored |
| Root suite on Rust 1.98.1 plus focused corrected assertion | `codex-root-tests-final3.log`, `codex-budget-final.log` | 529 passed, one test expectation corrected; budget group 9/9 |
| Polars actual callback suite | `codex-polars-callbacks1.log` | 9/9 |
| Polars all-target/test-support API check | `codex-polars-check2.log` | Passed |
| Candle/Postgres/HTTP server/demo Polars/project tool, final pin | `final-workspace-checks.json` | All five passed |
| Candle demo all-target check, final pin | `codex-demo-candle-check2.log` | Passed |
| Jupyter tests | `codex-jupyter-tests1.log` | 25/25 |
| Polars generator tests | `codex-generator-tests1.log` | 55/55 |
| Deterministic regenerated bindings/oracle | `codex-generate-check2.log`, `codex-generate-check-final.log` | Passed twice |
| Rune full stock library | `codex-fork-lib1.log` | 578/578 before supplemental tests |
| Rune budget tests, including nested async/generator | `codex-fork-slices5.log` | 9/9 |
| Rune parser recovery guards | `codex-fork-spans4.log`, `codex-fork-guard-baseline.log` | Passed |
| Rune formatter | `codex-fork-fmt1.log` | 47/47 |
| Rune Clippy library, warnings denied | `codex-fork-clippy1.log` | Passed |
| Rune no-std alloc configuration | `codex-fork-nostd1.log` | Passed |

The full root suite includes transcript/demo, run/eval/session, formatting,
JSON, interruption, async, module loading, release metadata and embedding gates.
Adapter all-target checks compile their test sources; they do not claim every
adapter runtime/inference test or database service test was executed. No model
download, live database test or performance measurement is part of this record.

Initial checks caught API/Arc port errors and the three reviewed semantic
changes. Supplemental test attempts also caught a private-module test placement
and an overly narrow expected native-error string; tests were corrected without
changing production behavior. Their failed logs are retained. The final root
suite has three pre-existing ignored tests; none were newly ignored. Existing
VmResult/vm_try/vm_write deprecation warnings remain for a later record.

The free-disk admission floor was 30 GiB. Observed free space during Codex's
implementation/validation remained at least 65 GiB before the final MSRV build.
No cleanup of source or receipts was necessary. Final target sizes and free
space are retained in the local closing inventory.

Final standalone workspace wall times (Rust 1.98.1, offline all-target checks):
Candle 23.03 s; Postgres 32.85 s; HTTP server 16.83 s; Polars demo 81.72 s;
project tool 29.14 s. These include incremental compilation and are not clean
build estimates. All eight current lockfiles were independently checked for
one exact fork revision (`final-lock-pins.json`).

Final source review: Claude chatd `01a13117-a296` (seq 8722) approved the
changes contingent on completed validation. All those checks now pass: the
full Rust 1.95.0 suite, five remaining workspace checks and the focused native
budget assertion. Root formatting and `git diff --check` pass. Free disk after
the additional MSRV build was 64 GiB, still above the 30 GiB floor.
