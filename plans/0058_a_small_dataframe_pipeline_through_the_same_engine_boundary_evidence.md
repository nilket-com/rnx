# 0058 gate 1: registration works; direct collect meets a runtime stop

Status: stopped for review, 2026-09-17. The plan is e1556c9, including the accepted
fourth native type and group_by(...).agg(...) spelling. No adapters/polars product
has been created. Root code, dependencies and execution policy are untouched.
Gate 1 is not passed; gates 2–6 have not begun.

## The discriminating failure

A real native LazyFrame::collect under the candidate Polars 0.55.2 panics with:

    can call blocking only when running on the multi-threaded runtime

The backtrace goes through polars_plan's DSL-to-IR metadata fetching,
polars_async::RuntimeManager::block_in_place_on, and Tokio block_in_place. It
then shows Rune running inside rnx's current-thread drive_async. This is not a
Rune registration error, an unawaited task or a private-API problem.

| Real assembled executable | Result |
| --- | --- |
| file: construct a frame | exit 0, opaque DataFrame |
| file: collect its lazy plan | exit 101, native panic |
| synchronous eval: same collect | exit 0, opaque DataFrame |
| eval promoted by awaiting time::sleep(0): same collect | exit 101, same panic |

The full reuse/aggregation script first fails in run, then succeeds unchanged
inside a synchronous eval block. The trace and the four smaller controls are
retained. Both failed processes are reaped. The script does not catch this as a
Result; no adapter catch_unwind, thread dispatch, runtime replacement or fork was
introduced to conceal it.

The planned direct synchronous calls are therefore not a usable file-runner
adapter on the pinned graph. A later boundary decision must specify where native
engine work runs outside this incompatible executor context and who owns and
joins it. It may be an adapter boundary or a host capability; this probe does not
establish that a root change is necessary. Switching all rnx execution to a
multithreaded runtime is not implied by this failure. Neither is detaching work
and reporting cancellation while it is still running.

## What the independent controls establish

The bench workspace builds with exactly polars 0.55.2, defaults off and lazy/csv/
parquet enabled, plus rnx's public Rune re-export and Extensions. No additional
Polars feature was added to make it compile. Its fixed frame has k and v columns
with rows (a,1), (a,2), (crab emoji,3).

DataFrame, LazyFrame, LazyGroupBy and Expr all register. Synchronous Rune code
filters v > 1 via gt(lit(1)), groups then aggregates, collects twice, and reuses
the same group-by and expression arrays. Results are (a,2), (crab emoji,3).
The original frame remains usable. Native ADD constructs Expr and preserves both
operands; (value + one).sum() and value.add(one).sum() give identical sums
(a,5), (crab emoji,4). A missing-column collect returns an error, followed by a
successful collect from the original plan. This supports the proposed borrowed
wrappers and optional ADD; it does not close every arithmetic precedence gate.

Public column access can inspect a bounded prefix without rendering the whole
frame. The fixture observer bounds rows and string inspection and returns simple
values for assertions. It is not the full escaping/layout/byte-bound preview;
that remains open. Bare frames stay opaque, as the record requires.

In a live synchronous session with POLARS_MAX_THREADS=2, task count changes from
one to three on first collect. Three event descriptors are added. Those exact
observations persist through an idle interval, another collect and reset; the
second collect returns normally without needing a previous input's runtime to
resume. Process exit removes the process and its threads. This is engine-owned
global runtime state, not proof that reset should destroy it. Allocation disposal
and broader per-operation ownership gates remain open because of the stop.

## CSV schema is not header validation

Supplying with_schema(k:string,v:i64) accepts a file headed x,y and presents k,v.
It even accepts a one-column file for this two-column schema. So with_schema
alone cannot enforce the draft's promised names/arity.

Inference/dtype-overwrite preserves ordinary header names but de-duplicates:
`k,k` and `k,k_duplicated_0` become the same public inferred names. Rejecting any
name with that suffix would also reject a legitimate literal header and is not
a solution.

A public-reader route exists: read the first record as data with has_header=false,
infer_schema_length=0 and n_rows=1. It preserves the two different headers,
including duplicate names, quoted comma and embedded newline. The probe records
all four reading modes across ordinary, renamed, duplicate, literal-suffix,
quoted, reordered, short and extra headers, with discriminating assertions.
Production can validate that raw string record then rewind the same file handle
for typed reading; no handwritten CSV parser is needed. This is a route to test
further, not a completed file-read contract or concurrent mutation guarantee.

## Provenance and reproducibility

Rust crate 0.55.2 embeds VCS commit d7488c71ecfbc77790292ff5b365b991c08380ce.
Python tag py-1.44.2 points at 1bd8ec12f42d40fcec62badf32ef2177d2377d8d and
its workspace declares Rust version 0.55.1. They are different source revisions.
The installed Python runtime's build_info reports version only, not an engine
commit or full feature/compiler provenance. No matched-engine or boundary-speed
claim follows. A same-revision build pair remains a separate gate if that
attribution is wanted.

Python 1.44.2 and polars-runtime-32 1.44.2 are installed only in the ignored probe
venv. The fixture records wheel filenames, published SHA-256 values, verifies
downloaded bytes and checks installed .py/.so bytes against those wheels. Cargo
has a checked-in lockfile, the resolved package/feature graph and a 385-package licence
inventory with declared expressions and available text hashes. This is not a
completed redistribution-notices deliverable for a product adapter.

The initial source-only check and final release build pass. The probe is formatted
and clippy-clean with warnings denied. The first build attempt's API spelling
mistakes are retained separately; they are fixture mistakes, not stop evidence.
No startup race, cold-build cost, Parquet round-trip, notebook acceptance or
Windows execution is claimed. The Python installation default thread count is
not used as a performance comparison.

Reproduce with rnx-bench/probes/polars-boundary/README.md. Raw results, source and
binary hashes, compiler identity, backtrace, graph and provenance are under
rnx-bench/results/polars-boundary-0058. The next decision is the native execution
boundary; the adapter implementation stays stopped pending that review.
