# rnx 0177: range fast-path callsite neutrality

**Status:** plan for Claude's review, before new analysis tooling, engine edits or execution. The user chose the next cut: an inlining-neutral range fast path under the same frozen gates and default profile. Codex owns the source investigation, engine and evidence; Claude owns measurement tooling and independently reviews checkpoints. “Inlining-neutral” names the design objective, not a proven property or permission to change a compiler profile.

## 1. Hypothesis and immutable identities

0176 closes as STOP: numeric/range gains and 96.46% fewer numeric allocation calls coexist with unrelated instruction and wall regressions. Its smaller Vm::run symbol does not cure fib/calls. Exclusive Callgrind attribution redistributes work into drop glue and other compiled functions; Repr drop symbols exist in both binaries. Neither thresholded tables nor changed function boundaries establish a causal mechanism.

**H1:** if retained call edges identify a concrete new fast-path caller of an ownership/drop routine also used by unrelated execution, restructuring that caller through an existing equivalent ownership boundary can preserve range gains without the unrelated regressions. A changed caller may affect inline-cost choices; single-caller/last-call bonuses, layout and register allocation are hypotheses, not conclusions. This record must not force inlining globally or label every code-generation shift “inlining.”

Pinned sources and retained evidence:

- Fork main: `bb8e69372353c50e271c9f115bc771c77aa6b83e`, unchanged.
- Semantic/performance baseline: `eaa59fc208c136ead86f8c4fa565431ea18de88b`, production equivalent to `3e7d4da9ce908eeb7e0e1ac119dec24f68d5449a`.
- 0176 candidate: `6f54bd32ee1038e3927ef384d1a914d8d5b36c4f`, the starting source for a possible candidate, diagnostic-only comparison subject.
- rnx 0176 closure: `69222e25d11011984f5f5aacfd5db1964fcf4a6f`; bench closure: `b66d596ce2301447418c445a00de328db2549fcf`.
- Retained callgrind, command ledger, annotation, nm and objdump files: `results/range-iteration-0176/diagnostic1/` in that bench commit. Raw Callgrind files and symbol tables are losslessly xz-compressed.

No newer upstream base, profile change, gate adjustment, shipped dependency change or automatic series of source variants.

## 2. Step 0: retained-edge feasibility, no subject execution

Create a separately retained, read-only analysis of the existing base/candidate fib, calls and context profiles. Numeric may be inspected only as the already-frozen positive control for whether the fast path appears in the call graph. No new interpreter run, compilation, PMU sample or wall sample in this step. Public callgrind_annotate caller trees or a checked parser may transform retained files; retain tool version/argv/status, input hashes and complete outputs. Any tool/parser failure stops analysis until a reviewed plumbing repair; it is not an absent edge.

Investigate exactly these targets: Repr drop glue, Value drop glue, pop_call_frame and B-tree infallible_cmp. Starting from the target, retain caller edges, call counts and edge Ir (inclusive costs attributed to that edge), plus its exclusive Ir and total program Ir. Keep candidate/base differences separate. Preserve caller identity/address/file where available and document aggregated or ambiguous names: nm shows multiple same-named Repr symbols, so a demangled name alone is not a unique machine function. Validate compression/name references and nonnegative counters, and match totals to the independently audited 0176 diagnostic. Use small synthetic callgraph fixtures before trusting parser-derived edges. Inclusive costs cannot be added to exclusive totals or summed across recursive paths as independent work.

Cross-check claimed new caller sites against full retained disassembly and the pinned source. Specifically inspect take(), into_mut error conversion, Option store/drop paths and range guard drop as possible fast-path origins; none is presumed responsible. Source locations absent from Callgrind stay absent. If existing disassembly does not cover a proposed caller, source alone may describe a possible caller but cannot prove that machine edge. Do not rebuild or regenerate disassembly from a differently compiled executable and pass it off as the retained binary.

Competing explanation to name explicitly: codegen-unit repartitioning or layout can change unrelated caller/callee visibility when items are added, independently of executing the fast path. Inspect the signature in context construction (which executes no range path) and op_call/pop_call_frame ownership edges. 0174's cgu=1 observation is consistent with this mechanism, not proof of CGU assignments. No nightly compiler, print-mono-items, rebuild or profile experiment is added. If the changed hot edges have no evidenced fast-path caller and retained data support only partition/layout hypotheses, close at FEASIBILITY STOP; a profile/partition follow-up requires user input.

**Feasibility checkpoint to Claude, before an engine edit:** report the observed edges and attribution limits; identify one concrete ownership/drop boundary and an exact single proposed transformation, or declare inconclusive. A candidate requires evidence binding a fast-path caller to an identified compiled ownership routine and a source design that can avoid or reuse that caller without changing semantics. A difference on unrelated hot edges alone, or a guess about LLVM bonuses, is insufficient. This evidence does not prove the compiler's decision or causation; the later experiment tests the operational hypothesis.

If identities/edges cannot be resolved enough to support a concrete design, or the only apparent fix needs a VM-layout/value-representation/public-API redesign, close 0177 as **FEASIBILITY STOP**, publish the analysis, and do not run a speculative candidate. Do not silently replace this checkpoint with another optimization.

## 3. Step 1: one reviewed candidate

Only after joint acceptance of the feasibility/design checkpoint may source change. The checkpoint fixes the exact edited functions/files, helper signatures, operand ownership, borrow lifetimes, drop timing and error exits. The permitted change is a local restructure of the 0176 fast path and reuse of an existing equivalent native-call ownership boundary. vm.rs is the preferred boundary; touching another runtime file requires an explicit plan amendment accepted before editing it. No forced-inline attributes on shared Value/Repr/drop routines, blanket code-generation directives, new unsafe code, leaked values, suppressed destruction, allocation-limit bridge changes or hidden pending VM state.

Preserve the 0176 private tag proof, context-first associated lookup, unit-function precedence, native-function alias behavior, one argument, diagnostics/tracing guards, receiver/output/lookahead qualification and refusing before mutation. Preserve argument take and BadArgument wrapping, typed next, range guard lifetime, two real instruction-budget charges, failed-second-permit ordinary Option store, terminal None and subsequent IterNext, ip/last_ip_len, raw stack states and ordinary fallback/halt ordering. Reusing a drop boundary must not weaken dynamic borrow checking or change which value is destroyed on success/error/unwind. The checkpoint explicitly explains why those contracts survive the restructure.

All 0175/0176 base-first tests and raw-boundary goldens stay byte-identical, as do positive-hit counter meanings. No recapture, deletion, additional normalization or fitting expected outputs to a candidate. Run base/candidate all-feature suites, named non-tracing positive/fallback cases, tracing fallback and no-std alloc checks before diagnostics. Counting controls retain explicit limits including usize::MAX, budget resume, unit shadowing, custom handlers, diagnostics and stdio on/off inventory with its negative control. The 36 origin-qualified corpus cases plus four range/manual-next fixtures remain frozen. Only the two existing historical diagnostic normalizations apply.

Commit and review the exact candidate hash after semantics, before any diagnostic execution or deciding timing. Record any source/test attempt and repair; superseded results do not certify the final source. No candidate revision after viewing its diagnostic or performance data. If the checkpoint transformation cannot be implemented without changing those contracts, stop at feasibility.

## 4. Step 2: pre-check diagnostic, then unchanged deciding run

Claude reuses the reviewed 0176 driver, with record paths, source constants, fresh manifest and build receipts only; other differences need prior review. Build baseline / 0176 / the one 0177 candidate at the same fork path with independent cleaned targets, default release profile and Rune features `alloc,anyhow,fmt,serde,std`, no tracing. Counting and timing builds stay separate. Report reproducibility against baseline `7a66b042…` and 0176 `3461f953…`; fresh builds are not claimed to be the retained old executables.

Run the same Callgrind pre-check once: context, numeric, fib and calls for those three subjects, installed Valgrind 3.26.0 with E0 + explicit VALGRIND_LIB, under the shared lock. Retain raw events, inclusive/exclusive tables, caller-edge analysis and the full command/status/lifecycle ledger. Retain nm symbol sizes and full run/op_call/helper disassembly exactly as in 0176. Diagnostic Ir remains distinct from instructions:u. Report whether the selected caller restructuring is observable, with identity limits. If the intended transformation cannot be observed, or diagnostic execution/retention fails, stop and report; do not infer success from an attribute or revise the source. Numeric diagnostic differences do not select another candidate. No extra PMU diagnostic or profile variants.

After passing pre-check, the deciding run is exactly 0176's:

- The same 18 historical workloads plus signed-range, negative-range and range-while. Baseline is eaa59fc2; 0176 is diagnostic-only.
- PMU: five true ABBA repetitions, ten samples per side/workload, instructions:u and cycles, core 4, at least 99% running; raw data retained before parsing or gates.
- Wall: pinned 0169 resident observer, fresh process and pipe capture, three ABBA/BAAB/ABBA rounds with five-sample blocks, 30 per side. No added workload warm-up blocks, clock offsets or process-floor subtraction.
- Hyperfine -N --output=pipe calibration within 0.15 ms; unchanged applicable historical reproduction gates at 2%, tiny FIFO floor/empty windows descriptive.
- Correctness, allocation, exact budget statuses and temporary context registration qualification (+1, floor/empty zero) unchanged; independent manual-next Options remain ordinary. Numeric allocation calls must fall at least 90% for a WIN.
- Contemporary Lua references remain descriptive complete process-plus-workload windows, not generalized language ranking or startup attribution.

Decision precedence is verbatim:

1. Correctness, safety, calibration, historical reproduction or retention failure: **STOP**.
2. Any workload's instruction median regression >0.5%, wall median worsening beyond its own base p10–p90 width, or opposing instruction/wall changes both >3% outside those bands: **STOP**.
3. Otherwise **WIN** requires at least 10% fewer instructions on original numeric and both admitted range controls, plus at least 90% fewer numeric allocation calls, with every unrelated workload passing its gate.
4. Otherwise **NO-WIN**.

**Kill criterion:** if fib or calls instructions still regress >0.5%, STOP and park the range fast path entirely pending a separately reviewed upstream VM-layout change. No automatic new inline/cold/drop-body variants, profile substitution, gate averaging or tolerance widening. Startup instruction regressions still STOP even if the selected caller is neutral; a profile decision requires user input. A negative result retains its allocation/semantic findings without adoption.

## 5. Lifecycle, security and integration

All heavy compilation/tests/diagnostics/measurements hold `/tmp/rnx-runtime-bench.lock`; read-only retained-file analysis executes no subjects. Fixed safe E0/EB only; never serialize inherited environments. Bind source/binary/tool/input/build-feature hashes and outer launch status. Commands have explicit process-group ownership, deadlines, bounded cleanup/reap and survivor checks. Keep raw failed attempts, partial outputs and untimed controls. Fake-secret sentinel controls cover output/ledger/failure/archive paths before official execution. Known-credential scans run in memory and print counts/paths only; prior exposed credential rotation remains separately unconfirmed.

Official runs are once only. Failure stops; any plumbing repair/replay needs prior review and disclosure with source/gates unchanged. No environment or timer change is a silent “fix.” Publish safe receipts and actual attempted-run counts.

Separate worktrees/branches; Codex engine/evidence and Claude driver ownership, no cross-amends; serial integration preserves both histories. On feasibility STOP, publish analysis/evidence only. On candidate STOP/NO-WIN, publish evidence and a named non-main candidate branch only. On WIN, joint semantic/retention review precedes any proposed fork-main adoption. Shipped Rune 0.14.2 and rnx remain unchanged in this record. Bench commit subjects use `probes:`. No public upstream filing in this cut.
