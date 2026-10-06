# rnx 0176: outlined integer range dispatch

**Status:** plan for Claude's review; no engine edits or new measurements. Codex owns engine implementation and evidence; Claude owns measurement tooling and reviews the engine. The user authorized the Rune competitiveness campaign. This is a new bounded experiment following 0175's STOP, not a reinterpretation of that result.

## 1. Hypothesis and sources

0175 removed 96.46% of the numeric workload's allocation calls and improved range wall time about 40–50%, but failed the frozen no-regression gates. Fibonacci regressed 8.56% instructions / 8.21% wall; calls regressed 3.96% / 4.94%. Startup instructions also rose. These measurements establish the regression, not its mechanism.

**H1:** moving handler resolution and the guarded range operation out of the VM dispatch arms reduces the candidate's cost on unrelated calls without losing its range improvement. Increased hot-loop code size, inlining/register allocation and extra predicates are competing mechanisms to inspect, not assumed causes. The default-profile startup shift remains a separate code-generation hypothesis. Outlining may fail to solve either.

Pre-edit reviewer observation: the frozen fib and calls inputs invoke named script functions; the compiler is expected to emit plain Call/op_call rather than CallFn/CallAssociated. If confirmed by bytecode or source evidence, their regression cannot be attributed to executing the newly added rejecting predicates on those calls. Code size, inlining, layout and register allocation remain hypotheses. Even perfectly outlining the accepting work leaves some hot classification and can still perturb the enclosing dispatch code. The diagnostic includes symbol-table byte sizes for run (or its enclosing symbol) and op_call across all three binaries, with absent/inlined symbols reported explicitly.

Source identities:

- Development fork main stays `bb8e69372353c50e271c9f115bc771c77aa6b83e`.
- Semantic/performance baseline: `eaa59fc208c136ead86f8c4fa565431ea18de88b`, production equivalent to `3e7d4da9ce908eeb7e0e1ac119dec24f68d5449a`, including all 0175 base-first tests.
- Previous candidate: `863370da279031b369ebb0ac1c0d80cd9ca159f6`, retained for diagnostic comparison only.
- New candidate starts from that previous candidate; the new production diff may only outline its range dispatch work. Final candidate hash is committed before any diagnostic execution or deciding measurement. No compiler, bytecode, handler registration, memory-limit bridge, profile or public API change.

## 2. Design checkpoint before source edits

One private `#[inline(never)]` helper owns all accepting work: unit/context handler resolution, native Function tag inspection, all existing range eligibility guards, argument consumption, typed next, the second budget charge and ordinary materialization/fallback. A small private carrier distinguishes associated dispatch, inline-Type callable and native Function callable; it is not a public Value or bytecode. No heap allocation is added for the carrier.

The hot arms retain rejecting prefilters only:

- Associated calls: `hash == Protocol::NEXT.hash`, arity one and diagnostics absent before entering the helper. Resolution remains context-first.
- Inline-Type callable: classify the existing callable value, compare its hash to the existing range-NEXT hash, then arity/diagnostics gates. The helper checks unit-function absence before context lookup/tag proof.
- Any Function callable: retain the cheap callable type classification plus arity/diagnostics gates; the helper performs the native-handler borrow and tag proof. An unrelated native Function may enter the helper and reject. We do not claim all CallFn operands permit a single hash comparison.

The helper's acceptance still requires the resolved handler's private tag; hashes only reject. All rejection occurs before iterator advancement/argument consumption, except the existing admitted borrow error, whose consumed-argument and BadArgument behavior stays identical. The ordinary fallthrough and VM halt propagation stay unchanged. Borrowed native Function state must not acquire a new lifetime spanning mutation or survive fallback.

Do **not** mark the helper `#[cold]`: it is called at every admitted range iteration, not only exceptional events. `#[inline(never)]` is the sole new code-generation directive in this cut. No second variant is selected after viewing diagnostics/results. If implementing the single helper needs broader semantics changes, stop at feasibility and seek a reviewed plan amendment.

First submit an exact proposed helper signature, operand ownership/borrow audit and dispatch diff outline to Claude. Confirm that 0175's positive-hit counters and raw-boundary goldens keep their meaning. Only after that checkpoint is accepted may engine source change.

## 3. Semantics and allocation, unchanged

Re-run all 0175 base-first goldens and controls verbatim on the new candidate, plus the full base/candidate suites, named non-tracing positive-hit configuration, tracing fallback and no-std check. Keep the stdio on/off inventory and its negative control. Preserve unit-function shadow precedence, custom handlers, diagnostics/tracing spans, raw stack states, explicit-limit scopes (including usize::MAX), one-permit halt/resume and failure ordering. No unsafe code is added. No test is deleted, loosened, normalized further or regenerated to fit the candidate.

Reuse the 36 origin-qualified corpus cases plus the four range/manual-next fixtures, with only the two historical diagnostic normalizations. Separate counting builds must preserve exact outputs and expected budget statuses, the zero/default/unlimited/tight-halt controls, independent manual-next Options and the exact +1 temporary context-registration qualification (zero for floor/empty-context). Numeric allocation reduction still must be at least 90%. Every earlier source/test limitation is stated again; physical allocator-failure points differ when allocations disappear, but explicit allocation-limit scopes never specialize.

Tracing-enabled and no-std builds never specialize. The named non-tracing tests must show positive hits and fallback coverage. Measured Rune features stay `alloc,anyhow,fmt,serde,std`; verbose build receipts prove the effective set. All-feature tests are not substitutes for positive-path tests.

## 4. Pre-registered dispatch diagnostic

Before deciding PMU/wall samples, after committing the candidate and passing semantics, retain Callgrind instruction-event attribution for three freshly built binaries: semantic baseline, 0175 candidate, and the one outlined candidate, all at the same default release profile and equal build path. Workloads are exactly context, run-fib, run-calls and run-numeric; one run per binary/workload. No optional fourth binary in this record.

Claude owns the diagnostic script. Use the installed user-local Valgrind 3.26.0 with its explicit VALGRIND_LIB path; diagnostic environment is E0 plus that one required safe variable. Retain version, full argv/environment, statuses, source/executable hashes, raw callgrind.out and both inclusive and exclusive callgrind_annotate tables. Every diagnostic run is bounded, under the shared lock, with partial output retained before checks. The script and exact commands are committed and reviewed before it runs.

Callgrind Ir is the tool's instrumented instruction-event count, not native hardware instructions:u or a wall-time result. Function attribution can expose where the totals accumulate; merged/inlined symbols or unavailable line attribution must be reported as limits. A function's increase alone does not prove which dispatch arm caused it. A failed diagnostic is reported as inconclusive, not silently replaced or used to revise the candidate. These descriptive counts do not substitute for any deciding PMU gate.

Static disassembly is secondary: locate Vm::run and the outlined helper through available symbols/direct-call addresses, retain command/version/hash receipts, and describe size/layout, calls, branches and observable spills. If run is inlined, identify its compiled enclosing caller rather than relabel another function. No further perf sampling variant is added in this record.

The startup hypothesis and dispatch hypothesis remain separate. Given 0174's profile dependence, startup instruction gates may still STOP this record even if fib/calls improve. A startup-only STOP feeds a separately planned code-generation/profile study; any shipped profile change requires user input. No candidate revision, profile substitution or gate change follows the diagnostic here.

Build disclosure: the old 0175 driver worktree and its subject binaries were deleted after closure. Rebuild the 0175 diagnostic candidate from 863370da and record its new hash against the old measured ec6962fb identity; do not imply it is the retained old executable. The baseline's old measured identity is 7a66b042; verify/report reproducibility. New builds receive fresh receipts. Diagnostic comparison uses the rebuilt binaries; deciding comparison remains baseline versus the single outlined candidate.

## 5. Frozen measurement and decision

Claude creates `probes/range-iteration-0176` from reviewed 0175 driver `640a792d` and its repaired lifecycle/retention controls. Differences are limited to record paths/names, the candidate SOURCES constant, fresh build/manifest receipts and the new Callgrind diagnostic script; any other change requires explicit review. Use a new record-specific manifest binding the baseline, new candidate, binary/build/feature hashes, inputs, observer and inventory receipts. No silent changes to the measurement algorithm. Claude authors the manifest/tooling in an isolated worktree; Codex reviews the committed driver and untimed rehearsals before an official run.

Keep the default release profile, one fork worktree path, independent cleaned target artifacts and staged executable path. No cgu/LTO substitution. The 18 historical workloads plus signed-range, negative-range and range-while controls stay separate. Historical references and the 2% applicable reproduction gates are unchanged; tiny FIFO floor/empty windows remain descriptive. Contemporary comparisons for new controls use the baseline, not historical results.

- PMU: five true ABBA repetitions, ten samples per side/workload, `instructions:u` and cycles, core 4, at least 99% running; every raw sample retained before parsing/gates.
- Wall: pinned 0169 resident observer, fresh process per sample with pipe capture, three ABBA/BAAB/ABBA rounds of five-sample blocks, 30 per side. Preserve 0175's declared absence of workload warm-up blocks. Hyperfine `-N --output=pipe` calibration within 0.15 ms first; no clock offset or process-floor subtraction.
- Counting builds remain separate from timing subjects. Exact ordinary outputs and budget completion/halt statuses are gated. Context +1 is an exact qualification, not an allowance for unrelated changes.
- Lua references remain descriptive complete process-plus-workload windows; no generalized ranking or startup attribution.

Decision precedence is carried unchanged from 0175:

1. Correctness, safety, calibration, historical reproduction or retention failure: **STOP**.
2. Any workload's instruction median regression >0.5%, wall median worsening beyond its own base p10–p90 width, or opposing instruction/wall changes both >3% outside those bands: **STOP**.
3. Otherwise **WIN** requires at least 10% fewer instructions on original numeric **and both admitted range controls**, plus at least 90% fewer numeric allocation calls, with every unrelated workload passing its gate.
4. Otherwise **NO-WIN**.

In particular, persisting startup instruction regressions STOP under the default profile. Fib/calls instruction regression above 0.5% also STOP; if outlining still fails that condition, park this implementation approach pending a separately reviewed dispatch redesign. No automatic further inline/cold/profile variants, tolerance adjustment or gate averaging. A STOP does not erase the allocation finding.

## 6. Security, lifecycle, integration

All heavy compilation, tests, diagnostics and measurements hold `/tmp/rnx-runtime-bench.lock`. Minimal fixed subject environment; never serialize inherited os.environ. Replay the fake-secret sentinel controls across ledgers, outputs, failures and archives before the official run. Pre-push known-value scans occur in memory and print paths/counts only. Credential rotation from the earlier incident remains separately unconfirmed.

Every command has explicit process-group ownership, deadline, bounded cleanup/reap and a post-retention lifecycle gate. FIFO acknowledgement padding repair and nonblocking deadline reads stay in place. Raw failed attempts and untimed controls are retained. Any failed official attempt stops; repairs/replays need prior review, with thresholds/source/policy unchanged. Publish the outer launch command/status/safe environment and clean-tree source identity.

Codex owns a new 0176 engine branch and generic rnx evidence; Claude owns measurement tooling on a separate branch. No cross-amends. Tests precede any new performance commit; previous candidate history stays intact. Serial integration preserves both authors' commits.

On STOP/NO-WIN, publish evidence and the named non-main candidate branch only. On WIN, both reviewers must accept semantics, attribution limits and retained gates before a proposed fork-main integration and unfiled upstream draft. Shipped Rune 0.14.2 and rnx remain unchanged in this record; adoption would require its own reviewed integration.
