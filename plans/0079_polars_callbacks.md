# rnx 0079: Polars callbacks, the production contract

Direction from Codex after 0078 closed: "a callback feasibility plan,
centered on useful map/apply operations. Record 0072 proved a route
through SyncFunction; it has not yet established the production
contract. The plan should measure distinct operations reachable and
settle captures, error propagation, threading and cancellation before
implementation." Revised after Codex's plan review (deferred
execution boundary, mutable arguments, re-entry, budget scope).

This record is the plan and its feasibility gates. It binds nothing in
the adapter: it measures the reachable set with a stated classifier,
settles the contract questions with a probe that runs the exact bridge
the generator would emit, and leaves a contract the implementation
record (0080) generates from. Production stays on 0.55.2.

## Problem

116 eligible callables in the API crates take a closure (`impl Fn`,
`FnMut`, `FnOnce`, `&dyn Fn`, a generic bounded by one, or a `Udf`
trait object), 43 in 0072's `callback` bucket and 73 in the generic
bucket because the closure is a function-level generic or the owner is
generic. The parity map projected about 70 of about 100 as reachable
through `SyncFunction`; the measured count below is smaller and
provisional, and the map's figure is superseded by it. Among them are
the operations a script author reaches for first: `Expr::map`,
`Expr::apply`, `Expr::map_many`, `Column::apply_unary_elementwise`,
`DataFrame::apply_columns`, `ChunkedArray::apply_mut`,
`LazyCsvReader::with_schema_modify`.

What 0072 proved (`plans/0072_binding_surface_evidence.md`, samples
s040 to s049): a Rune `Function` converts with `Function::into_sync`
into a `SyncFunction`, which is `Send + Sync`; ten callback samples
executed and matched their Rust oracle; a Rune error inside a fallible
closure surfaced as a Polars `ComputeError`; a runtime scalar capture
was carried; a native (wrapped Polars value) capture was refused with
an error. What 0072 did not establish: the cost of a call, the
behaviour under Polars's parallel invocation, a callback retained in a
plan and invoked by a later `collect`, a callback that calls a binding,
an error inside a closure whose Rust signature cannot carry one, what
a mutable closure argument means, what Ctrl-C and a budget do during a
callback, and what the generator needs to know to emit and verify
these bindings.

## Facts the plan rests on

- `SyncFunction::call` (`rune-0.14.2/src/runtime/function.rs:469,
  538-565, 860-878`): a closure's environment is cloned into an owned
  tuple and the body runs in `Vm::new(context, unit)` with a new
  stack, every call. Nothing is shared between calls; each call pays a
  `Vm` and stack allocation. The `SyncFunction` holds `Arc`s of the
  runtime context and the compiled unit, so a callback stored in a
  plan outlives the script variables that named it and the prompt
  turn that installed it.
- `Function::into_sync` (`function.rs:356, 744-760`) converts every
  captured value to a `ConstValue`: numbers, booleans, strings, and
  tuples, vectors and objects of those. A wrapped Polars value, a
  `Function`, or any other native object fails the conversion with a
  `RuntimeError`, before the Polars call.
- `rune::runtime::budget` (`budget.rs:36-42`, its own words): an
  instruction budget bounds Rune instructions only; "without explicit
  co-operation from native functions" it cannot bound a native call.
  `budget::with(n, f)` sets the current thread's allowance around one
  call and restores the previous one when the wrapper is dropped.
- rnx's engine (`adapters/polars/src/engine.rs`): a routed binding runs
  its Polars call on a fresh scoped thread `rnx-polars-engine` and
  joins it; the join's panic payload is resumed on the caller
  (`engine.rs:48`); a thread that cannot start is an `Err(String)`.
  The hand-written `collect` (`adapters/polars/src/lib.rs`) runs
  through the same engine, returns `Result<DataFrame, String>` and
  formats Polars errors into strings; it is the entry point that
  executes every deferred expression callback today.
- Polars 0.55.2 (`polars-core/src/chunked_array/ops/apply.rs`):
  `apply_mut` (`:193`) on a numeric array mutates the values in place
  through `downcast_iter_mut` with a `Fn(T::Native) -> T::Native +
  Copy` closure; `apply_in_place` (`:183`) consumes `self` and rebuilds
  the chunks; `apply_into_string_amortized` (`:71`) takes `FnMut(T::
  Physical<'a>, &mut String)`, clears one buffer, calls the closure
  and appends the buffer's contents per element, so the closure's
  writes are the result; `StringChunked::apply_mut` (`:311`) and the
  binary form (`:325`) take `FnMut(&'a str) -> &'a str`, a return
  borrowed from the input. `Expr::map_many`, `apply_many`,
  `map_multiple` and `apply_multiple` take `Fn(&mut [Column]) ->
  PolarsResult<Column>` (`polars-plan/src/dsl/mod.rs:554, 593, 1711,
  1739`); the slice is passed mutably so a closure may
  take buffers from it, and nothing reads the slice after the call
  (gate 1 cites the lines). `Schema::retain_mut` passes `&mut Field`
  and keeps the mutated field.
- Interrupts (`src/platform.rs:68-78`): SIGINT sets a flag the
  session's driver polls between slices of the script's own VM; a
  native call in progress is not preempted, and neither is a callback
  running inside one. The CPU-loop interrupt is an open item of rnx
  independent of this record.

## Census: signature candidates and feasible operations

The classifier, to be implemented in the generator as the `callbacks`
census of `surface.json` (gate 1), looks at every closure parameter's
signature (from the parameter type or the generic bound that names
it) and applies, in order: the closure's argument and return types
must be types the existing mapping rules take (a wrapped type by value
or shared reference, a scalar, `&str`, an `Option` of those, a slice
or vector of a wrapped type, unit) or `T::Native`/`T::Physical`
projections the 0076 instantiation engine resolves per family; a
return whose lifetime is tied to an argument is refused; every mutable
closure argument is classified by its read-back contract (below), not
by its pointee; and the callable's other parameters, receiver and
owner must pass the existing applicability and argument rules, so a
closure-type match alone is never "reachable". Applied by a script
equivalent to the rule to the 0.55.2 inventory:

| class | callables | note |
|---|---:|---|
| feasible, concrete owner | 31 | `Expr::map/apply/map_expr/try_map_expr/agg_with_fmt_str/…` (7 take two closures, the second the output-field closure `Fn(&Schema, &Field) -> PolarsResult<Field>`; counted once), `Column::apply_unary_elementwise` and its `try_`/binary forms, `DataFrame::apply_columns(_par)`, `try_apply_columns(_par)`, `LazyCsvReader::with_schema_modify`, `map_parse_options` ×2, `infer_udf_output_dtype` ×2, `PolarsError::wrap_msg`, … |
| feasible, generic owner (`ChunkedArray` family, per instantiation) | 8 | `apply_mut` (numeric), `apply_in_place`, `apply_as_ints` ×2, `apply_to_inner`, `try_apply_fields`, `for_each`, `PolarsContext::with_context` |
| mutable argument with no read-back: `&mut [Column]` | 4 | `Expr::map_many`, `apply_many`, `map_multiple`, `apply_multiple`: passed as a Rune vector of the columns; decision 3 |
| mutable argument that is the result: `&mut String` buffer | 1 | `apply_into_string_amortized`: the Rune callback returns a string the bridge writes into the buffer (result adaptation); decision 3 |
| mutable argument read back: `&mut Field` | 1 | `Schema::retain_mut`: refused, the callback cannot write back a wrapped value in place |
| return borrowed from the argument (`FnMut(&'a str) -> &'a str`, binary likewise) | 2 | `StringChunked::apply_mut`, `BinaryChunked::apply_mut`: refused |
| closure output type is a free generic (`K`, `E`, `Arr`, `C`) | 22 | the Rust type is chosen by the closure; a Rune function has no static return type to choose it |
| arrow-internal argument or result (`T::Array`, `Bitmap`, `ArrayRef`) | 19 | |
| `AmortSeries` argument (not `Send`) | 8 | |
| `Udf` trait objects | 5 | |
| async closures (`FnOnce() -> Fut`) | 4 | the async execution model is a separate decision |
| other (`BooleanChunked`, `Cow<DataType>`, `ExprIR`, `S::Native`, `Result<(), E>`, …) | 11 | |
| total signature candidates | 116 | |

Provisional: 44 feasible under the policy below (31 + 8 + 4 + 1),
before the generator's mapping and applicability checks, which gate 1
applies and which can only lower it. The `ChunkedArray` eight multiply
into bindings per family exactly as 0076's instantiations do and are
counted as eight operations. The four numbers this record reports are
kept apart: signature candidates (116), feasible operations under the
settled policy (gate 1), operations the probe exercises (8), and value
cases a recipe can verify (gate 3, for 0080). No parity percentage is
claimed from a callback-shaped signature.

## Decisions

### 1. Captures: constants only, refused before the call

A Rune closure passed to a binding is converted with `into_sync` inside
the binding, before any Polars work. A closure that captures a wrapped
Polars value, another function or any native object is refused with a
`polars::Error` of kind `CallbackCapture` whose message names the
binding and Rune's own description of the value; nothing is executed.
Constant captures (numbers, strings, booleans, and tuples, vectors and
objects of those) are carried by value: a closure sees the value the
capture had when the binding was called, whenever Polars invokes it.
The script-level rule is stated in the README: "a callback can use
what it is given and what it closes over as plain data; a frame,
series or expression it needs goes in through the API, not through
the closure."

This is the 0072 rule made production. The alternative, a channel back
to the script's VM for native captures, would serialize every call
through the script thread and is not needed by any of the feasible
operations; it stays out of scope.

### 2. Errors: one bridge, one translation boundary that covers deferred execution

Every callback call goes through one bridge function in `support.rs`,
`callback::<Args, Ret>(name, sync_fn, args) -> Result<Ret, CallbackFailure>`,
which converts the arguments to Rune values (wrapped values cloned in,
borrowed arguments cloned, scalars copied), calls the `SyncFunction`
under the budget of decision 4 and the re-entry guard of decision 3,
and converts the result back with `from_value`. `CallbackFailure`
carries the operation name that installed the callback and one of four
causes with Rune's text: the call failed (a VM error or Rune panic),
the result had the wrong type (expected and actual Rune type names),
an argument could not be converted, or the budget was exhausted.

- A closure whose Rust signature returns `PolarsResult<_>` returns the
  failure as `PolarsError::ComputeError` with the text
  `callback <operation>: <cause>`.
- A closure whose Rust signature is infallible (`Fn(&Series) ->
  Series`, `Fn(T::Native) -> T::Native`) has no error channel in
  Polars. The bridge panics with a payload of type `CallbackFailure`.
- The translation boundary is `engine::run`, and coverage of it is a
  generation rule, not a property inferred from the routing heuristics
  or from the probe: **every binding that accepts a closure is routed**
  (its Polars call runs inside `engine::run`) whatever its owner, name
  or types would otherwise decide, because the callback may be invoked
  immediately by that very call (`PolarsError::wrap_msg`,
  `CsvReadOptions::map_parse_options`, `Expr::map_expr` invoke their
  closure before returning, on the calling thread, and none of them
  is routed by today's rules). Deferred invocations reach the same
  boundary through the routed execution entry points, hand-written
  `collect`, `fetch` and kin alike. So every callback invocation,
  immediate or deferred, runs under an `engine::run` join, which is
  also where the re-entry guard of decision 3 is checked. The
  census records, per feasible operation, whether it invokes its
  closure immediately (before returning, on the calling thread:
  `wrap_msg`, `map_parse_options`, `map_expr`, the elementwise and
  column applies) or stores it (`Expr::map` and kin, `with_schema_modify`),
  from a source audit of the pinned crates, and for stored ones the
  execution sinks that can invoke it. The
  join's payload is inspected, a `CallbackFailure` becomes
  `Err(EngineFailure::Callback(text))`, any other payload is resumed as
  today (a Polars-internal panic stays a panic). In 0080 `engine::run`'s
  error type becomes `EngineFailure { NoThread(String), Callback(String) }`;
  generated routed bindings map `Callback` to a `polars::Error` of kind
  `CallbackError` (the binding becomes fallible in Rune, recorded with
  the reason `callback` in the accounting), and the hand-written
  entry points map it into their existing string error with the same
  text. A deferred callback, installed by `Expr::map` in one prompt
  turn and invoked by a `collect` in a later one, therefore fails
  through the `collect`, whose error names the installing operation.
  Retrying the same plan after a failure invokes the callbacks again
  with the same captured constants; nothing in the plan is consumed by
  the failure.
- Deferred invocations from bindings that take no closure (schema
  resolution outside `collect`, `Expr` inspection, `LazyFrame::schema`)
  are the remaining way to reach a stored callback from an unrouted
  binding. Gate 1 enumerates the sinks by a source audit of
  `polars-lazy` and `polars-plan` (where a stored `SpecialEq<Arc<dyn
  ColumnsUdf>>` or `FunctionOutputField` is invoked: plan execution,
  schema resolution, optimizer passes that evaluate literals, and the
  reader's schema hook), maps each to the adapter bindings that reach
  it, and records them in the census as `sinks`; every sink binding is
  routed by 0080 with the route reason `executes callbacks`, and a
  stored-callback operation whose sinks are not all classified is
  refused (`unclassified execution sink`) rather than assumed covered.
  The deferred journey then checks the audit against observed entry
  points, and its runner fails if a `CallbackFailure` reaches a script
  as an unwinding panic or a callback runs through an entry point the
  audit did not list.
- The oracle's Rust closure for these cases fails the same way (an
  `Err` for fallible signatures, a panic with the same payload type for
  infallible ones), so the error controls compare on both sides.

### 3. Mutation and re-entry: read-back contracts, a receiver guarantee, no routed calls from a callback

Mutable closure arguments, per the census: `&mut [Column]` for the
four multi-column operations is passed as a Rune vector of cloned
columns and never read back, which is what Polars does with it (gate 1
cites the call sites); the `&mut String` buffer of
`apply_into_string_amortized` is the result, so the Rune callback
returns a string and the bridge writes it into the buffer; `&mut
Field` in `Schema::retain_mut` is read back and is refused. A gate-1
self-test covers the three shapes so the generator never accepts a
mutable argument because its pointee maps.

`apply_mut` on a numeric array mutates the receiver in place and
Polars settles length and sortedness only after the loop, so a callback
that fails on element n has already written n − 1 values. The receiver
guarantee is commit-on-success: the binding applies the callback to a
clone of the array and swaps it into the receiver only when every
element succeeded; on failure the receiver is unchanged in values,
nulls, length and sorted flags and usable afterwards. Gate 2 measures
the cost of the clone against the bare in-place call and the failure
after several successful elements on an initially sorted receiver.

Re-entry policy: a callback may not call a routed binding, and a
callback may not start while another is running on the thread. The
bridge sets a thread-local guard for the duration of every callback
call (a drop guard, restored on success, error and panic);
`engine::run` checks it before spawning and refuses with
`CallbackFailure` naming the binding; and the bridge itself checks it
on entry, so an invocation that begins under an active guard, which
can only happen through a path the routing rule missed, fails with
`nested callback` before any budget is set or replaced, instead of
silently running with its own allowance. Both refusals surface like
any other callback failure.
Unrouted bindings (plan construction, expressions, dtypes, fields,
schemas, scalars) are allowed inside a callback. Why deny: Polars
invokes callbacks from its rayon workers; a routed binding called
there blocks the worker on a std-thread join whose work may need the
same pool, and one successful nested call on one pool size proves
nothing about arbitrary bindings, sizes or fast paths. Gate 2 still
runs the starvation experiments (below) so the denial is evidenced,
not assumed, and a later record may relax it for demonstrated paths.
What the guard covers: every Rune callback invocation, on whichever
thread Polars uses, since the guard is set on that thread by the
bridge itself; what it does not cover: a Rust thread the callback
cannot create (Rune scripts have no thread API), so nothing.

### 4. Cancellation and budget: no wall-time bound exists; the instruction budget is per invocation and opt-in

A callback runs inside a native Polars call, which rnx does not preempt
today (`rnx run` has no slicing; the session polls its Ctrl-C flag
between slices of the script's own VM). The contract states it: Ctrl-C
during a Polars call that is invoking callbacks takes effect when the
call returns, the same as during any `collect`; no new interrupt
mechanism is introduced here.

`polars::set_callback_budget(instructions)` sets a process-wide value
read by the bridge at each invocation (an atomic, so a callback stored
in a plan and executed after the setting changed uses the new value,
and a change during a parallel evaluation applies to invocations that
start after it). Each invocation gets its own allowance through
`budget::with` around the `SyncFunction` call, restored when the call
returns by any path; it is a per-invocation limit, never a shared
per-operation budget, and it counts Rune instructions only: a native
call inside a callback (an unrouted binding, since routed ones are
denied) runs to completion whatever the budget says, and a blocking
native call cannot be rescued by the budget or by Ctrl-C. Nested
budgets do not arise: a callback cannot execute another callback,
because the operations that would invoke one are routed and denied.
Exhaustion fails the invocation as decision 2 describes, with the text
`callback <operation>: instruction budget <n> exhausted`. The default
is no budget, as for scripts today.

### 5. What the generator emits (for record 0080, fixed here)

For every feasible operation: a binding whose closure parameters are
`rune::runtime::Function` arguments (a Rune closure or a named
function), converted with `into_sync` first; a Rust closure that calls
the bridge; the binding routed through `engine::run` by the rule of
decision 2, its accounting carrying the route reason `callback`; the two-closure operations (`Expr::map` and kin) take both
Rune functions, the output-field closure receiving `(Schema, Field)` as
wrapped values and returning a `Field`. Oracle cases use a recipe table
keyed by closure signature with a Rune closure and the equivalent Rust
closure that produce a non-identity result (`|s| s * 2`,
`|c| Ok(c.cast(Float64))`, `|f| Field::new(f.name(), Float64)`, `|x|
x + 1` for `T::Native`), so a match verifies a value the callback
decided, and the accounting keeps "compiled" and "value-tested" apart
as before. Every feasible operation is reported by name; every refused
one with its reason from the census table.

## Gates

1. **Census.** The `callbacks` census in `surface.json`: every eligible
   callable with a closure parameter, its closure signatures, the
   mutable-argument contract where one applies, and the disposition
   `feasible` / `feasible (vector argument)` / `feasible (result
   buffer)` / a refusal reason from the table above; the classifier
   reuses the mapping rules' argument and return decisions and the
   applicability engine (no second type list), and cites the Polars
   call sites for the four `&mut [Column]` operations. Controls
   (self-test): a closure over wrapped types is feasible; a free
   generic return is refused with the parameter named; an
   arrow-internal argument, an `AmortSeries`, a `Udf` object and an
   async closure are refused with their reasons; a return borrowed
   from the argument is refused; `&mut [Column]`, `&mut String` and
   `&mut Field` get their three contracts; the two-closure operation
   counts once; a `T::Native` closure on a generic owner is feasible
   and resolves per family; a closure that maps on a callable whose
   other argument does not is not feasible; a callable that accepts a
   closure on an owner and name the routing rules leave unrouted
   (`PolarsError::wrap_msg`) is emitted routed with the reason
   `callback`, and the emitted text is checked for the `engine::run`
   call.
2. **Contract probe** (`probes/0079/`, a scratch adapter with a
   hand-written bridge, the same code the generator will emit, and a
   runner that fails closed; every hang-prone control runs in a
   subprocess under a watchdog that kills and reaps the whole process
   group, and a timeout is a failure, never a permissive result). Six
   operations, plus `apply_into_string_amortized` for the buffer
   adaptation and `PolarsError::wrap_msg` for the immediate
   non-data-owner control, eight exercised in all, the final count
   reported from what runs: `Expr::map` (two closures), `Expr::map_many`,
   `Column::apply_unary_elementwise` (infallible signature),
   `DataFrame::apply_columns_par`, `Int64Chunked::apply_mut`
   (`T::Native`, in place), `LazyCsvReader::with_schema_modify`.
   Measured and reported, costs separated into VM-call overhead,
   argument conversion, Polars work and the commit-on-success clone:
   - per-call overhead of the bridge at 1, 1000 and 1 000 000 calls
     against the same closure in Rust; the elementwise case at
     1 000 000 rows against Rust;
   - the deferred journey: install `Expr::map` with both closures at a
     session prompt, return to the prompt, drop and rebind the
     variables that held the functions, collect later; a VM failure, a
     wrong return type and a budget exhaustion each at their invocation
     point (execution closure, output-field closure); the error names
     the installing operation and arrives through `collect`; no
     `CallbackFailure` escapes `engine::run`; a Polars-internal panic
     is still distinguished; the same plan retried after a failure
     runs the callbacks again; every entry point that invoked a
     callback is listed;
   - `apply_columns_par`: the number of distinct threads and the maximum
     simultaneous calls from a counter in the closure;
   - re-entry: a routed call from a callback is refused with the
     binding named, the guard is restored after success, error and
     panic, an unrouted call from a callback succeeds; and the
     starvation experiments as evidence: isolated subprocesses with
     pool sizes 1 and 2, outer workers synchronized into their
     callbacks, inner work forced onto the same pool, a nested
     callback-bearing call, each under the watchdog;
   - mutation: `apply_mut` failing after several successful elements on
     an initially sorted receiver leaves values, nulls, length and
     sorted state unchanged and the receiver usable; the cost of the
     commit-on-success clone; `map_many` receiving a vector; the
     `&mut String` adaptation on `apply_into_string_amortized`;
   - error propagation for a fallible and an infallible signature (a
     Rune error, a Rune panic, a wrong return type, an argument the
     callback mutated), the receiver usable afterwards; the immediate
     unrouted-by-heuristic operation `PolarsError::wrap_msg`, built by
     the probe's bridge with the routing rule applied: a failing and a
     routed-calling callback inside it are translated and refused
     exactly as in the routed cases, an attempt to invoke it from
     inside another callback is refused as `nested callback` by the
     bridge before any budget is replaced, and the same operation
     built without the rule is shown to leak an unwinding panic, which
     is the control the rule exists for;
   - captures: a wrapped value, a function and a nested object holding
     one refused with the binding named, a constant capture carried and
     unchanged by a later rebinding of the variable;
   - Ctrl-C during a long callback in a session takes effect after the
     call, the value intact;
   - budget: pure Rune nontermination stopped at the set budget with the
     specified error; a finite blocking native call (a test-support
     sleep) inside a callback is not stopped; a stored callback executed
     after the setting changed uses the new value; the allowance is
     restored after success, error and panic; the cost of a
     budget-wrapped call against an unwrapped one.
   Controls: the runner fails on an injected wrong result, on an
   unwinding panic that reaches the script, on a refusal that does not
   name the binding, and on a watchdog timeout.
3. **Contract.** The decisions above rewritten with the measured
   numbers and every entry point found in the deferred journey, as the
   "Contract" section of this plan (amended into the impl commit), and
   a one-paragraph statement in the adapter README under "not yet
   bound" so the surface a script sees today is unchanged. Evidence in
   `plans/0079_polars_callbacks_evidence.md`: the census table with
   the generator's numbers, the probe's measurements with costs
   separated, and the four numbers (candidates, feasible, exercised,
   value cases the recipe table can verify) as the bound for 0080,
   with 0080's integration obligations listed: `engine::run`'s error
   type, the hand-written entry points, the routing of every
   callback-invoking entry point, the re-entry guard.

Cost: none to the adapter (nothing is bound); the probe builds in its
own target directory. Commits: this plan, then one impl commit for the
three gates, amended per gate.

## Contract (gate 3, from the census and the probe)

What record 0080 generates from; each clause has its control in the
probe (`probes/0079`) and its numbers in the evidence.

1. **Scope.** The 41 operations the census marks feasible (`surface.json`
   under `callbacks`), 8 of them per-family candidates subject to pair
   applicability. Every one is routed through `engine::run` with the
   route reason `callback`; the stored ones (`Expr::map`, `apply`,
   `map_many`, `apply_many`, `map_with_fmt_str`, `apply_with_fmt_str`,
   `agg_with_fmt_str`, `map_multiple`, `apply_multiple`) are invoked
   only from the classified sinks, which 0080 routes with the reason
   `executes callbacks`: `LazyFrame::collect` and its kin, the
   `describe`/`explain` family, `optimize`, `to_alp*`, `collect_schema`,
   `schema_with_arenas`, `DslPlan::compute_schema`, `describe`,
   `describe_tree_format`, `display`, `to_alp`, and `Expr::to_field`.
   The hand-written `collect` maps `EngineFailure::Callback` into its
   string error with the same text.
2. **Captures.** Constants only, converted with `into_sync` before any
   Polars work; a wrapped value, a function or an object holding either
   is refused as `CallbackCapture` naming the operation and the value's
   Rune type; a constant is carried by value and later rebinding does
   not change it.
3. **Errors.** One bridge; a `CallbackFailure` carries the installing
   operation and one of: `call failed: <Rune's message>`, `wrong result
   type: got <type> (<conversion error>)`, an argument conversion
   failure, `instruction budget <n> exhausted`, `nested callback`. A
   fallible signature returns it as `ComputeError` with the text
   `callback <operation>: <cause>`; through an expression sink Polars
   wraps it in `ExprContext`, the kind a script sees, with the text
   inside. An infallible signature unwinds it to `engine::run`, which
   returns `EngineFailure::Callback`; the binding returns a
   `polars::Error` of kind `CallbackError` with the text, and becomes
   fallible in Rune (accounting reason `callback`). Any other panic
   payload stays a panic. A plan whose callback failed can be collected
   again; the callbacks run again.
4. **Threading.** Polars invokes callbacks on its own threads, in
   parallel; each invocation is its own `Vm` (about a quarter of a
   microsecond per invocation on one thread, the conversion of a small
   series in and out about 4% of that; under parallel
   execution the probe measured throughput, not latency). A callback may call unrouted bindings; it may not call a
   routed binding (refused at `engine::run` with the binding named) and
   no callback may start under another on the same thread (refused by
   the bridge before any budget is set). The denial stands on the
   probe's evidence: with it switched off, pool sizes 1 and 2 deadlock
   when inner work needs the pool, and a nested call that happens not
   to need it completes, so completion proves nothing.
5. **Mutation.** `&mut [Column]` arrives as a vector of clones and is
   not read back (`map_many`, `apply_many`, `map_multiple`,
   `apply_multiple`); the `&mut String` buffer of
   `apply_into_string_amortized` is the string the callback returns;
   `Schema::retain_mut` is refused. `apply_mut` and `apply_in_place`
   apply to a clone and commit on success; on failure the receiver is
   unchanged in values, nulls, length and flags and usable (the clone's
   cost was not separable from noise at a million elements; Polars's
   own in-place form leaves a partial write with a stale sorted flag). A callback that mutates
   what it received mutates a clone.
6. **Cancellation and budget.** No preemption: Ctrl-C during a Polars
   call that is invoking callbacks takes effect when the call returns.
   `polars::set_callback_budget(n)` is process-wide, read at each
   invocation, per invocation, restored on every exit, counts Rune
   instructions only and never bounds a native call or wall time; the
   default is none; its cost is inside the noise of paired samples.
   Exhaustion is Rune's halt matched exactly with the allowance
   confirmed spent; a script's own panic text is never mistaken for it.
7. **Script surface.** A closure parameter is a Rune `Function`
   (closure or named function); the two-closure operations take both;
   `Vec<W>` parameters are borrowed and cloned, never taken (a taken
   value leaves the script's variable empty, as the probe found with
   `select`); `select` is `select_` (a Rune keyword); a callback
   written as a closure whose body ends with a loop must `return` its
   value (Rune 0.14.2 returns unit for the tail expression there).
8. **Oracle.** Cases through a recipe table keyed by closure signature
   with a Rune closure and the equivalent Rust closure producing a
   non-identity result; the error and capture controls of the probe
   become the harness's controls; compiled and value-tested stay apart.

## Out of scope

Binding any callback in the adapter (record 0080, from this
contract); native captures through a channel; async closures; `Udf`
trait objects; arrow-internal closure types; the free-generic-output
closures (a Rune function cannot choose a Rust type parameter);
callbacks that call routed bindings (denied, may be relaxed by a later
record with demonstrated paths); the CPU-loop interrupt; a default
budget for scripts.
