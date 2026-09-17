# rnx 0058: a small dataframe pipeline through the same engine

Status: gates 1–3 accepted 2026-09-17. Gate 4's project and notebook assembly
is ready for review in `_assembly_evidence.md`; gates 5–6 remain open.
`_preview_evidence.md` records the accepted bounded preview.
`_files_evidence.md` records the accepted CSV/Parquet contract.
`_boundary_evidence.md` preserves the original current-thread failure and
`_engine_thread_evidence.md` records its accepted adapter-side resolution.
The fifty-eighth record follows the completed 0050–0057 extensibility sequence. It tests a small
Polars adapter and a useful script, not a new dataframe implementation or a
claim that Rune makes Polars' query engine faster than Python does. At drafting no dependency had been fetched. Gate 1 now
records the pinned graph and a release prototype build, but makes no performance claim.

## Context

The desired example fits on one screen: Rune writes a tiny CSV, loads it through
Polars, filters rows, groups and sums, writes Parquet, and reads the result back.
Python can express the same pipeline over Polars' Rust engine. The experiment is
whether rnx offers a readable script and competitive launch/boundary costs.
Neither an interpreter-lock advantage nor a compute advantage follows merely
from writing a Rust adapter. No Python row callbacks belong in the comparison.

0051 provides trusted native registration and an assembled executable. 0057
provides declarations, generated Cargo assembly, locks and verified launch. The
PostgreSQL adapter already consumes both; Polars is another consumer, not their
first. Keep it outside the stock graph, just as PostgreSQL is.

Rune 0.14.2's VM comparison path calls partial comparison and converts Ordering
into bool. Consequently > cannot construct a deferred expression. Arithmetic
protocols have a different return path and need a real native-type probe before
we promise their use. rnx's renderer currently presents native values opaquely;
registering a frame does not install a rich table renderer.

The counting allocator observes allocation requests passing through Rust's global
allocator, across threads. Session ceilings are sampled at rnx boundaries, not
allocation rejection inside native code. A native call can overrun a ceiling;
allocations bypassing that allocator, mapped files and stacks are not covered.
Polars may own process-lifetime threads and internal runtimes independently of
rnx's per-input runtime. This record must measure that ownership rather than
turning a synchronous method into an imaginary cancellable operation.

Stock rnx timing is not an assembled-Polars timing. 0057 measured about 40.6 ms of
additional project verification/launch cost on its particular native tree. That
is neither a prediction for this tree nor a cost Python imports should hide.

## Decisions

### 1. An independent adapter, with provenance settled by a probe

Use adapters/polars/ as an independent workspace with its own Cargo.lock,
resolved graph and notices. The crate rnx-polars exports build for
Extensions::with("polars", ...), and builds rnx-polars as a thin main_with wrapper.
The shipped example also declares it as a plain native extension through the
0057 manifest. No native libraries or Polars features enter stock rnx's graph.

Candidate Rust pin: polars = "=0.55.2", default features disabled, initially only
lazy, csv and parquet enabled. Candidate Python install: polars==1.44.2 in a
private pinned environment, including the exact runtime wheel and its hash.
These are candidates to test, not a claim of matching engine revisions. Record
all additional required features before fixing the final pin; no silent switch
to Polars' defaults, nightly, an older release or a fork to get a build through.
A required unsupported compiler or incompatible public API is a stop for review.

Inspect release source, Cargo graph, Python build metadata and wheel provenance.
The Python 1.44.2 tagged workspace declares Rust package version 0.55.1; that
alone proves neither equality with the published crate nor equality with 0.55.2.
Upstream's 0.55.1 release note promises matching DSL with Python 1.43.2, which is
also not an identity statement about compiled code. Gate 1 records the exact
relationship before the record calls any comparison matched-engine.

The primary three-way result compares real installable products, with their
provenance differences visible. To attribute a difference specifically to the
language boundary requires an additional pair built from the same upstream
engine revision with aligned relevant features, optimizer settings and thread
count. If that pair cannot be established, keep the product comparison and
explicitly leave boundary-only attribution unproved. Do not select versions or
build flags after seeing which result favours rnx.

### 2. Four native values, a small composable surface

Expose opaque DataFrame, LazyFrame, LazyGroupBy and Expr values. These names do not expose
Rust internals or an arbitrary Polars object conversion API. Proposed script API:

| Call | Result |
| --- | --- |
| polars::read_csv(path, schema) | Result<DataFrame> |
| polars::read_parquet(path) | Result<DataFrame> |
| polars::col(name) | Expr |
| polars::lit(value) | Result<Expr> |
| frame.lazy() | LazyFrame |
| plan.filter(expr) | LazyFrame |
| plan.group_by(keys) | Result<LazyGroupBy> |
| grouped.agg(aggregates) | Result<LazyFrame> |
| plan.sort(column_names) | Result<LazyFrame> |
| plan.collect() | Result<DataFrame> |
| expr.gt(other) | Expr |
| expr.add(other) | Expr |
| expr.sum() | Expr |
| expr.alias(name) | Expr |
| frame.write_parquet_new(path) | Result<()> |
| frame.preview() | Result<String> |

The array-taking methods return Result for invalid Rune shapes, as the accepted
gate 1 prototype already did; the table now makes that fallibility explicit.
Arrays of Expr are used for keys and aggregates; sort takes ascending column
names, nulls first, matching Polars' default. The fourth opaque value keeps
`group_by(...).agg(...)` faithful to both Rust and Python; avoiding one wrapper
is not worth making this central line read differently. Scalars accepted
by lit are bool, i64, finite f64 and String; unit and other values refuse by type.
Null remains a data value from input, not an untyped expression literal in this
first surface. Schema is an ordered array of (name, dtype) tuples, with unique
nonempty names and dtype exactly string, i64, f64 or bool. Empty schema refuses.
CSV columns must agree in number and order with that explicit schema. Gate 1
must establish enforcement through the pinned reader without a second CSV parser.

All receiver methods borrow the caller's value and return a fresh wrapper. Reuse
of a bound frame, plan, expression, path, schema or expression array remains
possible after every call, including a failing one. Clone the underlying Polars
plan/frame as required by its consuming Rust methods. Shared underlying buffers
are allowed; promise neither deep copies nor a universally zero-copy boundary.
Collecting a plan twice executes twice. No implicit collect on formatting.

col and expression construction do not validate column existence. Polars errors
are returned catchably when the engine resolves/executes the plan. The adapter
names the operation and preserves the useful engine message. Invalid Rune shapes
refuse before I/O or execution. Do not stringify an unbounded offending object.
Fallible engine paths use Result, never unwrap or expect. Unexpected engine
panics are not promised to become catchable errors by this record.

The ADD protocol is optional syntax over expr.add(expr), conditional on gate 1
proving it returns Expr, has the expected precedence and preserves both operands.
No mixed scalar coercions: use lit explicitly. Comparisons remain methods even
if arithmetic succeeds. A failed arithmetic probe drops the optional sugar;
it does not justify changing Rune or using > with surprising semantics.

### 2a. Blocking engine work runs on a call-owned plain thread

The accepted gate 1 stop identifies an executor-context incompatibility, not a
need to change rnx's runtime. Polars' runtime manager reaches Tokio block_in_place;
a current-thread block_on disallows that operation, whereas a plain thread with
no entered Tokio context permits it. File execution and async-promoted eval
therefore failed while synchronous controls passed.

The adapter performs collect, CSV/Parquet reading and Parquet writing on a fresh
scoped std thread, with no entered Tokio context. Snapshot/clone the owned Send
Polars inputs and paths before spawning; Rune wrappers, references and VM values
stay on their calling thread. The worker returns owned engine data or an engine
error, and the caller constructs the Rune-visible wrapper only after joining.
Pure expression/plan construction, cloning and bounded value inspection remain
on the caller; they do not execute the engine or perform file I/O. Gate 1 checks
that distinction on the pinned version. Any newly exposed operation that can
enter the engine goes through the same boundary.

The native call owns its thread and joins before returning, on success and error.
Use a fallible scoped-thread builder so spawn failure is a named catchable error
for these Result-returning operations. An unexpected panic is joined and its
unwind resumed on the caller; it is not silently converted into an ordinary
Polars error. No detached job or global adapter thread queue is introduced.
Polars may still own its separate process-global pool, as already stated.

The call remains synchronous and blocks the caller until completion. Ctrl-C and
instruction budgets still cannot preempt it. Thread spawn/join is included in
all timings; a persistent owned engine thread is a later measured optimisation,
not a hidden change if the first numbers disappoint. Awaitable collect is also
later: dropping its future would not stop the engine work without another
explicit ownership/cancellation design.

Gate 1 must prove no current Tokio context inside each scoped engine thread,
completion and join before return, success from file and async eval, and nested
and overlapping engine calls with a small Polars pool. A nested call must not
wait on a single-worker adapter queue. Record adapter-owned scoped threads
separately from Polars' persistent threads, including error paths.

### 3. Local files and explicit side effects

Reads take borrowed Unicode paths, open a local regular file, and pass an owned
file to the reader. Reject directories and FIFOs without blocking; follow normal
filesystem symlinks as the existing fs reads do. No URLs, globs, cloud credentials,
path expansion or file discovery. A lazy plan is built from an already loaded
frame: this record does not expose lazy file scanning or defer path resolution.

CSV is UTF-8, comma separated, header present, double-quote escaping, strict
parse errors, no date inference, no lossy decoding and no skipped error rows.
Header validation reads the first record as string data with has_header=false,
infer_schema_length=0 and n_rows=1, compares its raw names/arity to the declared
schema, then rewinds the same owned file handle for typed reading. The accepted
probe shows with_schema alone replaces names, and inferred headers de-duplicate
them; neither can validate this contract. Use Polars for both passes, with no
second CSV parser and no reopening the path. Concurrent file mutation is not a
consistent-snapshot guarantee. Gate 2 must observe the same handle being rewound.
Explicit schema removes inference differences from the race. Use the pinned
reader's missing-field/quoted-empty semantics, document its measured answers,
and match them in Python. Short rows, extra fields, duplicate headers, CRLF,
quoted comma/newline and empty fields are gate 2 cases; do not guess their
answers from a different reader. Stop if enforcing the promised schema/layout
requires inventing a CSV parser in the adapter.

read_parquet preserves the file's native engine schema. The demo and preview
support only string/i64/f64/bool plus null; refuse other dtypes by column before
returning a frame from either read or collect. This is a deliberately narrow
first adapter, not a lossy conversion of arbitrary tables.

write_parquet_new opens with create_new, never truncates an existing path, writes
with an explicit codec/settings shared with Python, and reports flush errors.
Select uncompressed Parquet for the first comparison to avoid codec differences.
There is no atomic publication or crash durability promise: a failure after
creation may leave a partial new file, stated in help. No retries or implicit
cleanup of a path another actor could have replaced. Existing files stay intact.

No general data-size cap is invented here. Reads and collect can materialize the
whole dataset, and these synchronous native calls are not preemptible by rnx's
instruction budget or Ctrl-C. Execution resumes observing its own boundaries only
after the call returns. The shipped example is tiny; large-data safety, streaming,
background jobs and cancellable collect require later designs.

### 4. A bounded preview, without changing presentation contracts

preview returns a String, never writes a stream and never calls an unbounded
upstream table formatter first. It inspects at most ten rows and eight columns,
includes full frame dimensions and displayed column dtypes, and marks omissions.
Each name/cell has an 80-Unicode-scalar inspection allowance; the final UTF-8
string has an 8192-byte cap including headers and omission markers. Escape
control characters (including ESC, tabs and embedded newlines), and never cut a
UTF-8 sequence or an escape spelling. Reserve room for the omission marker.
These bounds govern preview work/output, not the frame's underlying allocation.

Use a simple deterministic textual layout, independent of terminal width and
Polars' environment-driven formatting. Float spelling must round-trip finite
f64, with null distinct from the string "null". The byte cap can stop the preview
before the row/column caps do. No essential content becomes invisible ANSI.

Users may return this string to the normal renderer or print it explicitly for
a table-like view. Bare frames stay opaque in REPL and notebook results. Automatic
native rendering, MIME bundles and inspection hooks are separate records.

### 5. The one-screen example and observable answer

The script creates a fresh CSV with fs::write_new. Its data has category and value
columns, repeated categories, a filtered-out row, one missing value and a
non-ASCII category. It reads with an explicit schema, filters value > 1 using
gt(lit(...)?), groups by category, sums into a named total, sorts category, then
collects, previews, writes new Parquet and reads it back. Expected sorted rows,
schema and null behaviour are written down before timing. Choose small integer
values whose sums cannot overflow. No Python UDF or Rune per-row callback.

Keep the example composable calls, not a single native demo_pipeline function
that hides all the script work. A second query reuses the original bound frame
and expressions. The external fixture validates the Parquet with Python and the
adapter, comparing values/dtypes/nulls, not compressed file bytes or metadata.
A failed query is followed by a successful query in the same session and kernel.

The example's checked-in manifest lives outside native dependency roots as 0057
requires; its runtime/adapter paths and resulting lock remain local identities.
Use private temporary inputs/output files for tests and a documented fresh demo
directory for people. No overwrite of user data. Notebook cells need no mapped
source imports: they call the installed adapter directly.

### 6. Three launches, with a separate boundary control

Measure (a) a private environment's Python plus import polars, (b) direct release
rnx-polars, and (c) rnx-project run using its generated Polars executable. Verify
that the two rnx assemblies use the same dependency graph/features and measure
each artifact's identity; do not assume the binaries are identical. Also launch
the generated artifact directly with the same entry/map to isolate the project's
verification cost from an assembly difference.

Two workloads: initialization/no data work, and the full tiny CSV-to-Parquet
round-trip. The initialization case must actually initialize/import the extension,
not rnx version/help (which intentionally skip registration). All three perform
equivalent observable work; they produce the same small textual summary. A fresh
output directory is prepared outside each timed launch, and cleanup happens
outside timing. CSV creation and both file reads/writes belong inside pipeline
time. State explicitly that close/flush is measured, not fsync durability.

Pin CPU affinity, Polars worker-thread count before process creation, environment,
versions, wheel/build hashes, CPU feature flags, allocator choices and engine
options. Record wheel/runtime provenance rather than assuming pip version alone
identifies the binary. Use repeated interleaved order, retain samples and outliers,
and distinguish warm filesystem/process launches from genuinely cold-cache work.
Use no-shell launch timing; peak RSS is separately measured for the whole process,
including native threads. Allocation-counter values are supplementary, not RSS.

Time first build from an empty target with cached downloads identified, warm
build, binary size, and Python installation separately. They are setup costs,
not pipeline time. Report project fingerprinted input bytes alongside verification
cost. No success threshold requires beating Python; correctness and truthful
measurement are the gates. Report any slower result with the same prominence.

Boundary-only experiments run many plan constructions and collections within
already initialized hosts, with equivalent observable work and same-revision
engine provenance as decision 1 requires. Do not infer conversion cost by
subtracting unrelated empty-program wall times or call the entire pipeline a
measure of Rune VM speed. Same-engine compute is a control, not the claimed win.

## Gates and stop points

1. Before adapter implementation, a source-only bench prototype pins/fetches the
   candidate graph and licences, proves native DataFrame/LazyFrame/LazyGroupBy/Expr registration,
   borrowed/reusable values, method comparisons, optional ADD and bounded preview
   access through public APIs. Establish CSV schema enforcement and engine-version
   provenance. Probe two collect calls separated by idle session time, reset and
   process exit, recording worker threads/file descriptors, not assuming 0053
   owns Polars' global pools. Stop for incompatible public APIs, a needed root
   change, or tasks depending on an rnx runtime turn after the input ends.
2. Tiny pipeline with independent expected rows and cross-read Parquet. Gate all
   CSV edge cases in decision 3, schema/type errors, empty results, missing columns,
   malformed Parquet, unreadable/non-regular input, existing output, write/flush
   failure, path and receiver reuse, null and non-ASCII handling. Late write
   failure may leave a partial file; the gate must not silently delete the evidence.
3. Preview at and past each cap: wide/tall frames, huge names/cells, ESC, tabs,
   combining characters and emoji, null versus strings. Bounded work must be
   structural, not a huge rendered string truncated afterwards. Bare frames stay
   opaque. No data-derived terminal control escapes from preview.
4. Real adapter assembly through 0051 and project lock/build/run through 0057;
   run, eval, session with reset and notebook with restart and recovery. Native
   values do not consume caller bindings. Existing stock binary cannot resolve
   polars. Settings stay pure; help/version do not invoke the builder.
5. Three-way timing, generated-direct verification control, peak RSS, build costs
   and same-revision boundary control or an explicit unproved attribution. Separate
   data-engine thread-pool lifetime from leaked per-operation handles. Demonstrate
   allocator visibility and the sampled ceiling's limitation without an OOM test.
   Observe Ctrl-C during a sufficiently long native collect in a child process
   with an external watchdog; report when it actually takes effect, then reap.
6. Root graph/manifests/locks unchanged; existing root suites, adapter tests,
   formatting, clippy, notices and packaged README checks. Compare stock startup
   before/after if any root source change is proposed; such a change requires
   review rather than being hidden inside the adapter. Windows type-check and
   Windows execution are separate results; no platform claim from Linux alone.

## Guardrails and later work

No Rust/Python driver rewrite, Rune fork, root Polars dependency, dynamic plugin
loader, table formatter hook, SQL interface, UDF registration, dataframe indexing,
mutation API, streaming sink, cloud connector or broad expression catalogue.
Expose only what this pipeline and its discriminating failure gates need.
Polars' global pool is engine-owned and may persist through session reset;
reset releases script bindings, not arbitrary process-global library state.

The likely product benefit is a compact executable with readable orchestration.
It remains a hypothesis. Different engine revisions, features, CPU dispatch and
allocators can dominate a small timing; record them rather than attributing every
delta to Python or Rune. A process-global pool and uninterruptible native work
remain relevant in notebooks even when a tiny CLI example is fast.

## Sources checked for this draft

- Pinned Rune 0.14.2 source: runtime/vm.rs internal_cmp and arithmetic dispatch;
  rnx src/memory.rs and Session::sample for accounting and ceiling boundaries.
- [Polars Rust 0.55.2 feature manifest](https://github.com/pola-rs/polars/blob/rs-0.55.2/crates/polars/Cargo.toml).
- [Python 1.44.2 workspace manifest](https://github.com/pola-rs/polars/blob/py-1.44.2/Cargo.toml).
- [Polars releases and DSL compatibility statement](https://github.com/pola-rs/polars/releases).
- [Expressions and contexts, Python and Rust examples](https://docs.pola.rs/user-guide/concepts/expressions-and-contexts/).
- [Aggregation and the distinct cost of Python callbacks](https://docs.pola.rs/user-guide/expressions/aggregation/).
