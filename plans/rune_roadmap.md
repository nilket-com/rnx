# Rune runtime campaign: long-term roadmap

**Status:** jointly agreed by Codex and Claude (2026-10-05), attached to record 0168.
The user asked for the long-term plan up front. Individual records plan, implement
and close one step at a time; this document states objectives, gates and reassessment
points. Record counts below are planning estimates, not delivery commitments.

**Mission:** own Rune's competitiveness through `nilket-com/rune`. The fork is a
vehicle for measured development and potential upstream contributions. Rune
semantics and rnx's budgets, ownership, diagnostics, async and default capabilities
remain part of the goal. Lua remains an additional experiment.

## 1. What winning means

| Objective | Current evidence | Target | Stretch |
|---|---|---|---|
| Fresh actual `rnx eval 42` | Measure the stock command in W0; 0168 engine harness printing 42 is 3.73 ms old / 4.29 ms main, **not this command** | <1.0 ms | <0.6 ms |
| Full stdlib context construction/drop | 0168 old / main whole-process endpoints 3.41 / 3.64 ms, including process floor | Direct in-process complete construction <0.5 ms; also track drop | |
| Numeric loop, 1M steps | Engine harness old / main 83.8 / 125.1 ms; earlier PUC probe ~6.3 ms | Within 2x contemporaneous PUC | Parity |
| fib(27) | Old / main 38.6 / 53.3 ms; earlier PUC ~9.5 ms | Within 2x contemporaneous PUC | Parity |
| Strings/vector, 20k | Old / main 14.4 / 19.3 ms; earlier PUC ~4.8 ms | Within 2x contemporaneous PUC | |
| Deep expressions and literals, depth 10k | Old aborts; main correct in bounded controls | Clean result or diagnostic on the adversarial suite | |
| Hot typed numeric kernels, if JIT proceeds | No implementation or feasibility proof | Within 3x contemporaneous LuaJIT on chosen kernels | |

Targets use the standing suite and full stated capability set. No literal-only
special case, omitted stdlib, disabled budgets or dropped ownership checks earns
a target. Engine harness, stock CLI and reused execution are distinct metrics.
Earlier Lua numbers motivate targets; deciding ratios require contemporary runs.
Two kernels do not define competitiveness. Grow the suite before general claims:
collections/strings, function calls/closures, dispatch and allocation, host crossings,
errors and budget exhaustion, async suspension/cancellation and realistic scripts.
Averages cannot hide a serious regression on one workload.

## 2. Workstreams

### W0. Standing measurement and correctness infrastructure (0168 onward)

Promote the engine comparison to a one-command bounded suite with a fresh output
directory: source/binary/compiler hashes, exact outputs/error classes, phase and
fresh/reused clocks, full registration, allocation/RSS and CPU counters. Keep the
clock preflight and stopping rules. Public-module breakdowns are labelled partial,
never substitutes for complete registration. Add actual stock CLI and contemporary
Lua references. Retain failures and disclose repairs/replays.

The small 0168 differential corpus is a seed, not a semantics certificate. Expand
it with engine changes and retain intended behavior differences separately.
Each fork change records before/after distributions and instruction counts.
Regression gates have a pre-stated noise rule; any real regression requires an
explicit tradeoff rather than being averaged away. Sanitizer/Miri/fuzzing or
other relevant checks accompany unsafe changes; throughput alone cannot pass.

**Exit:** one bounded invocation reproduces the retained baseline within its
predeclared uncertainty, validates all outputs and emits a regression report.

### W1. Base and robustness (0168)

Options: A, optimize 0.14.2 and backport main's heap-stack fixes; B, develop on
pinned main and address its measured speed deficit. Compare port contracts,
execution/registration work, robustness and maintenance trajectory. Upstream
alignment favors B but does not prove optimization potential. A fixed maximum
number of migration records is not a valid cost estimate.

**0168 decision:** B for isolated engine development; released 0.14.2
continues shipping. Main has the robustness fixes and newer architecture; its
34–49% compute deficit is explicit. The scratch core port is blocked; adopting
the fork requires W4. No automatic moving to another upstream commit.

**Exit:** joint base decision, clean robustness outcomes on chosen base, small
corpus agreement with reference and a documented migration boundary. Remaining
untested semantics and adversarial limits stay visible.

### W2. Capability-preserving fresh startup (estimate 2–4 records)

Profile complete registration first. 0168's partial breakdown points to install
work more than module construction. Historical 0030 hypotheses (trait default
expansion, per-type function-handler arcs, item/hash cloning and drop) must be
confirmed against main, including private collection modules. Candidate changes:
1. Reduce duplicate handler/metadata construction and share immutable data while
   retaining conflict detection, module extension and automatic trait behavior.
2. Defer metadata work when correctness permits; prove lookup/import/diagnostic
   behavior and first-use latency. Do not omit names needed by compilation.
3. If needed, design a precomputed registry/snapshot. Explicitly handle native
   function relocation, version compatibility, custom modules, trust, portability
   and fresh-process costs. Serialization is a hypothesis, not a ready API.

Every optimization has differential and registration-conflict checks. Full default
capabilities remain reachable. Process-exit cleanup may be avoided only in an
explicit one-shot execution path with equivalent observable resource semantics;
never leak repeated invocations or workers or skip lifecycle cleanup in them.

**Exit:** actual fresh `rnx eval 42` <1.0 ms under the full product contract and
other startup fixtures passing; engine attribution and first-use costs reported.
If the first two approaches do not reach ~1.5 ms, write a bottleneck/design
checkpoint before attempting a snapshot. This is a reassessment, not abandonment
or permission to narrow the capability set.

### W3. Interpreter competitiveness (estimate 6–12 records)

Profile on the pinned P-core, wall time plus instructions/cycles and useful
opcode/allocation attribution. Investigate only demonstrated costs:
- Value representation and primitive operations, reference-count/borrow traffic;
- opcode dispatch, superinstructions, stack/register movement;
- call frame setup, arguments and return (fib and realistic call workloads);
- strings/collections and avoidable allocation or cloning.

Budget accounting is a contract. Basic-block/back-edge charging is only a
separate design candidate, not an approved relaxation: establish exact behavior
or obtain explicit agreement on any changed accounting/overshoot contract before
implementation. Tight budgets, nested async, native callbacks and cancellation
must remain bounded. Removing checks cannot be called optimization.

A register IR or broad value redesign needs its own design record and stronger
semantics/unsafe validation. Small library additions are separate scoped changes,
not a way to obscure runtime progress.

**Exit:** within 2x contemporary PUC on the compute suite with the growing corpus
and host contracts intact. If three consecutive optimization records each gain
<5%, reassess attribution and architecture. Decide on structural redesign or W5
from evidence; do not interpret three weak ideas as proof the engine is hopeless.

### W4. Product integration (parallel after a base is chosen)

Dedicated migration record: shared arcs/fallibility, diagnostics/source loaders,
budgeted sync/async resume semantics and reusable invocation lifecycle; adapters,
generator and conditional builds; project/server/Jupyter/notebook integration.
Choose an immutable dependency/release strategy (git revision versus published
fork crates) explicitly. The user's authorization covers preparing and testing
this work; agree the user-facing dependency/release decision before switching
what published rnx installs. No need to ask again for already authorized actions.

**Exit:** unchanged product contracts and full relevant suites on the new base,
measured startup/steady-state behavior, reproducible install and rollback story.
This workstream may start once isolated engine changes justify migration; it
need not block W2/W3 experiments. Uncompiled adapters are not credited by a
standalone engine harness.

### W5. JIT feasibility, then a separate implementation track if justified

After W3 progress or a reassessment, a bounded feasibility record investigates
Cranelift or another justified backend on a typed integer/float subset (locals,
arithmetic, loops, calls). No assumption that JIT is necessary or feasible yet.

Candidate go criteria: >=5x speedup over the then-current interpreter on selected
hot kernels; cold compile/initialization off the fresh path through lazy use;
correct overflow/error semantics, budget enforcement, safepoints, deoptimization
and ownership; bounded code/compilation memory; maintenance costs. Report cold,
warm-up and hot behavior, not only steady-state throughput. A 5x microkernel gain
alone does not establish competitiveness or permit weakening the contracts.

If promising, separately review the broader JIT track. If not, record limits and
continue improving the interpreter where evidence supports it.

### W6. Upstream relationship and fork maintenance (continuous)

Keep changes reviewable, one concern per change with reproduction/tests. Prepare
upstream drafts; public issues/PRs require the user's go. Rebase experiments onto
new upstream commits only as an explicit measured base update. Preserve published
pins and avoid automatic force-pushing shared history. If upstream rejects/stalls
a necessary change, record the divergence and its maintenance cost.

## 3. How we work and report

Plan/implementation record cadence; Codex implements, Claude independently
reviews/reruns, with role swaps by agreement. The user's unfettered mandate plus
joint agreement authorizes repository work and pushes; public upstream actions
and release/dependency decisions are explicitly separate. No repeated approval
loops for work already authorized.

Every claim has a pre-stated measurement/correctness gate, provenance and stop
rules. Checkpoints report what improved, what regressed, what remains unproven
and the next experiment. The user sees workstream exits and reassessment points.
Numbers and record estimates are targets and evidence, not promises of a schedule
or proof that two agents can solve every compiler problem.
