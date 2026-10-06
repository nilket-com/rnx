# rnx 0179: capability-preserving fresh startup

**Status:** plan for Claude's review. Codex owns feasibility, candidate implementation and independent reconstruction; Claude reviews and owns the driver and deciding gates. Startup is the user's next campaign cut. Gates remain unchanged; stopped range candidates remain stopped. This is an isolated experiment on fork main bb8e69372353c50e271c9f115bc771c77aa6b83e, not a product migration or a profile decision.

## 1. Question and boundaries

Can complete standard-library registration be prepared more cheaply while preserving the capabilities and extension behavior of a fresh Context? Preparing once in a long-lived process and amortizing later calls is a different result from improving a fresh CLI invocation. Every fresh deciding subject includes any one-time initialization it requires. No warm singleton, omitted module, literal-only shortcut, process-floor subtraction or hidden first-use work earns a startup gain.

0169 measured stock `rnx eval 42` at 4.173 ms, an historical product reference; the campaign target is below 1 ms. Main's engine harness is a separate subject: the blocked migration remains blocked. 0169's complete main attribution places 1.503 ms in trait-implementation installation, with iter/ops inclusive factory+installation intervals of 0.504/0.477 ms. Those intervals overlap and are not additive removable bounds. September record 0030 avoided constructing a context for help/version but left eval unchanged. Its lazy host construction is not proof that lazy standard-library lookup preserves compilation or execution semantics.

No engine edit or candidate build precedes the accepted plan and feasibility checkpoint. Read-only source analysis may proceed after plan acceptance. No changes to compiler, Cargo profile, shipped rnx dependency, budgets or cleanup semantics. Fork main remains untouched unless a candidate earns an accepted WIN; any product adoption remains a separate record.

## 2. Step 0: feasibility, before an engine edit

Audit these four shapes separately against the pinned source and retained September/0169 notes:

| Shape | Required proof and cost accounting |
| --- | --- |
| Lazy module installation | Compilation lookup, prelude imports, traits, macros and diagnostics must still see every required name. Dependency and trait realization order must remain identical; custom installation cannot resolve conflicts differently. Charge cold resolution and execution, including all deferred modules, to first use. |
| Static or compile-time registration tables | Determine which metadata, hashes and handler factories can be generated without runtime allocation or trait expansion. Native pointers are relocated normally, never serialized addresses. Prove feature/stdio variants, deterministic order, architecture/version compatibility and fallible mutable overlays. Build time and binary size are reported. |
| Trait default-method expansion | Eager sharing must keep the inventory and installation order byte-identical while removing repeated handler/metadata preparation. Entry-level deferred expansion must preserve hash/name lookup, metadata/doc enumeration, conflict detection and override precedence against later user installs; charge misses to first use. Audit both separately and state which part of the measured trait stage can actually be removed, rather than crediting the entire interval. |
| Shared prepared Context or immutable registry | Distinguish immutable metadata from per-context extension state and handler captures. Prove independent custom modules, conflict checks and drop/resource ownership. Include first-process preparation and any copying; cross-request sharing cannot leak mutable host state or replace fresh construction with an already warm context. |

For each shape identify edits confined to registration versus edits to hashing, B-trees, Value/Repr drops or other code shared with VM hot paths. 0172's stopped eager preparation experiment is prior evidence of compiler sensitivity, not proof this family is safe.

Trace all Context fields and public lookup/install/runtime paths, module dependencies, trait-handler callbacks and macro/constant construction. Include stdio true and false, private collection modules, doc/emit feature paths and alloc/no_std availability. Preserve registration order, not just a sorted final inventory. State which allocation failures may move; do not promise identical allocation-failure sites.

Deliver a source-linked feasibility table, proposed storage/ownership layout, exact edit surface, first-use workload design and test inventory. One shape at most proceeds. Selection must be justified structurally before any candidate performance result, not by trying variants until one wins. Claude reviews this checkpoint and the precise source/test amendment before edits. If none can preserve capability, close **FEASIBILITY STOP** with no candidate. If preserving capability requires an API/semantic relaxation, stop and report it rather than implement it implicitly. Merely proving reuse helps a warm process does not satisfy fresh startup feasibility.

## 3. Tests first and implementation checkpoint

After the feasibility design is accepted, commit tests on the unmodified base before the candidate. Reuse 0172's exhaustive registration inventory and refusal controls, with a documented adaptation to this design. Inventory includes every function hash, metadata kind, constants, names, crates and relevant trait/associated registrations; opaque entries fail closed, handler identity is structural and behavioral rather than raw pointer equality. Cover stdio true/false. Record a deterministic installation/event order trace as well as sorted inventory, so changed trait resolution order cannot hide behind equal sets.

Exercise user modules installed after defaults: new functions/types, automatic trait methods, name/hash conflicts, duplicate and missing-container errors, runtime creation before/after extension, independent contexts and repeated construction/drop. Include observable macro/import/diagnostic behavior, custom handler captures and any sharing invalidation boundary identified at feasibility. Tests must demonstrate isolation and no leaked retained execution state. A lazy design gets an explicit all-default-modules first-use fixture, including macros/traits and private collection functionality; its output and complete touched-module coverage are pinned on the base.

Base and candidate pass the full Rune suite, non-tracing production-feature controls, no_std checks, the standing 40 correctness fixtures with only existing normalizations, and any new first-use fixtures. Run inventory under both test features and the actual production harness configuration; untested feature combinations are named, never credited. Corruption controls remove/change a function, constant, metadata entry and ordering event and must fail.

Only the reviewed edit surface may change. No opportunistic VM, allocator, inlining, profile or handler-signature edits. Freeze candidate commit, source diff and all artifacts after correctness. A source change required by a failure returns to review before any replay. Memory and allocation counts cover fresh construction/drop and first use; resident shared tables are reported separately, never counted as free memory.

## 4. Deciding measurement, frozen before a candidate

P0 only: Cargo default release, opt-level 3, default 16 CGUs, lto=false with possible crate-local ThinLTO, panic=unwind. Pin rustc 1.98.1, safe build/runtime environments, lockfile, measured Rune features and harness ancestry from 0178. Fresh base and candidate builds use identical paths/settings and three artifact roles (primary/counter/allocation). Retain verbose compiler commands, source/compiler/binary/.text hashes and sizes. First-use coverage and new harness operations are frozen at the reviewed feasibility checkpoint before any candidate timing.

Reuse 0178's reviewed native PMU and resident wall driver algorithms: core 4, instructions:u/cycles, at least 99% running; five ABBA repetitions, ten PMU samples per side/workload; three ABBA/BAAB/ABBA rounds, thirty fresh-process wall samples per side/workload. Calibration is Hyperfine -N --output=pipe within 0.15 ms, no offset correction. Same-profile fresh baseline supplies every wall band. Applicable historical instructions references are pinned by workload/path/hash before running and retain the 2% reproduction gate; historical tiny floor/empty FIFO windows remain descriptive, as agreed in 0172. New fixtures have contemporary paired baselines only.

Report startup separately: floor, empty-context, context, runtime, compile-answer, run-answer and run-empty. Include all 0178 unrelated execution workloads, including ordinary/manual range and signed/negative controls, without installing stopped range candidates. Add the frozen full-capability first-use fixture as a deciding workload even if the design is not lazy. A lazy implementation's context-only savings are reported beside complete cold construction+compilation+first execution; a workload touching every deferred module must pass the same regression gates. Repeated/warm context calls may be diagnostic only and cannot decide a fresh-process WIN.

The stock `rnx eval 42` reference and below-1-ms target are reported as product context, not as an achieved result from the main harness. No scratch main port is silently introduced. Report complete engine fresh-process times and any bounded in-process phase observations with their distinct scopes. Allocation and peak-memory passes run outside timing. No extra diagnostic variant/profile is selected after results.

Decision precedence:

1. Correctness, safety, provenance, calibration, applicable reproduction or retention failure: **STOP**.
2. Any workload's instruction median rise above 0.5%, wall worsening beyond its own base p10–p90 width, or opposing instruction/wall changes both above 3% outside their noise bands: **STOP**.
3. Otherwise **WIN** requires at least 10% fewer whole-process instructions in context **and** run-answer, with no regression in the full-capability first-use or any other deciding workload.
4. Otherwise **NO-WIN**.

The target below 1 ms is not the minimum improvement criterion and may remain unmet after a WIN. No averaging, net-benefit trade or exception overrides a failed workload. If 0172's unrelated 0.5–0.7% shifts recur, retain them and report STOP, even with large startup gains. Results locate a cost; no causal compiler explanation follows without evidence.

## 5. Driver review, ownership and stop discipline

Claude commits the exact adapted driver, immutable workload manifest and untimed rehearsals for Codex's review before official execution. Controls cover changed pins/inventory/order, altered profile or artifact role, missing/duplicate/non-finite samples, wrong outputs, first-use omission, mandatory cold initialization, scientific STOP versus infrastructure failure, and bounded process cleanup. Raw records precede checks; failed attempts and approvals remain retained. A semantic failure stops before deciding samples. Any plumbing repair/replay needs prior review, disclosure and unchanged design/workloads/gates.

All heavy builds, suites and measurements hold /tmp/rnx-runtime-bench.lock; deadlines and clocks begin after admission. Retain explicit safe environments, command/status/process-group/reap/survivor records, staged binary identity and before/after load. Never serialize inherited environment or credentials. Replay fake-secret success/failure/retention controls and the existing known-credential scan without printing secrets. Earlier credential rotation remains separately unconfirmed. Use existing bounded build/measurement phase limits; review any required extension before starting, never silently extend a running phase.

One official paired run after reviewed controls. Codex reconstructs the decision independently from retained samples and reviews every capability/cold-cost claim. Generic rnx plan/evidence, bench probes/results and an isolated fork candidate branch, integrated serially preserving each author's commits. No website-specific content, public upstream filing, deployment or product dependency change. On STOP/NO-WIN retain the candidate without merging fork main. On accepted WIN, jointly review the fork integration separately; no product shipping follows automatically.
