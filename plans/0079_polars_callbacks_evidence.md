# rnx 0079: Polars callbacks, evidence

Plan: `plans/0079_polars_callbacks.md` (cf03e63, revised twice after
Codex's plan review). Production stays on 0.55.2 and nothing is bound:
the generated adapter files are unchanged except `surface.json`, which
gains the `callbacks` census; the probe lives in `probes/0079/` with
its own target directory.

## Gate 1: census

The generator classifies every eligible callable with a closure
parameter (`closure_signature`: the `Fn`/`FnMut`/`FnOnce` kind, the
argument and return types, and whether the bound requires `'static`,
read from the parameter type or the generic bound that names it) by
the rules the plan states, reusing the mapping rules' argument and
return decisions (`World::ret` for what a closure receives, since
Polars hands it Rust values; `World::arg` for what it returns) and
refusing before any of them a `Udf` trait object, an async closure, a
free generic in the closure's types, a return borrowed from the
argument, an amortized borrow (`AmortSeries`), and a mutable argument
without an audited read-back contract in the release file
(`[[callback_mutable]]`, five entries citing
`polars-expr-0.55.2/src/expressions/apply.rs:235,325` for the four
`&mut [Column]` operations, where only the call's result is used
afterwards, and `polars-core-0.55.2/src/chunked_array/ops/apply.rs:71-92`
for the `&mut String` buffer that is the result). The callable's other
parameters, receiver and owner go through the same rules, so a closure
match alone never makes an operation feasible (self-test: a closure
that maps on a callable whose other argument does not is refused with
that parameter named).

Invocation comes from a source audit, not from the bound. The release
file's `[[callback_invocation]]` entries name, per closure parameter
of every feasible operation, whether the pinned source invokes it
before returning (`immediate`, 36 closures) or keeps it in the
expression for a sink to invoke (`stored`, 18 closures on 9
operations: the `function` closures at plan execution, the
`output_type` closures at schema resolution), each with the file and
lines (`Expr::AnonymousFunction { function: new_column_udf(f),
output_type, .. }` at `polars-plan-0.55.2/src/dsl/mod.rs:475,501,526,
554,582,593,1711,1739`; the elementwise and column applies at their
call sites in `polars-core`). The `'static` requirement is recorded per
closure as `static_bound`, a signature fact that in this release
coincides with storage but classifies nothing: a feasible closure
without an audit entry leaves the operation `unresolved: invocation
not audited` (self-test: a `'static` closure the audit does not cover
is unresolved, its bound recorded).

Sinks are classified, not inferred. Every method of `LazyFrame`,
`DslPlan`, `DslBuilder`, `JoinBuilder`, `LazyGroupBy` and `Expr` whose
result is not a plan type (32 in 0.55.2) has a `[[callback_sink]]`
entry naming it `plan execution`, `schema resolution` or `none` with
the citation; a method without one is listed under
`callbacks.sinks.unclassified`, and while that list is non-empty every
stored operation is `unresolved: unclassified execution path` (self-
test: removing the `collect_schema` entry makes the stored operation
unresolved and leaves the immediate one feasible). The audit's call
sites: `ColumnsUdf::call_udf` from plan execution
(`polars-expr-0.55.2/src/expressions/apply.rs:114,133,235,295,325,376,
395`, `polars-plan-0.55.2/src/plans/functions/mod.rs:202`, the
streaming engine's `map`, `columnar_function` and `in_memory_map`
nodes) and `FunctionOutputField::get_field` from schema resolution
(`polars-plan-0.55.2/src/plans/aexpr/schema.rs:287,299,391`,
`plans/ir/unoptimized.rs:48,65`, `plans/schema.rs:21`).

`surface.json` under `callbacks`, 166 rows:

| class | callables |
|---|---:|
| signature candidates in the API crates, eligible | 119 |
| feasible | 41 |
| of which: concrete owner, immediate invocation | 24 |
| of which: concrete owner, stored, invoked from a sink | 9 (`Expr::map`, `apply`, `map_many`, `apply_many`, `map_with_fmt_str`, `apply_with_fmt_str`, `agg_with_fmt_str`, `map_multiple`, `apply_multiple`) |
| of which: `ChunkedArray` family, per instantiation (`apply_mut`, `apply_in_place`, `apply_as_ints` ×2, `apply_to_inner`, `for_each`, `try_apply_fields`, `apply_into_string_amortized` with the result-buffer contract); applicability per pair is 0080's, so these are candidates, not established bindings | 8 |
| of which: with the vector-argument contract (`map_many`, `apply_many`, `map_multiple`, `apply_multiple`) | 4 (within the 9 stored) |
| of which: two closures (execution and output field), counted once | 11 |
| refused | 78 |
| unresolved (invocation not audited, or an unclassified execution path) | 0 |
| not eligible (0072 buckets `unsupported`/`unknown`: `_`-prefixed, `unchecked`) | 16 |
| internal crates, out of scope | 31 |

Refused, by the first reason in the plan's order:

| reason | count |
|---|---:|
| closure type is a free generic (`K`, `E`, `Arr`, `C`, `B`, …) | 37 |
| other (the callable's own parameters or return: `Cow<DataType>`, `ExprIR`, `Self::FuncRet`, `Nullable` indexes, foreign errors) | 14 |
| `AmortSeries` argument | 8 |
| mutable argument without an audited contract (`Schema::retain_mut`, `&mut [S]`, visitor arenas) | 5 |
| `Udf` trait object (`LazyFrame::map`, `with_udf` kin) | 5 |
| arrow-internal types | 4 |
| async closures | 3 |
| return borrowed from the argument (`StringChunked::apply_mut`, binary form) | 2 |

The feasible 41 against the plan's provisional 44: the generator adds
`DataType::map_leaves`, `visit_with`, `try_visit_with`, the binary and
boolean `Column` elementwise forms and `ScalarColumn::map_scalar`,
which the plan's script had not matched, and drops
`PolarsContext::with_context` (generic return) and two `ChunkedArray`
amortized forms the plan's own `AmortSeries` rule covers; the 41 is
the bound for 0080, before the pair-level applicability of the eight
per-family operations, which can only lower it.

Sink bindings (`callbacks.sinks.bindings`): 22 classified as execution
or resolution paths, 10 as `none` (namespaces, literal extraction,
node collection, `set_cached_arena`). Routed by today's rules: every
`LazyFrame` sink, with `collect` the hand-written entry point.
Generated and unrouted today, each a 0080 obligation with the route
reason `executes callbacks`: `Expr::to_field`, `DslPlan::compute_schema`
(Codex's round-1 reproduction: an output-field closure invoked three
times through it), `DslPlan::describe`, `DslPlan::describe_tree_format`;
`DslPlan::display` and `to_alp` are unsupported today. Every
callback-taking binding is routed by the rule `routed_for_callbacks`
(self-test: `PolarsError::wrap_msg` is unrouted by today's rules and
routed by it).

Self-test (`--self-test`, synthetic inventory through the production
census): a closure over wrapped types is feasible and immediate by its
audit entry; an audited two-closure operation is feasible, stored, one
row, each closure with its own sinks; a `'static` closure without an
audit entry is unresolved with the bound recorded; a free generic
return, an arrow-internal argument, an `AmortSeries` argument, a `Udf`
object and a borrowed return are refused with their reasons; `&mut
[Column]` with a contract is a vector argument, `&mut String` with a
contract is a result buffer, `&mut Field` without one is refused; a
`T::Native` closure on `ChunkedArray` is feasible per family; `collect`
is a classified sink and `filter` is not a candidate; removing one
sink classification makes the stored operation unresolved and leaves
the immediate one feasible.

Unchanged and passing: drift, accounting, the generator's other
self-tests.

## Codex gate-1 review round 1 (6ddb244): two corrections

R1: the sink rule had been an owner-and-return heuristic that missed
`DslPlan::compute_schema` (generated, unrouted, invoking stored
output-field closures); it is now the classified table above, with
every candidate method either audited or listed as unclassified, and
the unclassified state propagates to every stored operation. R2:
invocation had been inferred from the `'static` bound; it is now the
per-closure audit table, the bound kept as a recorded fact, and the
self-test no longer asserts that a bound proves storage.

## Gate 2: the contract probe

`probes/0079/probe` is a scratch adapter (its own wrappers for `Series`,
`Column`, `DataFrame`, `LazyFrame`, `Expr`, `Schema`, `Field`,
`Int64Chunked`, `LazyCsvReader`, `PolarsError`) with the bridge the
generator would emit, as the plan's decisions: `install` converts the
Rune `Function` with `into_sync` before any Polars work and refuses a
non-constant capture as `CallbackCapture`; `bridge` is the one path
every invocation takes (guard checked first, then the budget, then the
call, then the typed conversion of the result), returning a
`CallbackFailure` naming the installing operation and the cause; a
fallible Polars signature carries it as `ComputeError`, an infallible
one unwinds it through `unwind`; `engine::run` is the boundary that
translates the payload into `EngineFailure::Callback` and refuses a
routed call under the guard; `apply_mut` commits on success. Eight
operations are exercised: `Expr::map` (two closures), `Expr::map_many`
(vector argument), `Column::apply_unary_elementwise` (infallible),
`DataFrame::apply_columns_par` (parallel), `Int64Chunked::apply_mut`
(in place, `T::Native`), `LazyCsvReader::with_schema_modify`,
`ChunkedArray::apply_into_string_amortized` (result buffer) and
`PolarsError::wrap_msg` (immediate, non-data owner). `run.sh` builds
the probe under `--locked`, runs the in-process contract test
(`tests/contract.rs`, every control asserting, measurements to
`out/contract.json`), then the hang-prone controls as subprocesses in
their own session under a watchdog that kills and reaps the process
group on timeout and reports a survivor as a failure; a timeout is a
result, and the last control makes the watchdog fire on purpose.
`out/report.md` is the run's report.

### Costs (release build; medians with min..max over interleaved samples; the observation counters off)

The instrumentation (call and thread counters, the thread set behind a
mutex) is switched on only for the controls that read it and off for
every timed measurement, so the bridge timed is the bridge proposed.
Variants are interleaved per iteration and every raw sample is kept in
`out/contract.json`; differences are paired.

| measurement | Rune ms | Rust ms | what it is |
|---|---:|---:|---|
| bridge alone, one thread, 100 000 calls, `i64` in and out | 23.94 (23.912..24.155) | | 0.239 µs per call: one `Vm` per invocation, the identity closure, no Polars, no engine thread |
| bridge alone, one thread, 100 000 calls, `Series` in and out | 25.03 (25.012..25.084) | | 0.250 µs per call; the conversion of a three-row series over the scalar: 0.011 µs |
| `apply_mut`, 1 000 000 elements, in place, end to end | 220 (217.197..225.016) | 1.28 | the binding, the engine thread, one invocation per element |
| `apply_mut`, 1 000 000 elements, commit on success | 222 (216.520..238.148) | 0.91 | paired commit minus in place: median 0.3 ms (-3.224..17.220) |
| `apply_mut`, 1 000 elements, in place | 0.389 (0.292..0.562) | 0.0010 | end to end |
| `apply_mut`, 1 element | 0.027 (0.027..0.184) | 0.0001 | the first call: `into_sync`, the engine thread |
| `apply_columns_par`, 1 000 one-row columns | 1.160 (0.942..2.109) | 0.653 | parallel wall time over the pool: throughput, not a per-invocation latency |
| `apply_unary_elementwise`, one call, 1 000 000 rows | 1.379 (1.237..3.265) | 0.888 | one bridge call plus the fixture, the engine thread and Polars's work; end to end |
| budget wrapper, 100 000 calls, paired difference | | | median 3 ns per call (-136..85): inside the noise |

What can be said: a callback invocation costs about a quarter of a
microsecond on one thread (one `Vm` per call), and wrapping a small
series in and out adds about a hundredth of that; a per-element callback
over a million elements costs a quarter of a second end to end, two
hundred times the Rust closure; the commit clone of `apply_mut` is not
separable from the noise at a million elements (paired median near
zero, spread from below zero to seventeen milliseconds); the budget
wrapper's cost is inside the noise of the paired samples. No observer
synchronization is on the timed path: the counters, the thread set and
the restoration probe are each gated by an atomic read before any lock. What cannot be said from these numbers: a per-invocation
latency under parallel execution (the parallel figure is wall time over
the pool), or a separation of the end-to-end series call into fixture,
thread and Polars parts.

### Concurrency

`apply_columns_par` over 64 columns on a pool of 28 threads
(instrumentation on for this control): 64 invocations on 28 distinct
threads, at most 28 at once. Each invocation is its own `Vm`; nothing is
shared.

### The deferred journey

Script A installs `Expr::map` with both closures, rebinds the variables
that held them and returns the expression; script A is dropped; script
B receives the expression, selects it on the fixture frame and
collects: `[2, 4, 6]`, the callbacks invoked (5 invocations, both
closures). The same plan collected again gives the same value and
invokes the callbacks again (retry is ordinary). `compute_schema` on
the plan invokes the output-field closure 3 times (Codex's
reproduction, now a control), `describe_plan` 3 times; the entry
points that invoked callbacks in the journey are `collect`,
`compute_schema` and `describe_plan`, all in the sink table. A VM
failure in the stored execution closure arrives through `collect` as a
Polars error of kind `ExprContext` (Polars wraps an expression's error
with its context; the `ComputeError` and the text `callback Expr::map:
call failed: Panicked: later failure` are inside), a wrong return type
from the output-field closure arrives through `compute_schema` and
through `collect` as `callback Expr::map: wrong result type: got
::std::i64 (Expected type ::polars::Field ...)`, and a budget of 500
set after installation arrives as `callback Expr::map: instruction
budget 500 exhausted`. A Polars-internal panic inside the engine is
still a panic with its own payload, not a callback failure.

### Re-entry

A routed call from a callback (`Series::sum`) is refused at the engine
with `` `Series::sum` is a routed binding and may not be called from a
callback ``; an unrouted call (`plus`, `times`) is allowed; a nested
invocation through the deliberately unrouted `wrap_msg` control is
refused by the bridge itself as `nested callback` before any budget is
replaced; a routed callback-bearing call (`wrap_msg` with the rule) is
refused at the engine. Restoration is checked on the thread that entered
the bridge, not the host, one path at a time: a fresh outer allowance of
50 000 instructions on the calling thread, a nonzero inner callback
budget, the call, the outcome asserted with its payload first
(exhaustion under an inner budget of 100; a value, a VM failure, a
native panic inside the callback with its `native-panic-marker` payload,
and a typed `CallbackFailure` unwind, each under an inner budget of
100 000), then on that same thread the guard is clear and the outer
allowance is the outer one again (a 1 000-iteration loop runs, a
100 000-iteration loop halts: restored, not replaced by unlimited). The
negative control runs the native-panic path with a callback that does
not panic and fails it, as Codex's reproduction required. On the rayon
workers, after 63 observed failing invocations (parallel
failure stops the rest), 64 succeeding invocations each see guard depth
1 inside, run the 3 000-iteration probe loop under no budget after
every invocation (zero probe failures), and return the right values
(the second element of every doubled column summed to 128). The starvation experiments (denial switched off, every pool
worker synchronized into its callback, inner work forced onto the
pool): pool sizes 1 and 2 deadlock and are killed by the watchdog; the
same shape with the denial in force is refused in under a
millisecond; a nested callback-bearing call whose inner operation
(`apply_unary_elementwise` on three rows) submits no pool work
completes, which is the demonstration that one completed nested call
proves nothing about arbitrary bindings, sizes or fast paths.

### Mutation

`apply_mut` on a sorted, nullable receiver with a callback that fails
at the fourth value: the receiver's values, null count, length and
sorted flag are unchanged afterwards and the next `apply_mut` on it
succeeds. The bare in-place form under the same failure leaves `[10,
null, 30, 4, 5]` with the sorted flag still `Ascending`, the partial
write and the stale metadata the commit avoids. The vector argument of
`map_many` and the result buffer of `apply_into_string_amortized`
behave as classified; a callback that mutates the value it received
mutates a clone, the source unchanged.

### Errors, captures, budget

Fallible signature: a VM error and a wrong return type arrive as
Polars errors naming the operation; infallible signature: both arrive
as `CallbackError` naming the operation, the receiver usable
afterwards; `wrap_msg` routed returns the wrapped message and
translates a failure; the two unrouted controls leak an unwinding
`CallbackFailure` into the host (the control the rule exists for).
Captures: a wrapped value, a function and an object holding a wrapped
value are refused as `CallbackCapture` with Rune's description of the
value; a constant capture is carried and a later rebinding of the
variable does not change it. Budget: pure Rune nontermination stops at
the set budget in 0.06 ms; a 300 ms blocking native call inside a
callback completes under a budget of 10 (not stopped, 301 ms); a stored
callback reads the setting at each invocation (unbounded, then
exhausted at 100, then unbounded again). Exhaustion is Rune's halt
matched exactly (`Halted for unexpected reason \`limited\``) with the
allowance confirmed spent inside the budget scope; a user panic whose
message contains the word (`panic("limited user message")`) keeps its
own text, immediate and deferred, while genuine exhaustion is still
reported (Codex's counterexample, now a control). Ctrl-C: a standalone
`SIGINT` handler that only sets a flag, the session's handler in
isolation and not an rnx session; the signal is delivered 0.8 s into a
4.2 s callback loop (`interrupted_at_ms` recorded by the handler is
after the call's start and before its end), the call completes with its
value intact (`completed len=20000 first=1` asserted) and the flag is
seen afterwards.

`LazyCsvReader::with_schema_modify`, in the asserted replay: the
callback's schema (every column `f64`) is the reader's schema after
`finish` and after `collect`, invoked once, before `finish`; without it
the inferred `i64` stands.

The watchdog tracks the whole session of each control through the
leader's pid, not the leader alone: after any exit the group's members
are listed and killed and a survivor fails the control; the in-process
contract test runs under the same watchdog (it contains deliberately
infinite callbacks); the two controls of the controls are a leader that
must time out (reported as a deadlock) and a leader that exits normally
leaving a child in its group (Codex's reproduction, now the
`survivor_control`, which the driver must catch).

## Codex gate-2 review round 1 (94948cf): four corrections

R1: exhaustion had been inferred from the substring `limited`; it is
now Rune's exact halt text with the allowance confirmed spent, and the
user-panic counterexample is a control. R2: the watchdog had checked
the leader's pid only; it now tracks the session, kills and verifies
after normal exit and timeout, fails on survivors, covers the in-process
test, and has the leader-exits-child-survives negative control. R3: the
CSV operation is in the asserted replay; restoration is checked on the
thread that entered the bridge (outer allowance, worker reuse) rather
than the host; the Ctrl-C control asserts the value and the signal's
timing and is labelled a standalone-handler experiment. R4: the
instrumentation is off for timing, samples are interleaved and kept
raw, the bridge's latency is measured alone on one thread, and the
parallel figure is reported as throughput. A Rune 0.14.2 fact surfaced
by the restoration probe: a closure whose body ends with a loop and a
tail expression returns unit (a named function or an explicit `return`
returns the value).

## Codex gate-2 review round 2 (f4f5810): two corrections

R3: the calling-thread restoration test had recorded its outcomes
without asserting them and had zeroed the inner budget after the
exhaustion path; each path now asserts its outcome and payload first
under a nonzero inner budget and a fresh outer allowance, the native
panic's omission is a negative control that fails, and the worker check
reports observed counts. R4: the restoration probe's mutex had still
been taken on every bridge call; an atomic flag is read first, so a
timed call touches no observer synchronization; the measurements were
rerun and the numbers above are from that run.

## Gate 3: the contract

Written into the plan's "Contract" section; the census numbers above
and the probe's measurements are its inputs. The four numbers: 119
signature candidates, 41 feasible under the settled policy (8 of them
per-family candidates pending pair applicability), 8 exercised by the
probe, and the value cases 0080's recipe table can verify are the
feasible operations whose closure signatures the probe's recipes
cover (every feasible signature shape has one: scalar in and out,
series or column in and out, schema and field to field, columns to
column, schema to schema, string to string).
