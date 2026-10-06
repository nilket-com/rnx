# rnx 0182: architecture choices for a competitive Rune runtime

**Status: draft plan / architecture decision record for joint review.** No
architecture is adopted. No engine edit, build, new measurement or shipping
change is authorized by this document. Codex owns integration and execution/design
sections; Claude contributes the evidence ledger and registration/compilation
options. Existing performance gates remain unchanged.

**User clarification, 2026-10-06:** "fast startup is a myopic take on the goal and
doesn't deserve to be a goal itself". The mission is Rune's competitiveness as a
runtime for useful applications, preserving its semantics and Rust integration.
Startup is a cost within that mission, not a separate goal or selection criterion.
A sub-millisecond `42` result cannot establish success.

This record reassesses the strategy after 0181, rather than selecting another
local optimization by default. It distinguishes established evidence, architectural
hypotheses, proposed experiments and decisions that still belong to the user.
It does not silently amend the historical targets in `rune_roadmap.md`.

## 1. What competitiveness must mean

The runtime must execute substantive programs efficiently while retaining the
features that make Rune useful: Rust-hosted data, extensible modules/protocols,
errors with usable source locations, controlled resource use, ownership and
aliasing rules, async suspension/resumption, cancellation and repeated invocation.
These are part of the product, not costs we can remove to win a benchmark.

Judge architectures on applications first and diagnostic kernels second. Proposed
scenario families below are a design menu, not a frozen benchmark or claim that
one implementation already wins. Before measuring, freeze concrete inputs,
outputs, capability equivalence, competitors, metrics and workload priorities.

| Scenario | User-visible work | Costs that must be included |
| --- | --- | --- |
| Script-driven data processing | Read structured records, transform/group/filter, produce a report | Compile/load, strings/collections/allocations, script work, native crossings, output; distinguish native kernel time from Rune orchestration |
| Embedded request/rule engine | Run user logic repeatedly with objects, branches and helper calls | Admission, invocation setup, sustained throughput, latency distribution, per-request memory, cancellation, errors and cleanup |
| Long-running computation | Numeric loops mixed with calls, iterators and mutable collections | Interpretation, any optimization/compilation and warm-up, hot throughput, memory and budgets over a realistic run length |
| Interactive development | Repeated small edits and executions with retained definitions | Edit-to-result time, incremental compile/load, diagnostics, first use and reuse; initialization counted where actually paid |

The existing loop/fib/call tests remain diagnostic anchors. Missing coverage
includes mixed dynamic types, closure captures, protocol overrides, collection
mutation/aliasing, error-heavy paths, host callbacks, async suspension and multiple
invocations. An adapter-heavy application can conceal a slow VM behind fast Rust
kernels; a pure arithmetic kernel can overstate the benefits to real applications.
Both must be visible, with end-to-end results and attributable components.

Compare a fresh CLI invocation, compile-plus-execute, a resident fresh invocation
and steady-state execution separately. They are different deployment paths, not
interchangeable measures. Resident contexts alone do not solve cold CLI costs.
Report total cost at realistic run lengths, not just the best hot phase. Competitive
references must provide equivalent useful work; differences in semantics and host
capabilities must be named rather than hidden in a ratio.

## 2. Evidence ledger and registration/compilation options

**Contribution pending from Claude.** Integrate retained 0168–0181 results with
exact record references, numbers and limits. Include registration/metadata work,
compile and load, invocation reuse, engine comparisons, allocation, source-change
and profile sensitivity, and the 0181 whole-process delivery observations.

Options to evaluate here: immutable registration base plus dynamic overlay,
precomputed metadata, reusable compiled units, lazy registration with first-use
accounting, and cheaper eager construction. For each name the application path
it improves, work it can actually avoid, invalidation/compatibility/diagnostics
contracts, remaining per-invocation work and a bounded experiment that could
reject it. These are enabling mechanisms; no independent launch-time target
makes one the preferred architecture.

Do not assume a compiled unit can serialize native handlers, that a context can
be safely shared between arbitrary hosts, or that a cache hit represents an edit
or cold installation. Artifact trust, code/native ABI identities, feature/module
configuration and invalidation must be specified before cached execution is a
candidate product path. Compilation latency, cache misses and first use remain
part of the application cost.

## 3. Execution architecture choices

### What the evidence currently permits

0170 measured a million net run-path allocations for a million-step integer range
loop, versus seven for its manual while counterpart. This establishes an allocation
opportunity; boxed Option/native-next is a source-backed explanation, not exclusive
allocation attribution. Its roughly 100–190 net run-path instructions per bytecode
instruction on several kernels is a ratio including setup/native work/teardown,
not a measured cost for each opcode (`0170_interpreter_profile_evidence.md`).

0175/0176 demonstrated large range improvements that did not pass all frozen gates;
0177 found that its proposed inlining-neutral cut did not address the measured
unrelated code change. These results do not establish that specialization is
hopeless, nor permit adopting the rejected candidates.

0181 established increased MITE delivery and lower DSB share on whole-process
numeric/range windows alongside reproduced cycle excess, despite fewer retired
instructions. It did not locate an interpreter loop, establish the cause of changed
machine code, or explain fib's extra instructions. Optimizing source instruction
count alone is insufficient; instruction delivery/code shape and complete workload
costs must stay visible (`0181_execution_cost_continuation_evidence.md`).

### Candidate mechanisms and their decision questions

| Mechanism | Hypothesis worth testing | Contracts / main implementation risks | What a useful feasibility result must establish |
| --- | --- | --- | --- |
| Primitive value/store specialization | Avoid generic lifetime machinery when values truly carry no owned payload | Alias/drop order, fallibility, deep-value dismantling, allocation-limit observers | Work removed on real mixed-value paths; unchanged ownership/errors; a win outside one homogeneous loop |
| Iterator/allocation specialization | Avoid a heap wrapper/native crossing for common iteration | Exact protocol identity, exhaustion/overflow, user overrides, borrow conflicts, budgets and allocation-limit behavior | Allocation reduction and application-level benefit without changing general iterators |
| Calls/returns/frame/drop | Reduce argument/frame movement and common return teardown | Closures/captures, reentrancy, async frames, exact cleanup and unwind behavior | Faster call-rich programs; same cleanup on return/error/budget halt |
| Dispatch / instruction layout | Reduce repeated decode, stack movement or generic helper work | Larger hot code, compiler partition/inlining sensitivity, trace/debug visibility, precise budgets | Gain on a mixed opcode corpus under a fixed build; diagnostic evidence identifying what work changed |
| Specialized bytecode | Fuse or type-specialize frequently executed sequences | Compiler/VM ABI, guards, fallback, branch targets, source mapping, exact charging | Less work per useful operation, bounded code growth and compile cost; semantic equivalence on failed guards |
| Value representation redesign | Make common primitive access and ownership cheaper across operations | Much larger ABI/unsafe/GC/borrow surface; Rust userdata and alignment/portability | A justified representation prototype plus differential/alias/deep-value checks before broad migration |
| Tiered native compilation | Remove interpretation overhead from hot, type-stable regions | Guards, deoptimization, safepoints, budgets, native calls, code memory and async/debug integration | Correct fallback/exit paths, useful speed at realistic break-even lengths, and bounded compilation/code memory |

These are hypotheses, not ranked removable percentages. Source-level sample mass
cannot be added across overlapping/inlining-sensitive categories. An apparent
store hotspot is not proof that a representation redesign removes all its work.

### Stronger interpreter, specialized bytecode, or JIT

A stronger interpreter remains valuable as the universal fallback, cold execution
path and diagnostic reference, even if native compilation later succeeds. A
specialized-bytecode tier could exercise guards, source mappings and budget
semantics without initially adding a machine-code backend. It could also turn
into another large dispatch loop; reduced opcode count is not itself success.

A JIT merits an explicit feasibility decision now rather than being postponed
indefinitely behind local tweaks. Its proposal must state the subset and all exits:
which values/operations are supported, when execution falls back, how live owned
values and frames are reconstructed, and how exact error/budget behavior survives.
A no-callback, integer-only prototype cannot be credited as competitive Rune.
It can answer a bounded backend/guard/break-even question if its limits are stated.

A compilation backend would solve instruction emission, not these runtime contracts.
Picking one before identifying the tier's boundaries and ownership model would
prematurely turn an architecture question into a tooling choice. No backend is
selected here. No speed or schedule estimate is claimed without a scoped design
and implementation inventory.

Compare total execution at several fixed workload lengths: interpreter only,
compilation plus execution, warm reusable code, failed guard/fallback and invalidated
code. Identify the break-even point and the inputs that never reach it. JIT code
must remain subject to the same lifecycle and resource controls; charging fewer
operations, omitting checks or abandoning retained state is not a speedup.

## 4. Decision framework and proposed recommendation

**Keep existing gates and historical outcomes.** Records 0171/0172/0175/0176/0179
remain rejected under their own rules. This architecture review does not reclassify
those outcomes, change a denominator, or authorize a profile that masks regressions.
The bounded 0183 frontend study is a separate reviewed record: localization first,
then at most one intervention from a predeclared family if feasible. Correlation
or an unavailable precise event must not be presented as a causal proof.

A future application-level trade-off proposal must be explicit before results:

1. Freeze the representative scenarios and priorities from actual intended use,
   naming why they matter and the deployment path they represent.
2. State acceptance limits for each scenario and for memory, code/compilation size,
   latency, build/install costs and cold/first-use/reuse behavior. Do not silently
   replace exact gates with an average or choose weights after a result.
3. Keep semantic/resource/lifecycle checks as hard prerequisites. Any requested
   change to their contract must be a separately visible design decision.
4. Keep diagnostic anchors reported, including every regression; describe which
   declared trade-off could accept one. New approval applies only prospectively.
5. Record negative results and a bounded stopping rule. Retain an unchanged
   baseline and require stronger independent checks as architectural scope grows.

**Proposed recommendation, for Claude and user review:** choose the next architecture
by its potential to close the application-level execution gap while preserving Rune's
embedding contract. Establish representative programs and an architecture inventory;
then review a bounded specialization/tiering feasibility proposal against a focused
interpreter-redesign alternative. Registration/artifact work belongs where it changes
those programs' total cost, not at the head of the campaign solely because `42` is slow.
0183 may sharpen one compiler/code-shape question; it should not hold the entire
architecture decision hostage to another round of PMU diagnostics.

Decisions to expose to the user after joint review:

- Are the proposed application scenarios the right expression of competitiveness?
- Which architectural feasibility cut offers the highest information value for those
  applications, with stated cost and semantic scope?
- Do we retain current diagnostic performance gates for that cut, or explicitly
  approve a prospective application-priority trade-off? The latter is not assumed.

The final recommendation waits for the evidence ledger, architectural constraints
and joint review. This draft authorizes document work only. Shipped rnx remains
Rune 0.14.2; fork main remains unchanged; Lua remains an additional experiment.
