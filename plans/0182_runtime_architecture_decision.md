# rnx 0182: architecture choices for a competitive Rune runtime

**Status: joint proposal awaiting the user’s decision.** No
architecture is adopted. No engine edit, build, new measurement or shipping
change is authorized by this document. Codex owns integration and execution/design
sections; Claude contributes the evidence ledger and registration/compilation
options. Existing performance gates remain unchanged.

## What we ask you to decide

1. **Sequence:** accept, reorder or replace applications → bounded frontend study → isolation/tiering/fresh-core feasibility → architecture choice (fresh-core scope remains under discussion). We recommend it to replace local-cut churn with decision evidence (§4).
2. **Targets:** retire launch time as a goal; keep the 2x-PUC kernel figure only as a diagnostic, or choose a different target. We recommend application criteria because kernels alone do not establish competitiveness (§1, §4).
3. **Gates:** keep existing gates for feasibility cuts, or approve a predeclared application-priority trade-off. We recommend keeping them until a concrete prospective trade-off is reviewed; none is assumed (§4).
4. **Applications:** confirm data processing, embedded requests/rules, long-running computation and interactive development, or name higher-priority applications. We recommend these to cover rnx’s actual deployment paths (§1).

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
The concrete rnx instances are CLI scripts, sessions/Jupyter, Polars/Candle
orchestration and the reusable request host. Only the last currently has the
controlled runtime/host comparison used in this campaign; existing adapter
workflow timings are useful evidence but not that matched engine comparison.

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
Report total cost at realistic run lengths, not just the best hot phase.
Section 2.4 distinguishes where preparation is actually paid. Competitive
references must provide equivalent useful work; differences in semantics and host
capabilities must be named rather than hidden in a ratio. The comparison set is
frozen with each scenario: retained Lua/LuaJIT kernels and Flask/native-axum host
results are reference roles, not a complete competitor set for Rust embedding.
Other embeddable runtimes are candidates to investigate, not selected here.

For example, a request comparison must match parsed inputs, exact response/error
behavior, invocation state isolation and stated concurrency/body limits. A native
Rust handler with no script compiler or instruction budgets is a performance
ceiling, not capability-equivalent Rune. A peer without equivalent budget or
cancellation support needs that capability difference named; we cannot remove
Rune's checks or quietly make the peer do less work to claim an equal contract.

## 2. Evidence ledger and repeated work

### 2.1 What the retained campaign established

The following ledger cites retained record files, not new measurements. Numbers
refer to the named binaries and deployment paths in each record. Negative
optimization decisions remain negative; a correctness pass or diagnostic gain
is not adoption.

| Record | Established observation | Outcome and limit | Retained source |
| --- | --- | --- | --- |
| 0168 | Fork main wall ratios versus 0.14.2: numeric 1.492, strings 1.339, fib 1.383. Depth-10k expressions/literals abort on old and complete on main. | Main chosen for isolated development, not a speed upgrade. The scratch rnx port did not compile; product stays 0.14.2. | [0168 evidence](0168_rune_competitive_base_evidence.md), lines 9–19, 45–47, 137–148 |
| 0169 | Stock `rnx eval 42` 4.173 ms; Lua print-42 .436 ms, LuaJIT .464 ms. Main trait-installation stage 1.503 ms; constructor 2.323 ms; 2,531 metadata insertions, 172 trait implementations, 1,717 native-function registration attempts with stdio. | Launch/stage measurements, not application competitiveness or removable bounds. Stage medians do not sum to the parent. | [0169 evidence](0169_complete_startup_attribution_evidence.md), lines 160–165, 192–217 |
| 0170 | About 100–190 net run-path instructions per dispatched opcode on several kernels. Range loop: 1,000,010 net allocations; corresponding while: seven. Main store sites about 48% of Vm::run self samples on while. | Profile categories overlap source operations and depend on inlining; samples are not cost per store or removable percentages. | [0170 evidence](0170_interpreter_profile_evidence.md), lines 37–54, 64–88 |
| 0171 | Store fast path reduces while instructions 4.54%, fib 3.39%, numeric 2.71%; corresponding wall medians fall 6.1%, 5.9%, 11.5%. | STOP: context-only invocation grows 2.1% instructions; empty/answer/alias windows exceed gates. Their setup includes changed compiled context code but no VM store execution. Compiler cause remains hypothetical. | [0171 evidence](0171_store_fast_path_evidence.md), lines 32–44, 61–73, 89 |
| 0172 | Borrowing registration data reduces context instructions 3.62%, with 2,277 fewer allocation calls. | STOP: unchanged-source while/compare instruction effects +.53%/+.70%; cause unverified. | [0172 evidence](0172_borrow_context_type_evidence.md), lines 32, 46–81 |
| 0173 | Three same-source builds byte-identical. Five null edits: three byte-identical; measured null-edit VM shifts at most .001%, largest anywhere .085%. | Does not reproduce the large prior shifts; no general causal exclusion. Launch conditions are a separate part of this diagnostic. | [0173 evidence](0173_build_perturbation_evidence.md), lines 64–78 |
| 0174 | Thin/fat LTO with cgu=1 reduce context/setup instructions about 8.4–9.2%; cgu=1 without LTO reduces context 1.21%. Single-unit profiles add 5.7–11.4% instructions on several interpreter kernels; other kernels improve. Harness builds rise from about 37 s to 78–87 s. | No profile wins everywhere. No assembly proof of the mechanism; a global profile switch is not interpreter isolation. | [0174 evidence](0174_build_profile_evidence.md), lines 26–29, 47–61, 92–99 |
| 0175 / 0176 | Inline/outlined range specialization reduces numeric/range instructions about 30–39% and wall medians about 40–52%; numeric allocation calls fall 96.46%. | Both STOP: fib +7.8–8.6%, calls +3.2–4.0% instructions, plus setup regressions. Approach parked; substantial local gains do not waive gates. | [0175 evidence](0175_integer_range_iteration_evidence.md), lines 27, 53–63; [0176 evidence](0176_outlined_range_dispatch_evidence.md), lines 29, 35–52, 72 |
| 0177 | Candidate fib has a recorded pop_call_frame→Repr-drop edge: 2,542,488 calls, 45,764,784 Callgrind Ir; base has no recorded edge. | FEASIBILITY STOP: unrelated changes are not reachable from an executed range helper. Absence of an edge does not mean base skipped destruction. Callgrind Ir is not native PMU instructions. | [0177 evidence](0177_range_callsite_neutrality_evidence.md), lines 13–32 |
| 0178 | Under cgu=1, range-candidate context shifts fall to .001–.002%, but fib stays +2.267%, calls +2.1–2.4%, and while/compare/vector +2.4–2.9% instructions. | All four cells STOP. Profiles move the regression distribution rather than establish isolation or eliminate it. | [0178 evidence](0178_profile_paired_range_study_evidence.md), lines 34–51, 88 |
| 0179 | One registration-formatting change reduces context instructions 7.242%, run-answer 6.504%, first-use about 3.5%; 5,801 fewer context allocation calls. | STOP; also below its two-leg instruction-win requirement. Fib +2.283% instructions; numeric/range wall costs +8.32%/+15.58%/+16.21%. Setup wall decreases are inside baseline widths. | [0179 evidence](0179_capability_preserving_startup_evidence.md), lines 81–83, 159–180, 190–191 |
| 0180 | One candidate's generated VM/drop code differs; whole-process, startup-subtracted contrasts approximately −12/numeric iteration, −10/range iteration, +33/fib invocation. Cycle excess reproduces; reference-cycle observations weaken a frequency-only explanation. | Measurement-validity STOP in B; only R/A valid. Whole-process subtraction is not hot-region attribution; failed B has no usable Topdown conclusion. | [0180 static checkpoint](0180_static_checkpoint.md), lines 28–69; [0180 evidence](0180_execution_cost_explanations_evidence.md), lines 74–100 |
| 0181 | Whole numeric/range/call windows have more MITE delivery and a lower DSB share alongside excess cycles. Branch mispredictions fall; two measured load events do not rise. | COMPLETE diagnostic. Compatible frontend category, not proof of interpreter-loop cache loss or compiler causation. Fib's instruction growth still lacks dynamic attribution. | [0181 evidence](0181_execution_cost_continuation_evidence.md), lines 87–137 |

### 2.2 What the repeated pattern means for architecture

Store, range and registration candidates show that substantial measured work can
be removed on their intended paths. They also show that changed compiled machine
code can shift costs on paths outside that intended operation. This is not merely
a precision problem with a .5% gate: some effects are several percent or larger,
and profile studies did not remove all of them.

The five candidates' effects differ. In 0171/0175/0176, a context-only process
never executes the changed VM path. In 0172/0179, VM programs still execute the
changed registration during initialization, but the generated interpreter also
changes; whole-process results alone do not assign the excess to registration.
It would overstate the evidence to say every stopped workload never executes
any changed code. The compiled-code finding, not that generalization, motivates
studying interpreter compilation boundaries.

This is an observation about retained sources, builds, compiler and hardware,
not a theorem that every monolithic VM or future change behaves that way. A
feasibility study must show whether a proposed boundary actually insulates hot
execution while allowing useful local changes; a smaller Vm::run symbol or one
codegen unit by itself did not establish that.

### 2.3 Competitive reference points and application evidence

| Window | Rune main, named record | Reference, named record | Scope |
| --- | --- | --- | --- |
| numeric, 1M-step range | 125.059 ms in 0168; 112.790 ms base in 0176 | Lua 5.4.7 6.42 ms; LuaJIT 8.28 ms in 0175 | Different record dates; full process plus workload. Indicative gap, not a deciding cross-record ratio. |
| while, 1M steps | 69.397 ms base in 0176 | Lua 5.4.7 9.64 ms; LuaJIT 8.29 ms in 0175 | Diagnostic kernels, not representative application rankings. |
| fib(27) | 53.347 ms in 0168 | No contemporary campaign Lua fib comparison established here | The historical roadmap's earlier PUC figure is not a deciding reference. |

Sources: [0168](0168_rune_competitive_base_evidence.md), lines 45–47;
[0175](0175_integer_range_iteration_evidence.md), line 71;
[0176](0176_outlined_range_dispatch_evidence.md), lines 40–43.

The request host supplies controlled application-shaped comparisons. In 0162,
separate instrumentation put fresh prepare at about 2.7 ms per request and handler
VM work at 10–80 µs. These phase timings are not uninstrumented latency shares.
In 0163, prepared-slot reuse moves the home route at concurrency 1 from 308 to
12,799 req/s; the greeting route at concurrency 32 moves from 672 to 88,385.
These are different routes/concurrency settings, not one scaling series. In 0164,
the generic keep-alive host is faster than Flask on those samples but below native
axum; a larger home handler costs substantially more than the short greeting.
Native axum is a ceiling, not a claim of equivalent scripting capabilities.

Sources: [0162 evidence](0162_website_gap_probe_evidence.md), lines 127–136;
[0163 evidence](0163_reusable_invocation_slots_evidence.md), lines 54–69;
[0164 evidence](0164_http_host_evidence.md), lines 107–117.

Notebooks and adapter workflows also have retained timings/parity evidence from
prior records, but these do not yet form a controlled runtime-architecture
comparison suite. Do not describe them as unmeasured applications. What is missing
is matched, representative coverage suitable for deciding the engine architecture.

### 2.4 Work the engine repeats, and who pays it

Main context-window work is about 26.87M whole-process instructions (0171/0174);
tracked allocation calls are 32,304 in 0168; matched constructor median is
2.323 ms in 0169. Those are different measurement kinds, not additive components
or a forecast of recoverable application time.

The retained instrumented inclusive Callgrind annotation
`rnx-bench/results/profile-paired-0178/diagnostic1/annotate-inclusive.p0-base.context.txt`
reports with_default_modules 84.86%, install_trait_impl 61.52%, install_meta 27.91%,
Names::insert 16.59%, context drop 13.33%, and ItemBuf type_hash 9.70% (lines
35–59). They overlap; they cannot be summed or read as exclusive removable costs.
0179's selected formatting edge is 18.02% inclusive Ir, not a native bound
([feasibility](0179_startup_feasibility.md), lines 50–56).

| Deployment path | When registration is paid | Application relevance |
| --- | --- | --- |
| Fresh script process | Each process | Part of total work; substantial for short scripts, amortized as execution grows |
| Resident host rebuilding per request | Each request | Demonstrated avoidable overhead in the 0162 probe |
| Resident host with reusable slots | Each slot initialization | 0163 already provides reuse while retaining fresh invocation state |
| Long-lived computation | Initial preparation | Its fraction depends on duration; the 100-ms kernel is not proof that it is negligible |
| Session/notebook | At preparation/rebuild boundaries, compilation per input | Compile/edit-to-result and extension changes need scenario-specific accounting; no once-only promise for every operation |

| Enabling mechanism | Question to settle | Contracts and evidence status |
| --- | --- | --- |
| Reuse prepared state | Can the application amortize immutable preparation while keeping fresh invocation state? | Implemented for the host in 0163; intentional native state and request-owned future cleanup have explicit boundaries |
| Cheaper eager registration | Does its removal matter to a chosen application's total cost? | 0172/0179 show instruction/allocation improvements and failed regression gates; inventory/conflict/error behavior must remain |
| Immutable base plus overlay | Can common metadata become shared/generated without rebuilding mutable indexes? | Architecture hypothesis, not a constructor switch; lookups, enumeration, extension/conflict timing, captured callbacks and features matter |
| Lazy registration | Can unused work be deferred without changing compile-time resolution or registration semantics? | 0179 rejected it as a local cut; dependency closure, names/macros/traits and first-use cost require a separate design |
| Reusable compiled units | Can unchanged source avoid compilation in the application's actual deployment? | Not established by 0168–0181; source/module/feature/ABI identities, native handlers, diagnostics, cache misses, trust and invalidation must be specified |
| Interpreter compilation isolation | Can useful VM edits stop reshaping unrelated hot code without unacceptable boundary costs? | Separate crate/codegen/inlining boundaries are hypotheses; 0174/0178 show why a global profile switch is insufficient |

No record measures the gain from an overlay, cached unit or compilation-isolation
design. Preparation must be charged where paid, including cold/first use and
invalidation. These mechanisms enable competitive applications; none earns an
independent sub-millisecond target.

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
unrelated code change. These establish that the allocation-heavy path can be materially improved: roughly
30–39% fewer numeric/range instructions and 96.46% fewer numeric allocation calls,
with the regressions beside those gains in section 2.1. 0171 likewise shows an
improvable store path, not a proven removable fraction of its sample share. This
is evidence for a stronger interpreter's potential, not permission to adopt the
rejected candidates.

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
| Dispatch / instruction layout | Reduce repeated decode, stack movement or generic helper work | Larger hot code, observed unrelated-source/code-shape sensitivity (2.2), trace/debug visibility, precise budgets | Gain on a mixed opcode corpus under a fixed build; diagnostic evidence identifying what work changed |
| Interpreter compilation isolation | Separate hot interpreter compilation from metadata/library edits | Crate/ABI/inlining boundaries, cross-boundary costs, build/install complexity; cgu=1 did not solve all regressions | Useful source perturbations leave unrelated hot behavior stable and targeted improvements remain useful under the same fixed build |
| Specialized bytecode | Fuse or type-specialize frequently executed sequences | Compiler/VM ABI, guards, fallback, branch targets, source mapping, exact charging | Less work per useful operation, bounded code growth and compile cost; semantic equivalence on failed guards |
| Value representation redesign | Make common primitive access and ownership cheaper across operations | Much larger ABI/unsafe/GC/borrow surface; Rust userdata and alignment/portability | A justified representation prototype plus differential/alias/deep-value checks before broad migration |
| Tiered native compilation | Remove interpretation overhead from hot, type-stable regions | Guards, deoptimization, exact logical-instruction charging, native calls, code memory, async slicing/resumption and debug integration | Correct fallback/exit paths, useful speed at realistic break-even lengths, and bounded compilation/code memory |

These are hypotheses, not ranked removable percentages. Source-level sample mass
cannot be added across overlapping/inlining-sensitive categories. An apparent
store hotspot is not proof that a representation redesign removes all its work.

### Stronger interpreter, specialized bytecode, or JIT

A stronger interpreter remains valuable as the universal fallback, cold execution
path and diagnostic reference, even if native compilation later succeeds. Retaining
one large compiler-generated dispatch function may retain the measured sensitivity
to unrelated edits until a controlled study demonstrates isolation. Machine code
emitted by a tier changes that exposure for emitted regions, but still depends on
compiler-generated helpers, guard/fallback stubs and code placement. A JIT is not
a blanket escape from frontend or compilation-layout costs. A
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
must preserve the exact logical bytecode-instruction charging contract at every
observable halt/exit, including guards, native calls and nested execution. Charging
a fused region once at entry is not silently equivalent: it can move the halt
point or side effects. Any different contract needs explicit separate approval.
Async code must retain owned continuation state across suspension, honor supported
resume/slice boundaries and cancellation, and release request resources on close,
error and unwind. This does not promise preemption inside synchronous native Rust
calls. Compiled execution must remain subject to those lifecycle/resource controls; charging fewer
operations, omitting checks or abandoning retained state is not a speedup.

### Fresh execution core: Rune language, Lua-inspired design

**Status of candidate C: active discussion with the user.** Their latest direction
is "I think we need to keep batting this around". Consider a fresh execution
core as a first-class alternative to evolving the existing VM. This is not a
commitment to rewrite the whole language implementation or to run Rune on Lua.
The user’s approximate code-size observation motivates an inventory, not a
conclusion that line count explains speed or determines replacement cost.

Candidate C would design values, instructions, frames, dispatch and the native
boundary together, learning from compact dynamic-language VMs. Cheap primitive
operations and calls, deliberate allocation and a small hot execution footprint
are design objectives. Fixed-width bytecode, tracing collection and a register
model are alternatives to evaluate, not decisions already made. Existing fork
main already has addressed operands in Copy, Return and Arithmetic
([inst.rs at the pinned base](https://github.com/nilket-com/rune/blob/bb8e69372353c50e271c9f115bc771c77aa6b83e/crates/rune/src/runtime/inst.rs));
"switch to registers" alone does not specify what changes.

**Scope boundary:** initially investigate reuse of parsing, AST/source handling
and diagnostics, with a separately specified lowering/IR boundary. Existing VM
bytecode may encode precisely the costs the fresh core is intended to remove.
Reusing it, or keeping every value/native ABI unchanged, cannot be a silent
requirement. Conversely, source compatibility does not imply binary, generated
binding or embedding compatibility. The inventory must distinguish reusable
frontend/tooling, replaceable execution state, library/protocol dependencies,
compiler lowering and host conversion APIs. No reuse percentage or project-size
estimate is claimed before that inventory.

**Ownership consequence:** an independently maintained core makes the fork
responsible for its runtime correctness, memory safety, performance, native ABI
and long-term compatibility. Upstream runtime fixes would need evaluation and
adaptation rather than automatic adoption. Shared frontend/tooling work and
selective upstream contributions may still be possible; we cannot assert that
all upstream exchange ends. The product decision is how much sustained independent
runtime ownership the user wants, which compatibility contract governs it, and
how divergence/release/migration is maintained. A finite test suite supplies
regression evidence; it does not by itself define all Rune semantics or establish
compatibility on untested programs.

**Memory management is the central contract question, not a slogan.** A tracing
script heap may permit cheaper value copies, but deferred finalization alone does
not preserve existing host-resource release, aliasing and borrow rules. A hybrid
of managed script objects and explicitly owned foreign handles/borrow guards is
an option, not a proven solution. Specify roots held in Rust, native reentrancy,
continuations, relocation/pinning, external memory accounting, identity, cycles,
release timing/order, allocation failure and cleanup after errors/cancellation.
The current Module/Any API and generated bindings may need a compatibility layer;
its measured costs and limits must be included.

Lua is inspiration, not an assertion that required capabilities are free or
absent there. Lua 5.4 supports allocation errors/custom allocators and scoped
closing variables; these are not automatically Rune’s same contracts
([official manual](https://www.lua.org/manual/5.4/manual.html), §3.3.8, §4.4 and
lua_Alloc). Our records do not causally attribute Lua’s speed to missing Rune
obligations. The question is whether the proposed core remains useful and fast
when it provides the obligations this product needs.

**Bounded feasibility slice, to be planned separately:** arithmetic and mixed
values, calls/returns, collections/aliasing and a native Rust-data path, plus
controls for exclusive/shared borrows, deterministic host-resource release,
fallible allocation, async suspension/resumption and budget halt. Include useful
application work and total costs as well as diagnostic kernels. Measure an
obligation’s cost only with an explicit matched contrast; do not add unrelated
probe differences into an application forecast. A fast integer-only loop cannot
settle the fresh-core decision.

Existing logical instruction-budget behavior must have a specified compatibility
mapping if the IR/bytecode changes; charging a new instruction count is not
silently the same contract. Any unavoidable observable change in semantics,
diagnostics, release or budget behavior is listed for a prospective decision,
not hidden as an implementation detail. Unsupported cases must be refused or
explicitly excluded; fallback to the old VM also needs a value/ownership boundary.

The study stops its selected design if required ownership/async/budget boundaries
cannot be represented safely, or if the bounded slice shows no useful benefit
under its predeclared application/cost criteria. Negative results narrow that
design, not every possible fresh runtime. Success establishes feasibility of a
subset, not full Rune compatibility, deployment readiness or permission to discard
the shipped runtime. Compare the cost of a new core with interpreter isolation
and specialization/tiering before selecting the architecture.

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

**Proposed sequence, for joint review and the user's acceptance or reordering:**

1. Define and measure a bounded representative suite for the four application
   families, with pinned inputs, exact outputs, competitor roles/capabilities and
   deployment paths. Reuse suitable application artifacts; freeze unmeasured
   scenarios before variants. Its information goal is the application-level gap
   and what fraction is Rune work versus preparation/native kernels.
   Where feasible, propose a Rust-hosted Lua version using the same native
   kernels and equivalent useful inputs/outputs as an application reference.
   Its budget/cancellation, ownership checks, release timing and host-boundary
   differences must be named, and unsupported cases must remain visible. This
   can help determine whether changing the scripting runtime would materially
   affect these applications before designing candidate C. It is not a
   mathematical performance ceiling, a forecast for a core preserving Rune’s
   obligations, or an isolated causal contrast for one runtime mechanism. A
   modest application gap weakens the case for a rewrite for that application;
   a large gap strengthens motivation but does not prove feasibility. No ratio
   threshold selects/rejects a core here. This peer is part of the step-1
   proposal, not approval to implement or measure it.
2. Run 0183 in parallel under its separately reviewed bounded plan. Its information
   goal is localization and, if feasible, one predeclared causal intervention.
   An unavailable method or failure to identify an admissible region closes that
   path; no placement search. It does not block the following source-design work.
   The separately planned study may consider switch-penalty counting alongside
   precise retired-DSB-miss localization; availability, semantics and the inference
   limits must be reviewed before any event opens. The pinned
   [Intel table](https://github.com/intel/perfmon/blob/78eb739dafa28c1b296f7b4d5fb7e1a7e81b1537/ADL/events/alderlake_goldencove_core.json)
   (SHA-256 d588ba821297c4097705214707b2f7d52ed173ec6a4ad9acf0fc3649e6e43a0f)
   defines DSB2MITE_SWITCHES.PENALTY_CYCLES (61/02) and precise
   FRONTEND_RETIRED.ANY_DSB_MISS / DSB_MISS (c6/01, MSR 3F7 values 1 / 11,
   TakenAlone=1). These are source definitions, not collected results; 0181
   did not measure the switch-penalty event. Counts comparable in magnitude to
   excess cycles would remain a compatibility check, not an additive causal
   decomposition. This proposal authorizes no event selection or measurement.
3. Review bounded feasibility studies for interpreter compilation isolation,
   specialization/tiering, and a fresh execution core (candidate C above). Freeze each scope, capability subset, cost envelope and
   stop rule before execution. Isolation must establish a stable compilation
   boundary under predefined perturbations and preserve local gains; failure
   closes that method. Tiering must establish correct guard/fallback/budget/owned
   state behavior and a useful total-cost break-even; unsupported required exits
   or no useful application benefit closes that subset. The fresh-core study
   must establish the language/lowering/host compatibility boundary and the
   multi-obligation vertical slice above. No second candidate is silently tried
   after results. Their order and shared evidence should avoid three overlapping
   implementation projects; each starts with its own reviewed scope.
4. Choose the architecture using those results: stronger isolated interpreter,
   specialization/tiering, a fresh core, or a scoped combination. Record migration/maintenance
   costs and the product adoption boundary. A feasibility pass alone does not
   switch shipped rnx or waive an existing gate.

Meanwhile, propose no further isolated VM micro-optimization cuts under the
unchanged structure merely to accumulate local gains. The campaign is reassessing
how to land durable application improvements. The sequence above needs acceptance
before its measurement/implementation records; this draft authorizes none.

**Roadmap revision proposed:** withdraw fresh `rnx eval 42` <1 ms as a campaign
goal, consistent with the user's explicit correction. Retain its measurement as
one diagnostic of repeated cost. Treat "within 2x of contemporaneous PUC Lua" on
numeric/fib/strings as provisional diagnostic reference points, not the definition
of competitiveness. The user should explicitly accept, replace or reject that
kernel target when approving the application criteria. No historical result or
frozen experiment threshold is rewritten.

Decisions to expose to the user after joint review:

- Are the application scenarios and proposed sequence the right expression of competitiveness, and should the roadmap kernel target be retained only as a diagnostic?
- Which architectural feasibility cut offers the highest information value for those
  applications, with stated cost and semantic scope?
- Do we retain current diagnostic performance gates for that cut, or explicitly
  approve a prospective application-priority trade-off? The latter is not assumed.

Codex and Claude agree on this proposal. The recommendation remains provisional
pending the user’s decision on the sequence and application criteria. This draft authorizes document work only. Shipped rnx remains
Rune 0.14.2; fork main remains unchanged; Lua remains an additional experiment.
