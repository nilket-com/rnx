# rnx 0168: choose the base for Rune runtime development

**Status:** plan. The user authorizes a sustained competitiveness mission through
`nilket-com/rune`, conditional on Codex's buy-in. Codex explicitly accepts.
Claude reviews and independently reruns gates; Codex plans and implements.
Lua remains an additional experiment. This record changes no shipped dependency.

## 1. Decision

Compare crates.io Rune **0.14.2** with fork main
**bb8e69372353c50e271c9f115bc771c77aa6b83e** (manifest 0.15.0).
Choose a base for subsequent runtime work, considering correctness, execution
cost, registration cost, API/maintenance trajectory and migration work together.
Main is not presumed faster; the existing 0030 measurements are hypotheses and
historical evidence, not contemporary results.

The campaign objectives are capability-preserving sub-ms fresh invocation,
competitive interpreter execution and, later, an evidence-led JIT decision.
Two kernels do not define overall competitiveness. Budgets, ownership, errors,
async and diagnostics remain part of the product contract; a fast literal-only
shortcut or disabled checks cannot satisfy the objectives.

## 2. Independent engine harnesses

In rnx-bench `probes/rune-base-0168`, build two release binaries with the same
logical harness and common requested features (`default-features=false`, std
and fmt). On main enable anyhow where the changed API requires it; list every
feature/package difference and retain both Cargo locks. Use one stable toolchain,
separate targets, identical release settings and source/compiler/binary hashes.
Small compatibility differences in harnesses are listed and reviewed.

Modes, with observable output validated before timing:
- process floor (exit without constructing a context);
- empty Context construction/drop;
- default Context construction/drop;
- default context plus runtime extraction;
- fresh source compilation, then execution: empty, printing 42, numeric modulo
  loop (1,000,000 steps), strings/table (20,000 items), recursive fib(27), from
  the validated lua-rust-0001 Rune fixtures;
- reused execution: compile once, repeat execution with fresh invocation state
  and identical outputs. Report the run-only in-process clock separately from
  process-level source-to-answer; no subtraction between unrelated medians.

Phase probes also measure in-process construct/install/runtime/compile/run/drop
where public APIs allow, allocation count and live/peak bytes, and separate
process peak RSS. A counting allocator has a disabled control: never mix
instrumented internal timings into the primary uninstrumented process table.
Default modules and resulting outputs must match their stated version; missing
stdlib support is a finding, not removed to make a version win. Explicit high
budget on compute fixtures on both engines, plus a default-budget refusal
control; no claim that raising a budget makes the checks free.

## 3. Correctness and port census

Run a fixed differential corpus before measurements: the five workloads, integer
boundaries and overflow/refusal, float arithmetic, strings/Unicode, vector/object
mutation and aliasing, errors, recursion, closures, iterator/default-trait method
calls, async await and budget exhaustion. Success values/output and semantic
error classes compared; wording/layout changes reported separately. Version
limitations and failures are retained. Do not credit a new engine as equivalent
from performance fixtures alone.

From a clean scratch worktree of rnx **4fbbd3b**, attempt the engine dependency
update to main, recording exact compiler diagnostics before any repair. Census
all direct Rune consumers (core, project, Jupyter, adapters and generator) and
classify changed symbols/signatures, public behavior and generated surface.
Limited mechanical compatibility experiments (Arc imports, feature names,
signature adjustments) are retained as a scratch patch, not shipped. If broader
semantic changes are required, stop that port attempt and price it; this record
must not silently become an upgrade or rewrite.

Run the current rnx release/test-support suites as baseline. Main suites run only
if the scratch port compiles; otherwise they are explicitly **blocked/not run**,
with no parity claim. When compilable, run the same relevant suites/configurations
and list every unchanged, adapted, failing and unrun gate. Adapter builds need
not be forced through an unbounded port; diagnostic/source census is sufficient
to expose that limitation. Measure compile/build time separately from execution.

## 4. Timing and reproducibility

Reuse the native rustc-42 spawn/pipe-capture/blocking-wait helper. Before deciding
runs, contemporaneous hyperfine -N and native `/bin/true` and cached print-42:
50 samples, five warmups; median difference <=0.15 ms for each or STOP.
No overhead subtraction. Pin core 4; record hybrid CPU and frequency policy.
An unpinned source-to-answer-42 control for both engines is separate and labelled.

Fresh floor/context/42 modes: five warmups, three shuffled rounds of 30 samples.
Compute modes: two warmups, three shuffled rounds of five samples. In-process
phase/reuse runs: fixed repetition counts declared in the harness before timing,
three process replicates, report all observations and dispersion. Bound every
producer externally; retain failures and repairs. Record expected outputs,
status and stderr for every measured invocation. Raw samples, locks, patches,
commands, machine state, source hashes and analysis scripts are committed.
No timing an unsuccessful build, and no replacing failed evidence silently.

## 5. Closure and next record

Deliver evidence and a base recommendation with explicit reasons and uncertainty:
registration versus VM costs, performance differences, correctness findings,
port blockers/estimated work, and what the comparison does not establish.
Claude reviews local rnx plan/impl and rnx-bench probe commits before push.
Do not change the fork base or rnx pin in this record. Subsequent fork changes
are focused, independently reviewed changes with differential/reproduction gates.
Any dependency migration is a distinct record; any public upstream filing remains
separate from development in our repositories.

## 6. Review amendments (accepted before implementation)

A1. Alongside wall time, each compute/phase mode has separate `perf stat
--cputype core -e instructions:u,cycles:u` captures using the FIFO-bracketed
method from 0166. Counters attribute work; wall time reports user experience.
Record unavailable counters explicitly and stop before substituting an invented
attribution. Perf-instrumented timings do not replace the native-clock table.

A2. Add bounded subprocess robustness controls for script recursion depths
10,000/100,000/1,000,000, deeply nested source expressions and nested data
literals. Retain generated source/parameters, expected result, exit/signal,
stderr and resource limits; classify correct, clean diagnostic/refusal, resource
limit, timeout, or native-stack overflow/abort. Disable core dumps, bound memory,
CPU and wall time; a terminated probe cannot damage the worker. These are
correctness/robustness findings, not performance samples, and weigh on the base
recommendation. Do not turn an abort into a successful error claim.

A3. A short trajectory table classifies the 0.14.2-to-main commit range by
compiler/VM/fmt/LSP/modules/alloc/deps/tests (classification rule and retained
per-commit rows), identifying the v2 heap-stack rewrite, indentation and fmt
changes. Record actual range/count, upstream last commit date and the unreleased
0.15.0 status without treating a manifest version as a published release.

A4. Port census ends after first full compiler-diagnostic capture and at most
four hours of mechanical compatibility work. Stop earlier if a semantic redesign
is necessary or the evidence already prices the blockers. No obligation to
exhaust the box. Any unbuilt main suite is explicitly blocked/not run.
