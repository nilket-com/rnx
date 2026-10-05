# rnx 0169: standing runtime suite and complete startup attribution

**Status:** plan, for Claude's review. First W0/W2 record after 0168 and the
jointly agreed rune_roadmap.md. Codex implements; Claude independently reviews.
The user's competitiveness mandate covers this work. This record measures and
prepares the standing suite; it chooses no optimization and changes no shipped
rnx dependency, language semantics or budget contract.

## 1. Deliverable and pins

In rnx-bench, add `probes/rune-runtime/run.sh --out NEW_DIRECTORY`: one bounded
entry point that verifies inputs, builds isolated subjects, runs correctness and
robustness controls, clock preflight, measurements and fail-closed analysis.
Retain a command/status ledger, locks, source/compiler/binary hashes, raw samples,
profile observations and a generated comparison report. Refuse an existing output
path before doing work. Build caches may live separately from result directories;
no earlier deciding evidence is overwritten. Rehearsal/control results are labelled.

Pins: rnx **b240937** (runtime source unchanged from 4fbbd3b); released Rune
**0.14.2**; engine-development fork **bb8e69372353c50e271c9f115bc771c77aa6b83e**;
rnx-bench 0168 baseline **20f9806**. Read baseline data from that commit, not a
mutable checkout. Keep the 0168 source/harness compatibility rules, release
settings and feature distinction (old std/fmt; main additionally anyhow).

Build stock rnx from the pinned clean source in an isolated worktree/target, using
its default release configuration and lock. Record its actual features/toolchain.
Do not add an adapter, server feature or special literal shortcut. PUC Lua 5.4
and LuaJIT use the existing executable installations; bind versions, binary hashes,
commands and fixture hashes before the run. These are contemporary reference rows,
not new Lua conformance claims. No installation or source upgrade is needed here.

## 2. Standing measurement matrix

Keep engine-harness rows distinct from stock CLI and Lua rows:
- Unmodified old/main engine harnesses: floor; empty/full default Context;
  runtime extraction; compile-only answer; fresh empty/42/numeric/strings/fib;
  compile-once repeated calls (20 calls, three process replicates). Same source,
  budgets and expected output as 0168, with any harness cleanup disclosed.
- Stock **`rnx eval 42`**, exactly that expression, default budget. Separately
  stock `rnx run` on the five existing validated fixtures; compute run commands
  state the explicit 1e9 budget. No changing eval into a println block and calling
  it the literal-expression baseline. Both answer forms print the same checked
  stdout, but entry paths and returned-value display remain distinct.
- PUC/LuaJIT: the corresponding existing lua-rust-0001 empty/42/numeric/strings/fib
  files, unmodified, default LuaJIT behavior. LuaJIT may compile traces; report
  startup and sustained work without attributing its speed to an unmeasured cause.
- Separate late unpinned answer controls; never pool with the deciding P-core run.

Before timing, independently compute/check outputs using the existing Python
oracles. Every timed invocation checks status, stdout and stderr. Engine reused
calls verify each output. Errors, defaults, async, aliases, closures/iterator traits
and the 0168 bounded robustness corpus run before timing on both engines. Add
stdlib checks exercising HashMap, HashSet and VecDeque and trait-provided iteration
so inaccessible private modules are not assumed away. Stock rnx remains an old-
engine product baseline, not evidence that main's core migration works.

Use the native spawn/pipe-capture/blocking-wait clock. Hyperfine -N --output=pipe versus native
true and cached Rust print-42: 50 samples, five warmups, median difference <=0.15 ms
for each or STOP. Pin logical P-core 4, record CPU/frequency policy before and after
measurements. Fast endpoints: five warmups, three shuffled rounds of 30 samples.
Compute endpoints: two warmups, three shuffled rounds of five. Retain all samples;
report median, min/max and p10/p90. No subtraction of unrelated medians.

Separate counters/allocation/RSS from deciding wall time. Engine FIFO-bracketed
PMU windows remain allocation-uninstrumented with three repeats and positive
runtime, >=99% running. CLI/Lua whole-command perf captures, if taken, are labelled
as including startup/loader work and are not compared as equivalent to the engine
FIFO windows. Record unavailable PMU data and stop any intended attribution that
requires it; never fabricate counts. Allocation passes and max RSS are separate.

## 3. Complete registration diagnostic on main

0168's public-only breakdown excluded three collection modules. This record
profiles **the actual complete Context::with_config sequence** on pinned main:
all 34 modules in exact source order, including hash_map/hash_set/vec_deque and
f64::consts, with default-module flag and normal error propagation intact.

Use an isolated diagnostic worktree and retain a minimal source patch. A gated
profiling feature may instrument private code; it is not enabled in any primary
binary and is not a shipped fork/product change. Base source remains pinned and
clean. Record patch/source hashes, feature tree and every instrumentation site.
Do not replace Context::with_config with a handwritten incomplete module list.

Instrumentation:
- Direct module construction and install intervals, complete Context construction
  and drop intervals; stdio true and false reported separately, since stock rnx
  supplies its own I/O bindings. Audit stock config source to explain the difference
  without calling the engine Context the entire rnx product context.
- Non-overlapping top-level install stages: module metadata, types, traits, items,
  associated items, trait implementations, reexports and constructs, plus remaining
  orchestration. Preserve exact source order and all loops. Nested attribution
  (e.g. trait handler/default-method expansion or install_meta) is labelled inclusive,
  with counts, and **never added to its parent as independent work**.
- Source-backed event counts for metadata insertions, trait implementations/default
  handler registrations and relevant cloning/handler construction where feasible.
  Distinguish measured counts from code-reading hypotheses; do not label arbitrary
  allocation events as Arc or clone costs without evidence.
- Separate allocation-enabled interval snapshots/counts; diagnostic collector/output
  costs identified. Buffer reporting outside measured intervals. Primary full-context
  measurements and allocation-disabled PMU windows use unmodified engine code.

Fingerprint complete context inventories (sorted hashes/categories of native
functions, types, traits and metadata, and flags) on diagnostic capture enabled
versus disabled. Run the corpus/collection controls in both diagnostic modes and
unmodified main. Same inventories do not prove all semantics; retained source
patch audit and output checks are additional gates. Injection omitting a private
module must fail the module/inventory/capability gate.

Three process replicates, fixed 20 full-context builds per replicate for direct
phase observations; construction/drop both included. Quantify instrumented versus
uninstrumented full-context overhead in contemporaneous observations. Diagnostic
clocks are attribution only. If instrumentation adds >20% median full-context
cost, STOP using its timing ranking, coarsen it under a disclosed repair, and
rerun the diagnostic controls before interpreting it. Counts and source-site
observations may remain useful but cannot replace missing timing evidence.

The report ranks actual stages/modules and lists the next candidate experiment
with supporting source sites. Historical trait-handler sharing, lazy metadata,
cloning and snapshots remain hypotheses until measured. No optimization is
implemented in this record, and no sub-ms success claim follows from a partial
or instrumented context.

## 4. Regression report and fail-closed gates

For unchanged old/main harness endpoints, compare contemporary medians to the
retained 0168 distributions by exact workload/feature identity. Mark a baseline
reproduction warning if the absolute median shift exceeds the larger of 10% of
the baseline median or its retained p90-p10 spread. This tolerance is for detecting
changed run conditions, **not** for accepting a future optimization regression.
A warning stops attribution/optimization selection pending diagnosis; retain it.
Do not remove samples or change thresholds after seeing results. Future change
records pre-register their own paired baseline/candidate regression gates.

Preflight validates pins, clean source trees, fixtures, locks/configurations and
binary identities before execution. Controls must demonstrate refusal for wrong
pin/source/fixture and for existing output paths, before downstream subjects run.
Analysis must refuse missing/duplicate samples or modes, wrong sample count,
failed/non-finite/non-positive time or count, mismatched output/status, missing
private-module profile data, altered inventories, non-finite profile observations
and improper parent/child aggregation. Include representative corruptions against
retained real output, not only success fixtures.

Runner lifecycle: separate bounded build and measurement phases (maximum 30
minutes build, 10 minutes measurement, five-second termination grace). Track
all owned producer/controller process groups; terminate and reap on error, timeout
or interrupt, including robust-control children. No detached waiters or orphaned
perf helpers. Expiry fails with a named phase and preserves the partial ledger.
A short-timeout child/descendant control proves bounded cleanup. All CPU-heavy
runs are serialized; no concurrent benchmarking by Codex and Claude.

## 5. Review and closure

Commit generic rnx plan/implementation evidence plus rnx-bench tooling/results;
retain the diagnostic fork patch in bench (no upstream/main base movement).
Claude reviews local commits and independently spot-checks before push. Root rnx
source and dependency remain unchanged, so this record does not rerun unrelated
adapter suites or credit main with the blocked migration suites.

Deliver: actual stock eval baseline, contemporary Lua comparisons, reproducible
standing runner, complete registration hierarchy/allocations and an evidence-led
next optimization proposal. If a gate fails, stop the affected claim, diagnose and
review any meaningful repair/replay; no relaxing capability, tolerance or workload
contracts to achieve a target. This record closes W0 tooling and supplies W2's
first attribution checkpoint, not the whole competitiveness mission.

## 6. Review amendment: shared machine admission

Accepted by Claude with A1, reconciled with the subsequently agreed two-track
protocol: use the single lock **/tmp/rnx-runtime-bench.lock**, not a second lock
name. Every CPU-heavy build, correctness/diagnostic run, timed or PMU phase
takes `flock --exclusive --timeout 1800` on that lock. Read-only analysis and
source edits remain parallel. The measurement phase holds it throughout; phase
deadlines and clocks start after admission, never during queueing. Record request,
acquire and release times and one-minute load average before/after each measured
phase. Reap owned descendants before release. A held-lock control proves the
runner waits and its execution timeout begins only after admission. Codex uses
dedicated 0169 worktrees; Claude uses dedicated 0170 worktrees. Reviewed commits
are integrated serially with explicit hashes; neither agent amends the other's
commits.

## 7. Reviewed calibration clarification

After retained launch, observer-placement and matched-I/O diagnostics, Claude
approved matching the reference output capture: Hyperfine -N --output=pipe
versus the unchanged native Command::output clock. Keep the same 0.15 ms gate,
process ownership, durable journal, core placement, workloads and all stops.
A fresh attempt repeats every control and calibration before deciding samples.
Matched blocks showed a consistent +0.110 to +0.145 ms native-minus-Hyperfine
offset, a systematic difference rather than random noise. Retain and report
contemporary offsets beside sub-ms medians; cross-record Hyperfine comparisons
need that caveat. No precision claim below the calibration bound follows.
No observer-core or process-group repair was applied.

## 8. Reviewed resident-observer repair

After attempt 3 and the retained READY/GO diagnostic failed calibration, a
resident native-observer diagnostic passed all six pairs, with contemporaneous
native-minus-Hyperfine offsets −0.019 to −0.031 ms. The exact historical rustc-42
preflight passed first. This is improved agreement under a different observer
architecture; no particular cache, scheduler, fsync or exec cause is proved.

Claude approved a resident driver per immutable command block: calibration,
warmups, each original seeded shuffled round, late unpinned observations, and
fresh reused-call subject invocations. Each entry remains a fresh target process
with the same Instant + Command::output interval. No subject VM/Context is cached.
Buffer records until block exit; durable pre-block job ownership and cleanup
remain. Bind each raw record by index and identity to the plan; declared and
executed sequence hashes must match. Keep all 36 identities, 41 groups, 2,265
samples, budgets, output checks, bounds, and every clock/baseline/overhead stop.
Actual distinct-PID, malformed/reordered record, and descendant timeout controls
run before attempt 4. Retain historical observer caveats; no offset subtraction.
