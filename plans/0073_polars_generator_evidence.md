# rnx 0073: Polars binding generator, evidence

Plan: `plans/0073_polars_generator.md` (ff122eb). Gates are appended as
they close.

## Gate 1: registration cost

`probes/0073/regcost`: a Rune 0.14.2 context with the default modules
plus one wrapper type and N associated stub functions; median of 15,
release build, this machine (nano).

| functions | module build µs | context install µs | runtime µs | compile+run µs | total µs |
|---:|---:|---:|---:|---:|---:|
| 0 | 1 | 2138 | 159 | 44 | 2342 |
| 100 | 18 | 2218 | 162 | 45 | 2443 |
| 1000 | 131 | 2833 | 199 | 45 | 3208 |
| 3000 | 374 | 4440 | 212 | 46 | 5072 |
| 6000 | 750 | 7264 | 470 | 48 | 8532 |

Registration is linear at about 0.9 µs per function, all of it in module
build and context install; script compile and run do not change. The
stage-one surface is about 3000 entries plus a few hundred wrapper types,
so the expected cost is 3 ms on top of the 2.3 ms the default context
already costs, against the 12–14 ms `:dep polars` launch measured in
records 0065 and 0067.

**Decision.** Eager registration, with a budget of 4 ms added context
build at the real generated module, measured again at gate 4 with the
wrapper types and catalogue strings in place. Rune builds its context
once, so lazy registration would mean a second context and a session
restart; that is the fallback only if the real module exceeds the budget.

## Gate 2: mechanical bucket generated

`tools/polars-gen` reads `probes/0072/out/0.55.2-adapter/result/inventory.json`
(the locked 0.55.2 documentation set, re-extracted with two additions the
generator needs: canonical type renderings that preserve identity, and each
item's public path inside its own crate) and writes
`adapters/polars/src/generated/` plus `adapters/polars/surface.json`. Hand-
written bindings are untouched; the four hand-written wrapper types are
reused so generated methods land on the same `DataFrame`, `LazyFrame`,
`LazyGroupBy` and `Expr` values scripts already hold. The adapter builds
on stable with no errors; the generated code carries the Polars rustdoc
first paragraph on every binding.

Accounting, every eligible callable of the 0072 inventory (all buckets):

| status | entries |
|---|---:|
| generated | 1032 |
| adapted (hand-written wins, or implied by another protocol) | 303 |
| unsupported, with reason | 3010 |
| out of scope (internals reachable through the prelude) | 710 |

The mechanical bucket in the API crates, which is this gate's scope:

| status | entries |
|---|---:|
| generated | 1032 |
| adapted | 303 |
| unsupported | 495 |

Adapted: implied by the PartialEq or Clone protocol (300), hand-written binding of the same name (2), hand-written protocol (1).
Unsupported, by reason: trait impl not mapped to a Rune protocol (242), unwrapped type (75), owner not wrapped (59), no wrapped implementor (40), lifetime owner (31), owner has no public path (12), receiver consumes a non-Clone type (10), async (10), on every implementor (7), trait has no public path (4), static borrow (1), foreign type (1), mutable reference (1), generic type (1), no public path (1).
The other buckets are `unsupported: bucket` until gate 3 and are counted
in the first table. Wrapper types emitted: 230, only those a
generated binding mentions.

Coverage number 1, compiled: 1032 mechanical bindings, all in the
committed module that builds. Number 3, predicted by 0072 for this bucket
in the API crates: 1830. Number 2, executed, is gate 4.

Tests, `adapters/polars`: `generated_files_do_not_drift` regenerates with
`--locked` and compares byte for byte; `every_callable_is_accounted_for`
checks each eligible callable has exactly one status with a Rune path or
a reason; `generated_bindings_run` executes an associated constructor, two
instance methods and an error surfaced as `polars::Error` with `kind()`;
the presentation suite passes unchanged. The engine unit test
`panic_is_joined_before_resuming_on_the_caller` failed once under the full
parallel run and passed three times in isolation: the flake noted in 0068,
not touched here.

Registration cost of the real module (release, this machine):

| measurement | µs |
|---|---:|
| empty module, default context | 2306 |
| generated types only | 2442 |
| full `build()`, best of 7 | 3361 |
| full `build()`, single cold call in the smoke test | 7000–10000 |

Warm, the whole module adds about 1.1 ms, under the gate 1 budget. The
single cold call is several times that and varies between runs; whether
that reaches a cold `rnx` launch is gate 4's measurement, made the way
records 0065 and 0067 measured launches, before the budget is called met.

## Gate 3: conversion and option-struct buckets, errors, routing, docs

Second version, after Codex's implementation review of ccafd37.

The generator emits all three buckets; the adapter builds on stable with
no errors. Accounting, all eligible callables of the API crates (out of
scope internals excluded):

| bucket | generated | adapted | unsupported |
|---|---:|---:|---:|
| mechanical | 1032 | 303 | 495 |
| conversion | 363 | 2 | 353 |
| option struct | 227 | 4 | 242 |
| total | 1622 | 309 | 2414 |

Generated entries by kind: inherent 878, foreign_trait_impl 605, free_fn 73, trait_method 66.
A trait method counts once and is emitted once per wrapped implementor;
`surface.json` lists every Rune path an entry produced. Every entry
carries its canonical signature so a release diff sees reshapes.

Policies as implemented, and where each is proven:

- Ownership: `self` clones (refused where the type is not Clone), `&mut
  self` mutates the Rune value in place and the oracle compares the
  receiver after the call as well as the return, `&mut Self` chains return
  unit, borrowed returns are cloned. Proven by the oracle cases below.
- Conversions: narrowing integers make a binding fallible with an error
  naming the parameter; borrows inside `Option` are hoisted into owned
  temporaries; vectors of borrows are refused.
- Option structs and enums: `default_()`, `with_<field>` setters and
  `<field>` getters where a method does not already own the name; one
  constructor per variant with a bound payload.
- Errors: `PolarsResult<T>` is a Rune `Result` with a `polars::Error`
  carrying `kind()` and `message()`; the oracle compares kinds, and the
  derivation is the same function on both sides (a control checks it).
- Execution: routing is by identity, not by name alone. Any binding whose
  owner, parameter or return is a frame, series, column, scalar column,
  group-by, lazy frame or lazy group-by runs on the engine thread, plus an
  explicit prefix list (collect, fetch, sink, scan, read, write, execute,
  concat, sort, rechunk); 352 generated entries are routed and say so
  in `surface.json`. The one type that is not `Send`, `AmortSeries`, is
  listed and not routed. Argument conversions run before the closure. A
  routed binding keeps its Rust fallibility: an engine thread that cannot
  start is a panic with a clear message. `tests/routing.rs` proves, in its
  own process because the counters are global, that an eager
  `DataFrame::sort_impl` and a frame accessor each start exactly one
  engine thread, and that a frame call made from inside a tokio runtime
  runs on an engine thread that sees no runtime context.
- Names that are Rune keywords get a trailing underscore (`select_`,
  `default_`), noted per entry.
- Documentation: one catalogue entry per generated binding. The adapter
  README now describes the shipping surface, the feature and these
  policies.

## Gate 4: executed coverage, adjacent release, cost

Fourth version, after Codex's third implementation review of 8314cdf.

### Execution dispositions and coverage number 2

Every generated entry has exactly one execution disposition, and the
accounting test asserts the dispositions partition the generated set and
that `case` equals the number of oracle cases emitted:

| disposition | entries |
|---|---:|
| case | 832 |
| no fixture for the receiver type | 520 |
| protocol Clone has no script-level trigger | 100 |
| protocol Debug has no script-level trigger | 91 |
| return type has no comparison | 35 |
| no fixture for a parameter | 31 |
| receiver type has no comparison | 7 |
| operator result type not wrapped | 4 |
| excluded: nondeterministic oracle | 2 |
| **generated** | **1622** |

Two oracles are excluded by identity, with the reason in `surface.json`:
`Series::as_single_ptr` returns a memory address and
`ConcurrencyController::new` shows runtime state in its Debug output.
Exclusions are explicit and per case; the runner never tolerates an
oracle that gives two different values.

The oracle test (`tests/generated_oracle.rs`, generated) calls each case's
binding in a Rune script on fixture values and the Polars function in
Rust on the same values. Comparison goes through `src/oracle.rs`,
hand-written: a frame is its dimensions and columns with dtypes plus one
structured entry per row with every cell, never a delimited text; series
and columns cell by cell with nulls; lazy frames and expressions
collected first; errors by kind; panics by message. Every case carries an
order policy: exact ordered values by default, rows as a set only for
`unique` and `unique_generic`, whose Rust contract leaves row order
unspecified; the same policy applies to the second call on the same
receiver. Mutating methods compare the receiver after the call together
with the return, including an error return as `ret=ERR:kind`. Protocol
impls are cases too. `classify` judges the binding side first, so a
script that failed to compile fails whatever the oracle did; then the
oracle must agree with itself before any outcome is approved, so a
matching first Rust run never hides a second run that changed value,
errored or panicked; then a VM error, a binding panic, a wrong value, a
wrong second call, or a successful binding against a panicking oracle
all fail.

Controls, all committed: unit controls on `classify` in `oracle.rs`, and
integrated controls in the generated runner, `oracle_runner_fails_closed`,
which push injected cases through the same code path as the real ones and
require a failing outcome: the unreversed frame against Rust's reverse
(the review's counterexample), the same under an unordered policy when a
cell differs, a wrong second call, a second call in another row order
under the default policy, a compile error against an alternating oracle,
a script panic against a Rust panic, a Polars panic against a Rust panic
with another message, a value against a panicking oracle, a wrong
value against an alternating oracle, and a matching first run whose
second run changes value, errors, or panics.

| outcome | cases |
|---|---:|
| structural match | 801 |
| same rows, different order, under the unordered policy | 2 |
| both error, same kind | 19 |
| both panic, same message | 10 |
| any failing outcome | 0 |
| **cases** | **832** |

The three coverage numbers for the API crates, side by side and never
added:

| number | value |
|---|---:|
| predicted by 0072 rules for the three buckets | 3021 |
| compiled: generated entries in the committed module | 1622 |
| executed against a structural Rust oracle with an approved outcome | 832 |
| of which ordered value matches | 801 |

The plan's configuration boundary carries into the accounting: the API
crates under the adapter's feature set; internals reachable through the
prelude are `out_of_scope` (710 entries).

### Adjacent release

`probes/0073/adjacent.sh` generates from the 0.54.4 inventory into a
scratch adapter (sources only, never a target directory) pinned to 0.54.4,
builds the generator and type-checks the adapter with `--locked` under the
committed `probes/0073/adjacent.lock`, fails if the lock changes or the
check fails, and diffs the accounting by path, signature and status.
`adjacent-control.sh` injects a failing `cargo check` and requires the
script to fail; it does. A second replay left the lock unchanged.

| measure | count |
|---|---:|
| `cargo check` of the 0.54.4 adapter, locked | ok |
| entries only in 0.55.2 | 181 |
| entries only in 0.54.4 | 42 |
| same path, signature changed | 22 |
| same path, status changed | 1 |
| same path, same signature and status | 4569 |
| 0.54.4 generated / adapted / unsupported | 1567 / 298 / 2354 |
| generated lines that differ (functions, types, catalogue) | 487, 73, 93 |

One observed upgrade; not a bound.

### Cost: measured, attributed, decided

`probes/0073/launch.py` launches binaries interleaved run by run on an
empty script, requires every launch to exit zero with no output (a
failing launch fails the driver, and `launch-control.sh` proves it with
`/bin/false`), and keeps every sample in `probes/0073/launch-results.json`.
Three binaries: the adapter before this record (4a412e9), the adapter
built with `--no-default-features` (hand-written only), and the default
build.

| binary | median ms | min ms | p90 ms | size |
|---|---:|---:|---:|---:|
| before (4a412e9) | 10.85 | 5.94 | 15.45 | 107.5 MB |
| generated off | 11.1 | 5.67 | 15.08 | 107.5 MB |
| generated on | 20.06 | 10.55 | 25.93 | 125.9 MB |

Host variance moves absolute numbers between sessions (an earlier run
gave 6.3 and 14.7 ms for before and on); the difference of 7 to 9 ms is
what reproduces, here and in Codex's interleaved runs. Clean release
build: 106 s before, 192 s with the generated module.

Attribution, measured with `perf` on the default build: the extra time is
inside the adapter process in Rune's `Context::install` (item-tree
insertion, hashing, allocation for about 1900 functions and 237 types),
not the dynamic loader (about 10% of startup samples in both binaries,
19k more relative relocations) and not the catalogue (removing it changed
nothing); minor page faults rise from 1497 to 2608, worth about 1 ms.
The earlier hypothesis of relocation and first-touch cost is withdrawn.
Warm registration measured 2.6 ms because the first install in a process
pays allocator and hash-table growth the later ones do not.

Decision: the generated module ships on by default behind the `generated`
feature. Full API parity is the goal, and a session that attaches Polars
pays this once for the broader surface. `--no-default-features` gives the
hand-written adapter at its former cost, measured above, and its test
configuration builds: the generated-only test targets require both the
`generated` and `test-support` features, and the hand-written and
presentation tests pass without `generated`. The gate 1 budget of 4 ms is
not met on the cold launch and this record says so; cheaper registration
in Rune's context build is upstream work and a follow-up, not something to
fake by trimming the surface.

### Tests and replay

`adapters/polars`, default features: drift (regenerates with `--locked`
and compares every generated file including the fixtures and the oracle
test), accounting with the disposition partition, the smoke test, the
oracle test and its integrated controls, the routing proof (its tests
take a process-wide lock because the engine counters are global), the
end-to-end script through the adapter binary, the `oracle.rs` controls,
and the presentation suite unchanged. Without `generated`: the library
tests and the presentation suite. The engine unit test flake noted in
0068 remains when the library tests run in parallel; single-threaded all
pass. Hand-written bindings are unchanged except for a `Clone` derive on
the four wrapper types and `pub(crate)` on their fields.
