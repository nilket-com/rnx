# rnx 0080: Polars callback bindings

Status: revised after Codex's plan review (reviews/0080_review_codex.md:
re-entry propagation through infallible routed bindings, callback-safe
recipes, audit eligibility gating emission), ready for impl. Record 0079 (cf03e63, baa71fc)
settled the callback contract without binding anything; this record
generates the bindings from that contract, in the adapter, and reports
newly available operations and newly value-tested operations apart.
The 0079 census's 41 feasible operations are the upper bound: family
applicability (8 of the 41 are `ChunkedArray` candidates) and
compilation, the second check, can only lower it.

## Problem

The adapter has no callback binding. `surface.json` under `callbacks`
lists 41 feasible operations (24 concrete owners invoking the closure
before returning, 9 `Expr` and free-function operations that store the
closure in the plan, 8 `ChunkedArray` operations per family), with the
mutable-argument contracts, the invocation audit and the sink table.
The contract (`plans/0079_polars_callbacks.md`, "Contract", eight
clauses) names what 0080 must build in the adapter before any binding
is emitted: a typed engine failure, the hand-written entry points
mapping it, the routing of every closure-taking binding and every
classified sink, the re-entry guard, commit-on-success for the in-place
applies, a recipe table keyed by closure signature, vectors borrowed
rather than taken, and the `select_` rename. Bindings that skipped any
of these would leave a script with unwinding panics, unbounded
re-entry or a partially written receiver, the outcomes the probe showed.

## Accounting before the rule

| class (0079 census) | operations | route in the generator |
|---|---:|---|
| concrete owner, immediate invocation (`Column::apply_*`, `DataFrame::apply_columns*`, `DataType` visitors, `ScalarColumn::map_scalar`, `PolarsError::wrap_msg`, `map_parse_options` ×2, `with_schema_modify`, `Expr::map_expr`/`try_map_expr`, `infer_udf_output_dtype` ×2) | 24 | `emit_callable` (inherent and free) with a closure parameter rule |
| stored under `'static` (`Expr::map`, `apply`, `map_many`, `apply_many`, `map_with_fmt_str`, `apply_with_fmt_str`, `agg_with_fmt_str`, `map_multiple`, `apply_multiple`) | 9 | the same rule; invoked from the routed sinks |
| `ChunkedArray` per family (`apply_mut`, `apply_in_place`, `apply_as_ints` ×2, `apply_to_inner`, `for_each`, `try_apply_fields`, `apply_into_string_amortized`) | 8 | `emit_instantiations`, the closure's `T::Native`/`T::Physical` resolved per pair by the applicability engine |
| upper bound | 41 | |

Two closures on 11 of them (execution and output field); the output
field closure receives `(Schema, Field)` or `(Schema, Vec<Field>)`.
Per family, the eight `ChunkedArray` operations multiply into bindings
(one per proven pair) but count as eight operations, as in 0076.

The 0079 measurements bound the cost: about 0.24 µs per invocation on
one thread, plus Polars's own work; the launch budget below is for the
bindings' registration, not their invocation.

## Decisions

### 1. The bridge and the boundary, in the adapter

`support.rs` gains the probe's bridge verbatim in shape:
`callback::install(op, Function) -> Result<Arc<SyncFunction>, Error>`
(kind `CallbackCapture`, the operation and Rune's description of the
value in the message) and
`callback::bridge<A, R>(op, &SyncFunction, args) -> Result<R, CallbackFailure>`,
which checks the thread-local guard before anything else (`nested
callback`), sets the guard for the call, reads the process-wide budget
at each invocation and applies it with `budget::with`, calls, and
converts the result; exhaustion is Rune's exact halt text with the
allowance confirmed spent; every other error keeps its text; a wrong
result type names the expected and actual Rune types.
`CallbackFailure { op, cause }` renders as `callback <op>: <cause>`.
`support::unwind(e)` resumes it as a panic payload for infallible
Polars signatures.

`engine.rs`: `run` returns `Result<T, EngineFailure>` with
`NoThread(String)`, `Callback(String)` (the joined payload was a
`CallbackFailure`; any other payload is resumed as today) and
`Reentry(String)` (the guard was set on the calling thread: a routed
binding called from a callback, named). Generated routed bindings map
`Callback` and `Reentry` to a `polars::Error` of kind `CallbackError`
with the text and `NoThread` to the panic they raise today; the
hand-written `collect`, `read_csv`, `write_parquet` and kin map all
three into their string error with the same text. A binding whose
Polars signature is infallible but whose closure is not therefore
returns `Result` in Rune, recorded with `fallible: true` and the reason
`callback` in the accounting.

`polars::set_callback_budget(n)` is a free function in the module;
0 (the default) is no budget.

Re-entry through every routed binding, existing and new. A routed
binding called from a callback gets `EngineFailure::Reentry` from
`engine::run`. A fallible binding (a `Result` in Rune today, or any of
the newly routed sinks: `to_field`, `compute_schema`, `describe`,
`describe_tree_format` all return `Result`) returns a `polars::Error`
of kind `CallbackError` naming the refused binding. An infallible
binding (`Column::reverse`, `DataFrame::reverse` and the other
direct-return wrappers, whose signatures do not change) resumes a
typed `CallbackFailure { op: <the refused binding>, cause: "routed
binding called from a callback" }` through `support::unwind`, never a
string panic: the payload unwinds through the callback's `SyncFunction`
frame, through the bridge (whose guard is a drop guard), through
Polars and rayon (which resume the original payload), to the outer
`engine::run`, which translates it into `EngineFailure::Callback`, so
the outer binding returns `CallbackError` with the text and the
receiver is usable. `NoThread` keeps its string panic in infallible
bindings and its `EngineError` in fallible ones, as today. The
accounting records per routed binding how it propagates a refusal
(`reentry: error` or `reentry: unwind`); no existing signature or
protocol changes. The README states the rule a script sees: a routed
method called inside a callback fails that callback with the refusal
text; outside a callback nothing changes.

The shared types (`CallbackFailure`, `EngineFailure`, the guard) live
in `engine.rs`, which is unconditional, and the bridge in a module the
`generated` feature gates with the bindings; the hand-written adapter
builds with `--no-default-features` and maps the typed failure the
same way.

### 2. What the generator emits

A closure parameter (recognized by the 0079 census's
`closure_signature`, the same rule, so the census and the emission
cannot disagree) becomes a `rune::runtime::Function` parameter of the
binding. The binding's prologue installs every closure before any
other work (`let __f0 = support::callback::install("<op>", f0)?;`), so
a refused capture never reaches Polars. The Rust closure handed to
Polars captures the `Arc<SyncFunction>` and calls the bridge with the
arguments converted as returns are converted today (`World::ret`: a
wrapped value cloned into its wrapper, a scalar copied, a shared slice
or vector of wrapped values as a vector of clones) and the result
converted as arguments are converted today (`World::arg`: a wrapper
cloned out, a scalar narrowed where the Rust type is narrower than
Rune's). A fallible Polars closure signature maps a `CallbackFailure`
to `PolarsError::ComputeError` with its text; an infallible one calls
`support::unwind`.

The mutable contracts from the release file's `[[callback_mutable]]`
entries: `vector` passes a vector of clones and reads nothing back;
`result buffer` passes nothing for the buffer, takes the callback's
`String` and writes it into the buffer. `apply_mut` and
`apply_in_place` on a `ChunkedArray` family apply the callback to a
clone inside the routed call and assign it to the receiver only when
the call returned; on failure the receiver is untouched.

Routing: every binding with a closure parameter is routed
(`routed_for_callbacks`, already in the generator), reason `callback`;
every callable the sink table classifies as `plan execution` or
`schema resolution` is routed, reason `executes callbacks`, whatever
today's rules say (`Expr::to_field`, `DslPlan::compute_schema`,
`describe`, `describe_tree_format` change from unrouted to routed;
`display` and `to_alp` stay unsupported for their own reasons). The
accounting records the route reason per binding.

Per family: `emit_instantiations` substitutes the pair's `T::Native`
and `T::Physical` into the closure signature before the rule runs, so
`Fn(T::Native) -> T::Native` on `Int64Chunked` is `Fn(i64) -> i64`
(Rune `int` in and out) and on `Float32Chunked` is `Fn(f32) -> f32`
(Rune `float`, the result narrowed with the existing cast rule, a
failure a `CallbackFailure`); a pair whose substituted signature the
rule refuses is a route exception with the reason, as 0076 does.

`select` is bound as `select_` where the generator meets it (it is a
Rune keyword; the rename joins `default_` and `not_`).

Audit eligibility gates emission, not only the census. The emitter
consults the same release tables the census reads and refuses, with
the census's own reason, any closure-taking callable that lacks a
`[[callback_invocation]]` entry for every closure parameter, any
stored operation while a plan-holding method is unclassified in
`[[callback_sink]]` or its sink group has no classified member, and
any mutable closure argument without a `[[callback_mutable]]` entry;
mapping and pair applicability apply on top. The census rows gain
`disposition after emission`, and the accounting test asserts that no
`unresolved` or `refused` row acquired a binding. Self-tests exercise
this through the emitter, not the census: with an invocation entry
removed the operation is not emitted, with a required sink entry
removed no stored operation is emitted while an independently audited
immediate one still is. The 0.54.4 and rc2 release files carry no
audit tables in this record, so on those runs every callback operation
is unresolved and unbound, a result the adjacent and rc2 reports state.

Vectors are borrowed, container and elements. Rune 0.14.2's
`FromValue` for `std::vec::Vec<T>` takes the script's vector object
before the binding runs (`runtime/vec.rs:533`), whatever `T` is, so a
`Vec<rune::Value>` parameter consumes the caller's vector even when its
elements are then borrowed (Codex reproduced it through the 0079
probe's `select_`: the named vector is unreadable afterwards). The
rule for every vector parameter, closure argument or binding, is the
hand-written `expressions` helper's shape: the parameter is a
`rune::Value`, the binding borrows the Rune vector through it
(`borrow_ref::<rune::runtime::Vec>()`), and clones each element out,
so the caller's container and its elements are intact after the call,
on success and on a capture or type refusal. Today's generator emits
68 bindings with `Vec<…>` parameters (`Vec<rune::Value>` and vectors of
scalars alike), which consume the caller's vector the same way; gate 2
moves them to the borrowing rule as a correction of an existing
defect, reported apart from the callback counts, with a control on an
existing binding (`Float64Chunked::from_vec`: the named vector and its
elements readable afterwards) beside the closure-argument control.

### 3. Sinks and the hand-written entry points

The sink table's routed bindings are the only way a stored callback
runs; the deferred journey of 0079 (`collect`, `compute_schema`,
`describe_plan`) becomes a harness control through the adapter's own
bindings, with a callback installed in one script unit and executed
from another.

### 4. Oracle: callback-safe recipes by closure signature

A recipe runs inside a callback, so it may use only bindings a callback
may call: unrouted ones. Today every method of `Series`, `Column` and
`DataFrame` is routed by the owner rule, whatever it does, and no
arithmetic protocol exists on them, so a transformation of a series or
column has no callback-safe binding at all. This record names an
audited, explicit set rather than un-routing anything silently: the
release file's `[[callback_safe]]` entries list bindings on data types
that do no data work (metadata only: `Series::with_name`,
`Column::with_name`, `Column::rename`, `Series::name`, `Series::len`,
`Column::len`, `Series::dtype`, `Series::null_count`, and their kin),
each with the citation that the implementation touches no values and
no pool; the generator emits them unrouted with the route reason
`callback-safe (audited)`. They stay usable outside callbacks
unchanged. Everything else on a data type stays routed and is refused
inside a callback, as decision 1 says.

`[[callback_recipe]]` entries are keyed by the closure signature as the
census spells it and give a Rune closure and the equivalent Rust
closure producing a non-identity result through callback-safe
bindings only: `Fn(&Series) -> Series` and `Fn(Column) ->
PolarsResult<Column>` rename (`|s| s.with_name("cb")`; the structural
comparator includes the name); `Fn(&Schema, &Field) ->
PolarsResult<Field>` and the `&[Field]` form change the dtype
(`|s, f| f.with_dtype(polars::DataType::Float64())`, `Field` and
`DataType` are unrouted); `Fn(&mut [Column]) -> PolarsResult<Column>`
renames the first column; `Fn(T::Native) -> T::Native` adds one;
`FnOnce(&str) -> String` and the result buffer concatenate;
`FnMut(DataType) -> DataType` returns a constructed dtype;
`Fn(Schema) -> PolarsResult<Schema>` and any signature without a
callback-safe non-identity form leave the binding compiled and its
case unverified with the reason `no callback-safe recipe for the
closure signature`, counted apart, never a match. Recipe eligibility
is checked by the generator against the actual bindings' routing
before a case is emitted; a recipe naming a routed binding is a
generator error, not a case. Controls: a negative recipe through a
routed method (`|c| c.reverse()`) is refused at the engine and the
case reports the refusal, not a match; a positive recipe through the
shipped `with_name` matches its Rust closure. The runner controls
ported from the probe: a VM failure, a wrong return type and a budget
exhaustion in a stored callback arriving through `collect` and
`compute_schema`; the same in an immediate infallible operation
arriving as `CallbackError` with the receiver usable; the capture
refusals; a routed call from a callback refused with the binding named
through an existing infallible method (`Column::reverse`) and through
a newly routed sink (`compute_schema`), the outer binding returning
`CallbackError`, the receiver usable, no panic leaking; `apply_mut`
failing after several elements on a sorted nullable receiver leaving
it unchanged; the user-panic-mentioning-`limited` counterexample; the
constant capture carried.

### 5. Reporting

| number | how it is measured |
|---|---|
| newly available operations | callables whose status becomes `generated` with at least one binding that has a closure parameter, by class of the accounting table; the per-family eight count once each when any pair is emitted |
| newly value-tested operations | those with at least one oracle case that matched, by class |
| refused | pairs and callables refused at emission or compilation, by reason, reconciled against the 0079 census (`feasible` rows that did not become bindings are listed by name) |
| cost | cold launch by the existing interleaved method (`probes/0073/launch.py`, three 60-run measurements, raw samples kept) against the 0079 binary; budget +3 ms for the registrations (0076 spent +2.2 ms for 650 bindings; this record adds at most a few dozen plus the per-family pairs); the probe's invocation timings were observations on its workload, not bounds for every callback shape |

The scoreboard's three numbers move only by these; no percentage is
claimed from a closure-shaped signature.

## Gates

1. **Boundary and bridge.** `engine::run`'s typed failure and the
   hand-written entry points' mapping; `support::callback` with unit
   controls ported from the probe: exact-halt exhaustion with the
   user-panic counterexample, nested refusal before any budget,
   restoration of guard and outer allowance on the calling thread for
   success, VM failure, native panic and typed unwind under a nonzero
   inner budget, the routed-from-callback refusal at `engine::run`
   returning as an error from a fallible binding and as the typed
   unwind from an infallible one (`Column::reverse` inside a callback:
   the outer binding's `CallbackError` names `Column::reverse`, the
   receiver is usable, no panic reaches the script); the adapter builds
   and its suite passes with and without the `generated` feature.
2. **Emission.** The closure rule in `World::arg`-adjacent code, the
   prologue, the routing reasons, the mutable contracts, commit on
   success, the per-family substitution, the `select_` rename;
   compilation as the second check; the `callbacks` census rows gain
   `disposition after emission` (`generated`, `refused: <reason>`,
   `pair refused: <reason>`); self-tests from a synthetic inventory for
   each shape (a closure over wrapped types, two closures, a vector
   contract, a result buffer, a per-family projection, an operation on
   an unrouted owner emitted routed, the audited callback-safe set
   emitted unrouted) and for the audit gate (an invocation entry
   removed: not emitted; a sink entry removed: no stored operation
   emitted, the immediate one still emitted); the borrowed-vector
   controls (a closure argument and an existing vector-taking binding:
   the caller's container and elements readable after success and
   after a refusal), and the 68 existing vector parameters moved to
   the borrowing rule, reported apart.
3. **Oracle.** The callback-safe set and the recipe table, the recipe
   eligibility check, the cases, the ported controls including the
   negative recipe, drift and accounting extended to the new route
   reasons and the post-emission census assertion; the deferred journey
   through the adapter's own `collect` and `compute_schema`.
4. **Evidence.** `plans/0080_polars_callback_bindings_evidence.md`:
   the two numbers by class, refusals by reason reconciled to the
   census, scoreboard before and after, launch three times against the
   0079 binary, adjacent 0.54.4, rc2 experimental (the release files
   for 0.54.4 and rc2 gain their own audit tables or the census marks
   their operations unresolved, which is a result, not a failure),
   README (adapter and generator).

Commits: this plan, then one impl commit for the four gates, amended
per gate; push at close. The 0079 wording correction Codex asked for
(the series conversion is about 4% of a scalar invocation, not a
hundredth) is folded into this plan's commit.

## Out of scope

Relaxing the re-entry denial (the callback-safe set is an audited
list of metadata operations, not a relaxation); native captures
through a channel; the
refused census classes (free generic closure types, arrow-internal
types, `AmortSeries`, `Udf` objects, async closures, borrowed returns,
`Schema::retain_mut`); a default budget; the CPU-loop interrupt; new
dtype families for the per-family operations.
